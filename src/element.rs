#![allow(private_bounds)]

use crate::{
	Context, CreateInnerInfo, CustomElement, ValidState,
	dynamic_element::{DynamicDiffer, DynamicElement},
	inner::{ElementInner, ElementInnerMap},
	mapped::Mapped,
};
use futures::FutureExt;
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	client::FrameInfo,
	spatial::{Spatial, SpatialExt, SpatialRef, Transform},
};
use std::{
	any::{TypeId, type_name_of_val},
	hash::{DefaultHasher, Hash, Hasher},
	marker::PhantomData,
	path::{Path, PathBuf},
	sync::OnceLock,
};
use tokio::sync::watch;
use tracing::debug_span;

fn element_type<E: std::any::Any>() -> &'static str {
	let type_name = std::any::type_name::<E>();
	// Cut off generics first
	let no_generics = type_name.find('<').map_or(type_name, |i| &type_name[..i]);
	// Now get after last ::
	no_generics
		.rfind("::")
		.map(|i| &no_generics[i + 2..])
		.unwrap_or(no_generics)
}

fn join_element_path<E: std::any::Any>(path: &Path, inner_key: u64) -> PathBuf {
	let segment = format!(
		"{}_{inner_key}",
		element_type::<E>(), // we want to get the element name without the namespace or generics
	);
	path.join(segment)
}
pub fn gen_inner_key<T: 'static>(parent_key: u64, local: usize) -> u64 {
	let mut hasher = DefaultHasher::new();
	parent_key.hash(&mut hasher);
	local.hash(&mut hasher);
	TypeId::of::<T>().hash(&mut hasher);
	hasher.finish()
}
pub fn hash_inner_key<T: 'static, H: Hash>(parent_key: u64, local: &H) -> u64 {
	let mut hasher = DefaultHasher::new();
	parent_key.hash(&mut hasher);
	local.hash(&mut hasher);
	TypeId::of::<T>().hash(&mut hasher);
	hasher.finish()
}

pub trait Element<State: ValidState>: ElementDiffer<State> + Sized + 'static {
	fn map<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut State> + Send + Sync + 'static,
	>(
		self,
		mapper: F,
	) -> Mapped<SuperState, State, F, Self> {
		Mapped::new(self, mapper)
	}
	/// Box as dynamic element for type swapping (rare cases like KDL)
	fn dynamic(self) -> DynamicElement<State>
	where
		Self: DynamicDiffer<State>,
	{
		DynamicElement::new(self)
	}
}
pub(crate) trait ElementDiffer<State: ValidState>:
	DynamicDiffer<State> + Send + Sync + 'static
{
	/// Create the inner imperative struct and all children
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	);

	/// Every frame on the server
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	);

	/// Fast path: diff against same type (zero-cost, fully optimized)
	#[allow(clippy::too_many_arguments)]
	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	);

	/// Clean up this element and all children
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
}

// HeapElement is not needed in the zero-cost abstraction approach
// Elements can be stored directly in the type system

// Tuple implementations for ElementDiffer to handle children structure
impl<State: ValidState> ElementDiffer<State> for () {
	fn create_inner_recursive(
		&mut self,
		_inner_key: u64,
		_context: &Context,
		_parent_space: watch::Receiver<Option<SpatialRef>>,
		_element_path: &Path,
		_inner_map: &mut ElementInnerMap,
	) {
	}
	fn frame_recursive(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner_map: &mut ElementInnerMap,
	) {
	}
	fn diff_same_type(
		&mut self,
		_inner_key: u64,
		_old: &Self,
		_context: &Context,
		_parent_space: &SpatialRef,
		_element_path: &Path,
		_inner_map: &mut ElementInnerMap,
	) {
		// Empty tuple - nothing to diff
	}
	fn destroy_inner_recursive(&self, _inner_map: &mut ElementInnerMap) {}
}

// For 2-tuples (the main case when adding children)
impl<State: ValidState, A: ElementDiffer<State>, B: ElementDiffer<State>> ElementDiffer<State>
	for (A, B)
{
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		// Create children with position-based keys
		let child_key_0 = gen_inner_key::<A>(inner_key, 0);
		self.0.create_inner_recursive(
			child_key_0,
			context,
			parent_space.clone(),
			element_path,
			inner_map,
		);
		let child_key_1 = gen_inner_key::<B>(inner_key, 1);
		self.1
			.create_inner_recursive(child_key_1, context, parent_space, element_path, inner_map);
	}
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		self.0.frame_recursive(context, info, state, inner_map);
		self.1.frame_recursive(context, info, state, inner_map);
	}
	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		// Same tuple type, diff each child with fast path
		let child_key_0 = gen_inner_key::<A>(inner_key, 0);
		self.0.diff_same_type(
			child_key_0,
			&old.0,
			context,
			parent_space,
			element_path,
			inner_map,
		);
		let child_key_1 = gen_inner_key::<B>(inner_key, 1);
		self.1.diff_same_type(
			child_key_1,
			&old.1,
			context,
			parent_space,
			element_path,
			inner_map,
		);
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.0.destroy_inner_recursive(inner_map);
		self.1.destroy_inner_recursive(inner_map);
	}
}

// Vec<Element> implementation - simple positional diffing
impl<State: ValidState, E: Element<State>> ElementDiffer<State> for Vec<E> {
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		for (i, element) in self.iter_mut().enumerate() {
			let child_key = gen_inner_key::<E>(inner_key, i);
			element.create_inner_recursive(
				child_key,
				context,
				parent_space.clone(),
				element_path,
				inner_map,
			);
		}
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		for element in self {
			element.frame_recursive(context, info, state, inner_map);
		}
	}

	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		let max_len = self.len().max(old.len());
		for i in 0..max_len {
			let new = self.get_mut(i);
			let old = old.get(i);

			match (new, old) {
				(Some(new), Some(old)) => {
					new.diff_same_type(
						gen_inner_key::<E>(inner_key, i),
						old,
						context,
						parent_space,
						element_path,
						inner_map,
					);
				}
				(Some(new), None) => {
					new.create_inner_recursive(
						gen_inner_key::<E>(inner_key, i),
						context,
						watch::channel(Some(parent_space.clone())).1,
						element_path,
						inner_map,
					);
				}
				(None, Some(old)) => {
					old.destroy_inner_recursive(inner_map);
				}
				(None, None) => {}
			}
		}
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		for element in self {
			element.destroy_inner_recursive(inner_map);
		}
	}
}

// HashMap<K, Element> implementation - stable key diffing
impl<State: ValidState, K: Hash + Eq + Clone + Send + Sync + 'static, E: Element<State>>
	ElementDiffer<State> for FxHashMap<K, E>
{
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		for (key, element) in self {
			element.create_inner_recursive(
				hash_inner_key::<E, K>(inner_key, key),
				context,
				parent_space.clone(),
				element_path,
				inner_map,
			);
		}
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		for element in self.values() {
			element.frame_recursive(context, info, state, inner_map);
		}
	}

	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		// Process all new elements (update existing or create new)
		for (key, new_elem) in self.iter_mut() {
			let child_key = hash_inner_key::<E, K>(inner_key, key);

			match old.get(key) {
				Some(old_elem) => {
					// Update existing element
					new_elem.diff_same_type(
						child_key,
						old_elem,
						context,
						parent_space,
						element_path,
						inner_map,
					);
				}
				None => {
					// Create new element
					new_elem.create_inner_recursive(
						child_key,
						context,
						watch::channel(Some(parent_space.clone())).1,
						element_path,
						inner_map,
					);
				}
			}
		}

		// Destroy elements that were in old but not in new
		for (key, old_elem) in old {
			if !self.contains_key(key) {
				old_elem.destroy_inner_recursive(inner_map);
			}
		}
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		for element in self.values() {
			element.destroy_inner_recursive(inner_map);
		}
	}
}

// Option<Element> implementation
impl<State: ValidState, E: Element<State>> ElementDiffer<State> for Option<E> {
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		if let Some(element) = self {
			// Option uses the same key as the parent - the element inside Option manages its own key
			element.create_inner_recursive(
				inner_key,
				context,
				parent_space,
				element_path,
				inner_map,
			);
		}
	}
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		if let Some(element) = self {
			element.frame_recursive(context, info, state, inner_map);
		}
	}
	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		match (self, old) {
			(Some(new), Some(old)) => {
				// Both present, diff them using the same key
				new.diff_same_type(
					inner_key,
					old,
					context,
					parent_space,
					element_path,
					inner_map,
				);
			}
			(Some(new), None) => {
				// New element, create it
				new.create_inner_recursive(
					inner_key,
					context,
					watch::channel(Some(parent_space.clone())).1,
					element_path,
					inner_map,
				);
			}
			(None, Some(old)) => {
				// Element removed, destroy it
				old.destroy_inner_recursive(inner_map);
			}
			(None, None) => {
				// Both None, nothing to do
			}
		}
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		if let Some(element) = self {
			element.destroy_inner_recursive(inner_map);
		}
	}
}

pub struct ElementWrapper<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>> {
	pub custom_element: Option<E>,
	children: C,
	inner_key: OnceLock<u64>,
	state_phantom: PhantomData<State>,
}

impl<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>>
	ElementWrapper<State, E, C>
{
	pub(crate) fn new(custom_element: E) -> ElementWrapper<State, E, ()> {
		ElementWrapper {
			custom_element: Some(custom_element),
			children: (),
			inner_key: OnceLock::new(),
			state_phantom: PhantomData,
		}
	}
	pub fn child<NC: Element<State>>(self, child: NC) -> ElementWrapper<State, E, (C, NC)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, child),
			inner_key: self.inner_key,
			state_phantom: PhantomData,
		}
	}
	pub fn maybe_child<NC: Element<State>>(
		self,
		child: Option<NC>,
	) -> ElementWrapper<State, E, (C, Option<NC>)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, child),
			inner_key: self.inner_key,
			state_phantom: PhantomData,
		}
	}
	pub fn children<NC: Element<State>>(
		self,
		children: impl IntoIterator<Item = NC>,
	) -> ElementWrapper<State, E, (C, Vec<NC>)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, children.into_iter().collect()),
			inner_key: self.inner_key,
			state_phantom: PhantomData,
		}
	}
	pub fn stable_children<NC: Element<State>, K: Eq + Hash + Clone + Send + Sync + 'static>(
		self,
		children: impl IntoIterator<Item = (K, NC)>,
	) -> ElementWrapper<State, E, (C, FxHashMap<K, NC>)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, FxHashMap::from_iter(children)),
			inner_key: self.inner_key,
			state_phantom: PhantomData,
		}
	}
}
impl<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>> ElementDiffer<State>
	for ElementWrapper<State, E, C>
{
	#[tracing::instrument(level = "debug", skip(self, context, parent_space, inner_map))]
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		let _span = tracing::debug_span!(
			"Create inner",
			inner_key,
			parent_space = ?parent_space.borrow().clone(),
			?element_path,
		);
		let _span_guard = _span.enter();
		let element_path = join_element_path::<E>(element_path, inner_key);

		// Store the inner key for later use in frame/destroy
		let _ = self.inner_key.set(inner_key);

		let (child_space_tx, child_space_rx) = watch::channel(None);

		if let Some(element) = self.custom_element.take() {
			// Create this element's inner
			let task = tokio::task::spawn({
				let mut parent_space = parent_space.clone();
				let element_path = element_path.to_path_buf();
				let context = context.clone();
				async move {
					let parent_space = parent_space
						.wait_for(|s| s.is_some())
						.await
						.as_deref()
						.cloned()
						.unwrap()
						.unwrap()
						.clone();
					let (child_space, child_spatial_ref) = Spatial::create(
						&context.stardust_client,
						&parent_space,
						Transform::IDENTITY,
					)
					.await
					.unwrap();
					let _ = child_space_tx.send(Some(child_spatial_ref.clone()));

					let result = element
						.create_inner(
							&context,
							CreateInnerInfo {
								parent_space,
								child_space,
								element_path,
							},
						)
						.await;
					(element, result, child_spatial_ref)
				}
			});
			inner_map.insert::<State, E>(inner_key, task);
		}

		// Create children
		self.children.create_inner_recursive(
			inner_key,
			context,
			child_space_rx,
			&element_path,
			inner_map,
		);
	}

	#[tracing::instrument(level = "debug", skip(self, context, state, inner_map))]
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		// Call frame on this element using the stored inner key
		if let Some(element) = &self.custom_element {
			if let Some(&inner_key) = self.inner_key.get() {
				if let Some(ElementInner::Done(inner, _)) = inner_map.get_mut::<State, E>(inner_key)
				{
					let element_type = type_name_of_val(&element);
					let _guard = debug_span!("Element frame", ?element_type).entered();
					element.frame(context, info, state, inner);
				}
			}
		}

		// Call frame on children
		self.children
			.frame_recursive(context, info, state, inner_map);
	}

	#[tracing::instrument(level = "debug", skip(self, old, context, parent_space, inner_map))]
	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		let element_path = join_element_path::<E>(element_path, inner_key);

		// Store the inner key for later use in frame/destroy
		let _ = self.inner_key.set(inner_key);

		// Diff this element
		if let Some(new_element) = &self.custom_element
			&& let Some(old_element) = &old.custom_element
		{
			let Some(inner_mut) = inner_map.get_mut::<State, E>(inner_key) else {
				return;
			};
			match inner_mut {
				ElementInner::Creating(result) => {
					// if the creation of async stuff is done, rip it out of the creating,
					// block on it (should return immediately), then slap it in the done pile
					if result.as_mut().is_some_and(|r| r.is_finished()) {
						let Some(result) = result.take() else { return };
						// TODO: don't depend on a whole crate just for this 1 function
						match result.now_or_never() {
							Some(Ok((decl, result, child_spatial))) => {
								*inner_mut = match result {
									Ok(mut element) => {
										// diff from the stored state at creation to the most recent creation
										// since async stuff *could* take several frames
										new_element.diff(&decl, context, &mut element);
										ElementInner::Done(element, child_spatial)
									}
									Err(err) => ElementInner::Error(err),
								};
							}
							_ => return,
						};
					}
				}
				ElementInner::Done(inner, _) => {
					new_element.diff(old_element, context, inner);
				}
				_ => (),
			}
		}

		// Get spatial ref for children
		let child_parent_space = if let Some(ElementInner::Done(_, spatial_ref)) =
			inner_map.get::<State, E>(inner_key)
		{
			spatial_ref.clone()
		} else {
			parent_space.clone()
		};

		// Diff children
		self.children.diff_same_type(
			inner_key,
			&old.children,
			context,
			&child_parent_space,
			&element_path,
			inner_map,
		);
	}

	#[tracing::instrument(level = "debug", skip(self, inner_map))]
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		// Destroy children first
		self.children.destroy_inner_recursive(inner_map);

		// Destroy this element using the stored inner key
		if let Some(&inner_key) = self.inner_key.get() {
			inner_map.remove(inner_key);
		}
	}
}

impl<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>> Element<State>
	for ElementWrapper<State, E, C>
{
}
