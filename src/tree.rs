use crate::{
	Context, CreateInnerInfo, CustomElement, ElementFlattener, ResourceRegistry, ValidState,
	inner::{ElementInnerKey, ElementInnerMap},
};
use bumpalo::{Bump, boxed::Box, collections::Vec};
use ouroboros::self_referencing;
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
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
		let current = Tree::flatten(Bump::new(), blueprint).unwrap();
		current.borrow_root().create_inner_recursive(
			context, // Use provided context
			CreateInnerInfo {
				parent_space,
				element_path: &root_element_path,
			},
			inner_map,
			resource_registry,
		);
		Self {
			root_element_path,
			current,
			old: None,
		}
	}
	pub fn frame(&mut self, info: &FrameInfo, state: &mut State, inner_map: &mut ElementInnerMap) {
		self.current
			.borrow_root()
			.frame_recursive(info, state, inner_map);
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
		let current_root = self.current.borrow_root();
		let old_root = self.old.as_ref().unwrap().borrow_root();

		// Start diffing from the roots, using a dummy parent spatial for the root level
		current_root.diff_and_apply(
			parent_space,
			&**old_root,
			context, // Use provided context
			&self.root_element_path,
			inner_map,
			resource_registry,
		);
	}
}

pub(crate) trait ElementDiffer<State: ValidState> {
	fn type_id(&self) -> TypeId;

	/// Create the inner imperative struct
	fn create_inner_recursive(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	);
	/// Every frame on the server
	fn frame_recursive(
		&self,
		_info: &FrameInfo,
		_state: &mut State,
		_inner_map: &mut ElementInnerMap,
	);
	fn diff_and_apply(
		&self,
		parent_space: &SpatialRef,
		old: &dyn ElementDiffer<State>,
		context: &Context,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);

	/// Recursively assign IDs to elements that don't have explicit IDs
	fn assign_id_recursive(&mut self, parent_id: u64, position: usize);
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

fn join_element_path<E: std::any::Any>(path: &Path, id: Option<ElementInnerKey>) -> PathBuf {
	let segment = format!(
		"{}_{}",
		element_type_name::<E>(), // we want to get the element name without the namespace or generics
		id.map(|i| i.0).unwrap_or(0)
	);
	path.join(segment)
}

pub struct FlatElement<'a, State: ValidState, E: CustomElement<State>> {
	pub(crate) element: E,
	pub(crate) children: Vec<'a, Box<'a, dyn ElementDiffer<State>>>,
	// only local for now
	pub(crate) id: Option<ElementInnerKey>,
	pub(crate) phantom: PhantomData<State>,
}
impl<'a, State: ValidState, E: CustomElement<State>> ElementDiffer<State>
	for FlatElement<'a, State, E>
{
	fn type_id(&self) -> TypeId {
		TypeId::of::<E>()
	}

	fn create_inner_recursive(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	) {
		let CreateInnerInfo {
			parent_space,
			element_path,
		} = info;

		let element_path = join_element_path::<E>(element_path, self.id);

		// Create our inner element and get the ID
		if let Some(id) = self.id {
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
				inner_map.insert::<State, E>(id, inner);
			}
		}

		// Get our spatial ref to use as parent for children
		let spatial = if let Some(id) = self.id {
			if let Some(inner) = inner_map.get::<State, E>(id) {
				self.element.spatial_aspect(inner)
			} else {
				parent_space.clone()
			}
		} else {
			parent_space.clone()
		};

		// Recursively create children under our spatial aspect
		for child in &self.children {
			child.create_inner_recursive(
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
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		// If we have an ID, call frame on our element
		if let Some(id) = self.id {
			if let Some(inner) = inner_map.get_mut::<State, E>(id) {
				self.element.frame(info, state, inner);
			}
		}

		for child in &self.children {
			child.frame_recursive(info, state, inner_map);
		}
	}

	fn diff_and_apply(
		&self,
		parent_space: &SpatialRef,
		old: &dyn ElementDiffer<State>,
		context: &Context,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let element_path = join_element_path::<E>(element_path, self.id);

		// Check if the old element has the same type
		if self.type_id() != old.type_id() {
			// Types don't match, destroy old and create new
			old.destroy_inner_recursive(inner_map);
			self.create_inner_recursive(
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

		// If we have an ID, update the element
		if let Some(id) = self.id {
			if let Some(old_id) = old_flat.id {
				// Get the inner element first
				let inner_opt = inner_map.get_mut::<State, E>(id);

				if let Some(inner) = inner_opt {
					// Update our element with the old one
					self.element
						.diff(&old_flat.element, inner, resources.get::<State, E>());
				} else if inner_map.get::<State, E>(old_id).is_some() {
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
						inner_map.insert::<State, E>(id, inner);
					}
				}
			}
		}

		// Get our spatial ref to use as parent for children
		let spatial = if let Some(id) = self.id {
			if let Some(inner) = inner_map.get::<State, E>(id) {
				self.element.spatial_aspect(inner)
			} else {
				parent_space.clone()
			}
		} else {
			parent_space.clone()
		};

		// Compare and update children
		for (i, child) in self.children.iter().enumerate() {
			if i < old_flat.children.len() {
				// If there's a matching child in the old tree, diff against it
				let old_child = &*old_flat.children[i];

				// Only diff if types match, otherwise recreate
				if child.type_id() == old_child.type_id() {
					child.diff_and_apply(
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
		// First destroy all children
		for child in &self.children {
			child.destroy_inner_recursive(inner_map);
		}

		// Then destroy self if we have an ID
		if let Some(id) = self.id {
			// Even though CustomElement doesn't have a destroy method,
			// we should ensure the entry is removed properly

			// Remove from inner map - this will drop the boxed inner value
			inner_map.remove(&id);

			// Could add debugging here if needed
			// if no_element {
			//     // Element was already removed or never existed
			// }
		}
	}

	fn assign_id_recursive(&mut self, parent_id: u64, position: usize) {
		// Only assign an ID if one hasn't been explicitly set
		if self.id.is_none() {
			// Create stable ID based on parent ID, position, and type
			let mut hasher = DefaultHasher::new();
			parent_id.hash(&mut hasher);
			position.hash(&mut hasher);
			TypeId::of::<E>().hash(&mut hasher);
			self.id = Some(ElementInnerKey(hasher.finish()));
		}

		// Recursively assign IDs to children with our ID as parent
		let our_id = self.id.map(|id| id.0).unwrap_or(0);
		for (i, child) in self.children.iter_mut().enumerate() {
			// Need to cast to mut pointer since we can't borrow child as mutable directly
			let child_ptr: *mut dyn ElementDiffer<State> = &mut **child;
			unsafe {
				(*child_ptr).assign_id_recursive(our_id, i);
			}
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
	fn create_inner_recursive(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		inner_map: &mut ElementInnerMap,
		resource_registry: &mut ResourceRegistry,
	) {
		self.wrapped
			.create_inner_recursive(asteroids_context, info, inner_map, resource_registry);
	}

	fn frame_recursive(
		&self,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		if let Some(mapped_state) = (self.mapper)(state) {
			self.wrapped.frame_recursive(info, mapped_state, inner_map);
		}
	}

	fn diff_and_apply(
		&self,
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

	fn assign_id_recursive(&mut self, parent_id: u64, position: usize) {
		self.wrapped.assign_id_recursive(parent_id, position);
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
		let mut tree = Self::try_new(bump, move |bump| {
			let root = root.flatten(bump);
			root.into_iter().next().ok_or(())
		})
		.ok()?;

		// Assign IDs to elements that don't have explicit IDs
		tree.with_root_mut(|root| {
			// Get a mutable pointer to the root
			let root_ptr: *mut dyn ElementDiffer<State> = &mut **root;
			// Assign IDs starting from the root with parent_id=0, position=0
			unsafe {
				(*root_ptr).assign_id_recursive(0, 0);
			}
		});

		Some(tree)
	}
}
