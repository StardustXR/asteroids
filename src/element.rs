#![allow(private_bounds)]

use crate::{
	Context, CreateInnerInfo, CustomElement, ValidState,
	dynamic_element::{DynamicDiffer, DynamicElement},
	inner::ElementInnerMap,
	mapped::Mapped,
	resource::ResourceRegistry,
};
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::{
	any::TypeId,
	hash::{DefaultHasher, Hash, Hasher},
	marker::PhantomData,
	path::Path,
	sync::OnceLock,
};

// Helper functions for generating keys
pub fn generate_keyed_inner_key<T: 'static>(parent_key: u64, stable_id: u64) -> u64 {
	let mut hasher = DefaultHasher::new();
	parent_key.hash(&mut hasher);
	stable_id.hash(&mut hasher);
	TypeId::of::<T>().hash(&mut hasher);
	hasher.finish()
}

pub fn generate_positional_inner_key<T: 'static>(parent_key: u64, position: usize) -> u64 {
	let mut hasher = DefaultHasher::new();
	parent_key.hash(&mut hasher);
	position.hash(&mut hasher);
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
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
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
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);

	/// Clean up this element and all children
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
}

pub trait Identifiable {
	fn identify<H: Hash>(self, h: &H) -> Self;
}

// HeapElement is not needed in the zero-cost abstraction approach
// Elements can be stored directly in the type system

// Tuple implementations for ElementDiffer to handle children structure
impl<State: ValidState> ElementDiffer<State> for () {
	fn create_inner_recursive(
		&self,
		_inner_key: u64,
		_context: &Context,
		_parent_space: &SpatialRef,
		_element_path: &Path,
		_inner_map: &mut ElementInnerMap,
		_resources: &mut ResourceRegistry,
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
		&self,
		_inner_key: u64,
		_old: &Self,
		_context: &Context,
		_parent_space: &SpatialRef,
		_element_path: &Path,
		_inner_map: &mut ElementInnerMap,
		_resources: &mut ResourceRegistry,
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
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Create children with position-based keys
		let child_key_0 = generate_positional_inner_key::<A>(inner_key, 0);
		self.0.create_inner_recursive(
			child_key_0,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		);
		let child_key_1 = generate_positional_inner_key::<B>(inner_key, 1);
		self.1.create_inner_recursive(
			child_key_1,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		);
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
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Same tuple type, diff each child with fast path
		let child_key_0 = generate_positional_inner_key::<A>(inner_key, 0);
		self.0.diff_same_type(
			child_key_0,
			&old.0,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		);
		let child_key_1 = generate_positional_inner_key::<B>(inner_key, 1);
		self.1.diff_same_type(
			child_key_1,
			&old.1,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		);
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.0.destroy_inner_recursive(inner_map);
		self.1.destroy_inner_recursive(inner_map);
	}
}
// We only need () and (A, B) tuples for the element children pattern

// Vec<Element> implementation
impl<State: ValidState, E: Element<State>> ElementDiffer<State> for Vec<E> {
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		for (i, element) in self.iter().enumerate() {
			let child_key = generate_positional_inner_key::<E>(inner_key, i);
			element.create_inner_recursive(
				child_key,
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
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
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Same Vec type, diff the vectors
		let min_len = self.len().min(old.len());

		// Diff common elements
		for i in 0..min_len {
			let child_key = generate_positional_inner_key::<E>(inner_key, i);
			self[i].diff_same_type(
				child_key,
				&old[i],
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
			);
		}

		// Handle extra elements in old (destroy)
		for old_elem in old.iter().skip(min_len) {
			old_elem.destroy_inner_recursive(inner_map);
		}

		// Handle extra elements in new (create)
		for (i, elem) in self.iter().enumerate().skip(min_len) {
			let child_key = generate_positional_inner_key::<E>(inner_key, i);
			elem.create_inner_recursive(
				child_key,
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
			);
		}
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		for element in self {
			element.destroy_inner_recursive(inner_map);
		}
	}
}

// Option<Element> implementation
impl<State: ValidState, E: Element<State>> ElementDiffer<State> for Option<E> {
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		if let Some(element) = self {
			// Option uses the same key as the parent - the element inside Option manages its own key
			element.create_inner_recursive(
				inner_key,
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
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
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
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
					resources,
				);
			}
			(Some(new), None) => {
				// New element, create it
				new.create_inner_recursive(
					inner_key,
					context,
					parent_space,
					element_path,
					inner_map,
					resources,
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
	pub stable_id: Option<u64>,
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
			stable_id: None,
			inner_key: OnceLock::new(),
			state_phantom: PhantomData,
		}
	}
	pub fn child<NC: Element<State>>(self, child: NC) -> ElementWrapper<State, E, (C, NC)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, child),
			stable_id: self.stable_id,
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
			stable_id: self.stable_id,
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
			stable_id: self.stable_id,
			inner_key: self.inner_key,
			state_phantom: PhantomData,
		}
	}
}
impl<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>> ElementDiffer<State>
	for ElementWrapper<State, E, C>
{
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Store the inner key for later use in frame/destroy
		let _ = self.inner_key.set(inner_key);

		// Create this element's inner
		if let Some(element) = &self.custom_element {
			let result = element.create_inner(
				context,
				CreateInnerInfo {
					parent_space,
					element_path,
				},
				resources.get::<State, E>(),
			);

			if let Ok(inner) = result {
				inner_map.insert::<State, E>(inner_key, inner);
			}
		}

		// Get spatial ref for children
		let child_parent_space = if let Some(element) = &self.custom_element {
			if let Some(inner) = inner_map.get::<State, E>(inner_key) {
				element.spatial_aspect(inner)
			} else {
				parent_space.clone()
			}
		} else {
			parent_space.clone()
		};

		// Create children
		self.children.create_inner_recursive(
			inner_key,
			context,
			&child_parent_space,
			element_path,
			inner_map,
			resources,
		);
	}

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
				if let Some(inner) = inner_map.get_mut::<State, E>(inner_key) {
					element.frame(context, info, state, inner);
				}
			}
		}

		// Call frame on children
		self.children
			.frame_recursive(context, info, state, inner_map);
	}

	fn diff_same_type(
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Store the inner key for later use in frame/destroy
		let _ = self.inner_key.set(inner_key);

		// Diff this element
		match (&self.custom_element, &old.custom_element) {
			(Some(new_element), Some(old_element)) => {
				if let Some(inner) = inner_map.get_mut::<State, E>(inner_key) {
					new_element.diff(old_element, inner, resources.get::<State, E>());
				}
			}
			(Some(_), None) => {
				// New element, create it
				ElementDiffer::create_inner_recursive(
					self,
					inner_key,
					context,
					parent_space,
					element_path,
					inner_map,
					resources,
				);
				return; // Don't diff children since we just created everything
			}
			(None, Some(_)) => {
				// Element removed, destroy it
				ElementDiffer::destroy_inner_recursive(old, inner_map);
				return; // Don't diff children since we destroyed everything
			}
			(None, None) => {
				// Both None, nothing to do for this element
			}
		}

		// Get spatial ref for children
		let child_parent_space = if let Some(element) = &self.custom_element {
			if let Some(inner) = inner_map.get::<State, E>(inner_key) {
				element.spatial_aspect(inner)
			} else {
				parent_space.clone()
			}
		} else {
			parent_space.clone()
		};

		// Diff children
		self.children.diff_same_type(
			inner_key,
			&old.children,
			context,
			&child_parent_space,
			element_path,
			inner_map,
			resources,
		);
	}

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

impl<State: ValidState, E: CustomElement<State>, C: ElementDiffer<State>> Identifiable
	for ElementWrapper<State, E, C>
{
	fn identify<H: Hash>(mut self, h: &H) -> Self {
		let mut hasher = DefaultHasher::new();
		h.hash(&mut hasher);
		let key = hasher.finish();
		self.stable_id.replace(key);
		self
	}
}
