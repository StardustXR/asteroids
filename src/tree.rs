use crate::{
	Context, CreateInnerInfo, CustomElement, ElementFlattener, ResourceRegistry, ValidState,
	inner::ElementInnerMap,
};
use bumpalo::{Bump, boxed::Box, collections::Vec};
use ouroboros::self_referencing;
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::sync::OnceLock;
use std::{any::TypeId, marker::PhantomData, path::Path};
use std::{
	hash::{DefaultHasher, Hash, Hasher},
	path::PathBuf,
};

pub struct Trees<State: ValidState> {
	root_element_path: PathBuf,
	current: Tree<State>,
	old: Option<Tree<State>>,
}
impl<State: ValidState> Trees<State> {
	pub fn new<E: ElementFlattener<State>>(
		blueprint: E,
		context: &Context,
		parent_space: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
		root_element_path: PathBuf,
	) -> Self {
		let mut current = Tree::flatten(Bump::new(), blueprint).unwrap();
		current.with_root_mut(|root| {
			let id = root.inner_key(0, 0);
			root.create_inner_recursive(
				id,
				context, // Use provided context
				CreateInnerInfo {
					parent_space,
					element_path: &root_element_path,
				},
				inner_map,
				resource_registry,
			)
		});
		Self {
			root_element_path,
			current,
			old: None,
		}
	}
	pub fn frame(
		&mut self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		self.current
			.borrow_root()
			.frame_recursive(context, info, state, inner_map);
	}
	pub fn diff_and_apply<E: ElementFlattener<State>>(
		&mut self,
		new_blueprint: E,
		context: &Context,
		parent_space: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	) {
		// rip the old one apart to get its bump to avoid reallocation
		let mut old_bump = self
			.old
			.take()
			.map(|o| o.into_heads().bump)
			.unwrap_or_default();
		old_bump.reset();
		// make the new tree
		let new_tree = Tree::flatten(old_bump, new_blueprint).unwrap();
		// now replace the current tree with the new one
		let old_tree = std::mem::replace(&mut self.current, new_tree);
		// and the old tree gets put there for diffing
		self.old.replace(old_tree);

		// Get root elements from both trees
		let old_root = self.old.as_ref().unwrap().borrow_root();
		self.current.with_root_mut(|root| {
			// Start diffing from the roots, using a dummy parent spatial for the root level
			let id = root.inner_key(0, 0);
			root.diff_and_apply(
				id,
				parent_space,
				&**old_root,
				context, // Use provided context
				&self.root_element_path,
				inner_map,
				resource_registry,
			);
		});
	}
}

pub(crate) trait ElementDiffer<State: ValidState> {
	fn type_id(&self) -> TypeId;
	fn inner_key(&self, parent_inner_key: u64, position: usize) -> u64;

	/// Create the inner imperative struct
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	);
	/// Every frame on the server
	fn frame_recursive(
		&self,
		context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner_map: &mut ElementInnerMap,
	);
	#[allow(clippy::too_many_arguments)]
	fn diff_and_apply(
		&self,
		inner_key: u64,
		parent_space: &SpatialRef,
		old: &dyn ElementDiffer<State>,
		context: &Context,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
}

fn element_type_name<E: std::any::Any>() -> &'static str {
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
		element_type_name::<E>(), // we want to get the element name without the namespace or generics
	);
	path.join(segment)
}

pub struct FlatElement<'a, State: ValidState, E: CustomElement<State>> {
	pub(crate) element: E,
	pub(crate) children: Vec<'a, Box<'a, dyn ElementDiffer<State>>>,
	pub(crate) stable_local_id: Option<u64>,
	pub(crate) inner_key: OnceLock<u64>,
	pub(crate) phantom: PhantomData<State>,
}
impl<'a, State: ValidState, E: CustomElement<State>> ElementDiffer<State>
	for FlatElement<'a, State, E>
{
	fn type_id(&self) -> TypeId {
		TypeId::of::<E>()
	}

	fn inner_key(&self, parent_inner_key: u64, position: usize) -> u64 {
		*self.inner_key.get_or_init(|| {
			// Create stable ID based on parent ID, position, and type
			let mut hasher = DefaultHasher::new();
			parent_inner_key.hash(&mut hasher);
			if let Some(id) = self.stable_local_id {
				id.hash(&mut hasher);
			} else {
				position.hash(&mut hasher);
			};
			TypeId::of::<E>().hash(&mut hasher);
			hasher.finish()
		})
	}

	fn create_inner_recursive(
		&self,
		inner_key: u64,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	) {
		let CreateInnerInfo {
			parent_space,
			element_path,
		} = info;

		let element_path = join_element_path::<E>(element_path, inner_key);

		// Create our inner element and get the ID
		let inner = self.element.create_inner(
			asteroids_context,
			CreateInnerInfo {
				parent_space,
				element_path: &element_path,
			},
			resource_registry.get::<State, E>(),
		);

		// Store inner in the map using our ID
		if let Ok(inner) = inner {
			inner_map.insert::<State, E>(inner_key, inner);
		}

		// Get our spatial ref to use as parent for children
		let spatial = if let Some(inner) = inner_map.get::<State, E>(inner_key) {
			self.element.spatial_aspect(inner)
		} else {
			parent_space.clone()
		};

		// Recursively create children under our spatial aspect
		for (i, child) in self.children.iter().enumerate() {
			let id = child.inner_key(inner_key, i);
			child.create_inner_recursive(
				id,
				asteroids_context,
				CreateInnerInfo {
					parent_space: &spatial,
					element_path: &element_path,
				},
				inner_map,
				resource_registry,
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
		// If we have an ID, call frame on our element
		if let Some(inner_key) = self.inner_key.get().cloned() {
			if let Some(inner) = inner_map.get_mut::<State, E>(inner_key) {
				self.element.frame(context, info, state, inner);
			}
		}

		for child in &self.children {
			child.frame_recursive(context, info, state, inner_map);
		}
	}

	fn diff_and_apply(
		&self,
		inner_key: u64,
		parent_space: &SpatialRef,
		old: &dyn ElementDiffer<State>,
		context: &Context,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let element_path = join_element_path::<E>(element_path, inner_key);

		// Check if the old element has the same type
		if self.type_id() != old.type_id() {
			// Types don't match, destroy old and create new
			old.destroy_inner_recursive(inner_map);
			self.create_inner_recursive(
				inner_key,
				context,
				CreateInnerInfo {
					parent_space,
					element_path: &element_path,
				},
				inner_map,
				resources,
			);
			return;
		}

		// Types match, so we can downcast and update
		let old_flat =
			unsafe { &*(old as *const dyn ElementDiffer<State> as *const FlatElement<State, E>) };

		// If we have an inner key, update the element
		if let Some(old_inner_key) = old_flat.inner_key.get().cloned() {
			// Get the inner element first
			let inner_opt = inner_map.get_mut::<State, E>(old_inner_key);

			if let Some(inner) = inner_opt {
				// Update our element with the old one
				self.element
					.diff(&old_flat.element, inner, resources.get::<State, E>());
			} else if inner_map.get::<State, E>(old_inner_key).is_some() {
				// We have the old element but not the new one yet, so create it
				let inner_result = self.element.create_inner(
					context,
					CreateInnerInfo {
						parent_space,
						element_path: &element_path,
					},
					resources.get::<State, E>(),
				);

				if let Ok(inner) = inner_result {
					inner_map.insert::<State, E>(inner_key, inner);
					inner_map.remove(old_inner_key);
				}
			}
		}

		// Get our spatial ref to use as parent for children
		let spatial = if let Some(inner) = inner_map.get::<State, E>(inner_key) {
			self.element.spatial_aspect(inner)
		} else {
			parent_space.clone()
		};

		// Compare and update children
		for (i, child) in self.children.iter().enumerate() {
			let child_inner_key = child.inner_key(inner_key, i);
			if i < old_flat.children.len() {
				// If there's a matching child in the old tree, diff against it
				let old_child = &*old_flat.children[i];

				// Only diff if types match, otherwise recreate
				if child.type_id() == old_child.type_id() {
					child.diff_and_apply(
						child_inner_key,
						&spatial,
						old_child,
						context,
						&element_path,
						inner_map,
						resources,
					);
				} else {
					// Types don't match, destroy old and create new
					old_child.destroy_inner_recursive(inner_map);
					child.create_inner_recursive(
						child_inner_key,
						context,
						CreateInnerInfo {
							parent_space: &spatial,
							element_path: &element_path,
						},
						inner_map,
						resources,
					);
				}
			} else {
				// This is a new child, create it
				child.create_inner_recursive(
					child_inner_key,
					context,
					CreateInnerInfo {
						parent_space: &spatial,
						element_path: &element_path,
					},
					inner_map,
					resources,
				);
			}
		}

		// Handle any remaining old children that need to be removed
		for i in self.children.len()..old_flat.children.len() {
			old_flat.children[i].destroy_inner_recursive(inner_map);
		}
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		// destroy them first to remove lingering references in server
		for child in &self.children {
			child.destroy_inner_recursive(inner_map);
		}

		if let Some(id) = self.inner_key.get().cloned() {
			inner_map.remove(id);
		}
	}
}

pub struct FlatmapElement<
	'a,
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState>,
> {
	pub(crate) wrapped: Box<'a, dyn ElementDiffer<WrappedState>>,
	pub(crate) mapper: F,
	pub(crate) phantom: PhantomData<State>,
}
impl<
	'a,
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState>,
> ElementDiffer<State> for FlatmapElement<'a, State, WrappedState, F>
{
	fn type_id(&self) -> TypeId {
		// Return wrapped element's TypeId since that's what we're primarily diffing
		self.wrapped.type_id()
	}
	fn inner_key(&self, parent_inner_key: u64, position: usize) -> u64 {
		self.wrapped.inner_key(parent_inner_key, position)
	}
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	) {
		self.wrapped.create_inner_recursive(
			inner_key,
			asteroids_context,
			info,
			inner_map,
			resource_registry,
		);
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		if let Some(mapped_state) = (self.mapper)(state) {
			self.wrapped
				.frame_recursive(context, info, mapped_state, inner_map);
		}
	}

	fn diff_and_apply(
		&self,
		inner_key: u64,
		parent_spatial: &SpatialRef,
		old: &dyn ElementDiffer<State>,
		context: &Context,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		if ElementDiffer::type_id(self) != old.type_id() {
			// Instead of panicking, we should handle the type mismatch gracefully
			// Destroy the old element and create ourselves from scratch
			old.destroy_inner_recursive(inner_map);
			self.create_inner_recursive(
				inner_key,
				context,
				CreateInnerInfo {
					parent_space: parent_spatial,
					element_path,
				},
				inner_map,
				resources,
			);
			return;
		}

		// have to downcast manually since we cannot use any due to lifetime
		let old_self = unsafe { &*(old as *const dyn ElementDiffer<State> as *const Self) };

		// Try to map the state, but handle the case where mapping fails
		self.wrapped.diff_and_apply(
			inner_key,
			parent_spatial,
			(&*old_self.wrapped) as &dyn ElementDiffer<WrappedState>,
			context,
			element_path,
			inner_map,
			resources,
		);
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.wrapped.destroy_inner_recursive(inner_map);
	}
}

#[self_referencing]
pub struct Tree<State: ValidState> {
	bump: Bump,
	#[borrows(bump)]
	#[covariant]
	root: Box<'this, dyn ElementDiffer<State>>,
}
impl<State: ValidState> Tree<State> {
	pub fn flatten(bump: Bump, mut root: impl ElementFlattener<State>) -> Option<Self> {
		Self::try_new(bump, move |bump| {
			let root = root.flatten(bump);
			root.into_iter().next().ok_or(())
		})
		.ok()
	}
}
