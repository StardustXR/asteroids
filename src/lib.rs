use custom::ElementTrait;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{
	root::FrameInfo,
	spatial::{Spatial, SpatialRef, SpatialRefAspect, Transform},
};
use std::{
	any::{Any, TypeId},
	collections::hash_map::DefaultHasher,
	fmt::Debug,
	hash::{Hash, Hasher},
	marker::PhantomData,
	sync::OnceLock,
};
use zbus::Connection;

pub mod client;
pub mod custom;
pub mod elements;

pub trait ValidState: Sized + Send + Sync + 'static {}
impl<T: Sized + Send + Sync + 'static> ValidState for T {}

pub trait Reify: ValidState + Sized + Send + Sync + 'static {
	fn reify(&self) -> Element<Self>;

	fn reify_substate<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut Self> + Send + Sync + 'static,
	>(
		&self,
		mapper: F,
	) -> Element<SuperState> {
		self.reify().map(mapper)
	}
}

pub struct DeltaSet<T: Clone + Hash + Eq> {
	added: FxHashSet<T>,
	current: FxHashSet<T>,
	removed: FxHashSet<T>,
}
impl<T: Clone + Hash + Eq> Default for DeltaSet<T> {
	fn default() -> Self {
		DeltaSet {
			added: Default::default(),
			current: Default::default(),
			removed: Default::default(),
		}
	}
}
impl<T: Clone + Hash + Eq + Debug> Debug for DeltaSet<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("DeltaSet")
			.field("added", &self.added)
			.field("current", &self.current)
			.field("removed", &self.removed)
			.finish()
	}
}
impl<T: Clone + Hash + Eq> DeltaSet<T> {
	pub fn push_new(&mut self, new: impl Iterator<Item = T>) {
		let new = FxHashSet::from_iter(new);
		self.added = FxHashSet::from_iter(new.difference(&self.current).cloned());
		self.removed = FxHashSet::from_iter(self.current.difference(&new).cloned());
		self.current = new;
	}
	pub fn added(&self) -> &FxHashSet<T> {
		&self.added
	}
	pub fn current(&self) -> &FxHashSet<T> {
		&self.current
	}
	pub fn removed(&self) -> &FxHashSet<T> {
		&self.removed
	}
}

#[derive(Default)]
pub(crate) struct ResourceRegistry(FxHashMap<TypeId, Box<dyn Any>>);
impl ResourceRegistry {
	fn get<R: Default + Send + Sync + 'static>(&mut self) -> &mut R {
		let type_id = TypeId::of::<R>();
		self.0
			.entry(type_id)
			.or_insert_with(|| Box::new(R::default()))
			.downcast_mut::<R>()
			.unwrap()
	}
}

pub struct View<State: Reify> {
	_root: Spatial,
	dbus_connection: Connection,
	vdom_root: Element<State>,
	inner_map: ElementInnerMap,
	resources: ResourceRegistry,
}
impl<State: Reify> View<State> {
	pub fn new(
		state: &State,
		dbus_connection: Connection,
		parent_spatial: &impl SpatialRefAspect,
	) -> View<State> {
		let _root = Spatial::create(parent_spatial, Transform::identity(), false).unwrap();
		let mut inner_map = ElementInnerMap::default();
		let vdom_root = elements::Spatial::default().with_children([state.reify()]);
		vdom_root
			.0
			.apply_element_keys(vec![(0, GenericElement::type_id(vdom_root.0.as_ref()))]);
		let mut resources = ResourceRegistry::default();
		vdom_root
			.0
			.create_inner_recursive(
				&_root.clone().as_spatial_ref(),
				&mut inner_map,
				&dbus_connection,
				&mut resources,
			)
			.unwrap();
		View {
			_root,
			dbus_connection,
			vdom_root,
			inner_map,
			resources,
		}
	}

	pub fn update(&mut self, state: &mut State) {
		let new_vdom = elements::Spatial::default().with_children([state.reify()]);
		new_vdom
			.0
			.apply_element_keys(vec![(0, GenericElement::type_id(new_vdom.0.as_ref()))]);
		new_vdom.0.diff_and_apply(
			self.vdom_root.0.spatial_aspect(&self.inner_map),
			&self.vdom_root,
			&self.dbus_connection,
			state,
			&mut self.inner_map,
			&mut self.resources,
		);
		self.vdom_root = new_vdom;
	}
	pub fn frame(&mut self, info: &FrameInfo) {
		self.vdom_root.0.frame_recursive(info, &mut self.inner_map);
	}
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct ElementInnerKey(u64);

#[derive(Debug, Default)]
struct ElementInnerMap(FxHashMap<ElementInnerKey, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
	fn insert<State: ValidState, E: ElementTrait<State>>(
		&mut self,
		key: ElementInnerKey,
		inner: E::Inner,
	) {
		self.0.insert(key, Box::new(inner));
	}
	fn get<State: ValidState, E: ElementTrait<State>>(
		&self,
		key: ElementInnerKey,
	) -> Option<&E::Inner> {
		self.0.get(&key)?.downcast_ref()
	}
	fn get_mut<State: ValidState, E: ElementTrait<State>>(
		&mut self,
		key: ElementInnerKey,
	) -> Option<&mut E::Inner> {
		self.0.get_mut(&key)?.downcast_mut()
	}
}

#[derive_where::derive_where(Debug)]
pub struct Element<State: ValidState>(Box<dyn GenericElement<State>>);
impl<State: ValidState> Element<State> {
	pub fn map<
		NewState: ValidState,
		F: Fn(&mut NewState) -> Option<&mut State> + Send + Sync + 'static,
	>(
		self,
		mapper: F,
	) -> Element<NewState> {
		Element(Box::new(MappedElement::<NewState, State, F> {
			element: self,
			mapper,
			data: PhantomData,
		}))
	}
	pub fn identify<H: Hash>(mut self, h: &H) -> Self {
		let mut hasher = DefaultHasher::new();
		h.hash(&mut hasher);
		let key = ElementInnerKey(hasher.finish());
		self.0.identify(key);
		self
	}
}
impl<State: ValidState> Hash for Element<State> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.0.inner_key().hash(state);
	}
}
impl<State: ValidState> PartialEq for Element<State> {
	fn eq(&self, other: &Self) -> bool {
		self.0.inner_key() == other.0.inner_key()
	}
}
impl<State: ValidState> Eq for Element<State> {}

struct MappedElement<
	State: ValidState,
	SubState: ValidState,
	F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
> {
	element: Element<SubState>,
	mapper: F,
	data: PhantomData<State>,
}
impl<
		State: ValidState,
		SubState: ValidState,
		F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
	> GenericElement<State> for MappedElement<State, SubState, F>
{
	fn type_id(&self) -> TypeId {
		TypeId::of::<(State, SubState)>()
	}

	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		dbus_connection: &Connection,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		self.element
			.0
			.create_inner_recursive(parent, inner_map, dbus_connection, resources)
	}
	fn frame_recursive(&self, info: &FrameInfo, inner_map: &mut ElementInnerMap) {
		self.element.0.frame_recursive(info, inner_map);
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.element.0.destroy_inner_recursive(inner_map)
	}

	fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
		self.element.0.spatial_aspect(inner_map)
	}
	fn as_any(&self) -> &dyn Any {
		self
	}
	fn inner_key(&self) -> Option<ElementInnerKey> {
		self.element.0.inner_key()
	}
	fn apply_element_keys(&self, path: Vec<(usize, TypeId)>) {
		self.element.0.apply_element_keys(path)
	}
	fn identify(&mut self, key: ElementInnerKey) {
		self.element.0.identify(key);
	}
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		dbus_connection: &Connection,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let old_mapper: &Self = old.0.as_any().downcast_ref().unwrap();
		let Some(mapped) = (self.mapper)(state) else {
			return;
		};
		self.element.0.diff_and_apply(
			parent_spatial,
			&old_mapper.element,
			dbus_connection,
			mapped,
			inner_map,
			resources,
		)
	}
}

impl<
		State: ValidState,
		SubState: ValidState,
		F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
	> Debug for MappedElement<State, SubState, F>
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("MappedElement").field(&self.element).finish()
	}
}

trait GenericElement<State: ValidState>: Any + Debug + Send + Sync {
	fn type_id(&self) -> TypeId;
	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		dbus_connection: &Connection,
		resources: &mut ResourceRegistry,
	) -> Result<(), String>;
	fn frame_recursive(&self, info: &FrameInfo, inner_map: &mut ElementInnerMap);
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
	fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef;
	fn as_any(&self) -> &dyn Any;
	fn inner_key(&self) -> Option<ElementInnerKey>;
	fn apply_element_keys(&self, path: Vec<(usize, TypeId)>);
	fn identify(&mut self, key: ElementInnerKey);
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		dbus_connection: &Connection,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);
}

#[derive_where::derive_where(Debug)]
struct ElementWrapper<State: ValidState, E: ElementTrait<State>> {
	params: E,
	inner_key: OnceLock<ElementInnerKey>,
	children: Vec<Element<State>>,
}
impl<State: ValidState, E: ElementTrait<State>> GenericElement<State> for ElementWrapper<State, E> {
	fn type_id(&self) -> TypeId {
		self.params.type_id()
	}
	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		dbus_connection: &Connection,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		let inner = E::create_inner(
			&self.params,
			parent,
			dbus_connection,
			resources.get::<E::Resource>(),
		)
		.map_err(|e| e.to_string())?;
		let Some(inner_key) = self.inner_key.get() else {
			return Err("Internal: Couldn't get inner key?".to_string());
		};
		inner_map.insert::<State, E>(*inner_key, inner);

		let spatial = self.spatial_aspect(inner_map);
		for child in &self.children {
			child
				.0
				.create_inner_recursive(&spatial, inner_map, dbus_connection, resources)?;
		}
		Ok(())
	}
	fn frame_recursive(&self, info: &FrameInfo, inner_map: &mut ElementInnerMap) {
		let Some(inner_key) = self.inner_key.get() else {
			return;
		};
		let Some(inner) = inner_map.get_mut::<State, E>(*inner_key) else {
			return;
		};
		self.params.frame(info, inner);

		for child in &self.children {
			child.0.frame_recursive(info, inner_map);
		}
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		for child in &self.children {
			child.0.destroy_inner_recursive(inner_map);
		}
		let Some(inner_key) = self.inner_key() else {
			return;
		};
		inner_map.0.remove(&inner_key);
	}

	fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
		let inner_key = *self.inner_key.get().unwrap();
		let inner = inner_map.get::<State, E>(inner_key).unwrap();
		self.params.spatial_aspect(inner)
	}

	fn as_any(&self) -> &dyn Any {
		self
	}

	fn inner_key(&self) -> Option<ElementInnerKey> {
		self.inner_key.get().cloned()
	}

	fn apply_element_keys(&self, path: Vec<(usize, TypeId)>) {
		let path = if let Some(key) = self.inner_key.get().cloned() {
			vec![(key.0 as usize, GenericElement::type_id(self))]
		} else {
			let mut hasher = DefaultHasher::new();
			path.hash(&mut hasher);
			let key = ElementInnerKey(hasher.finish());
			let _ = self.inner_key.set(key);
			path
		};

		for (i, child) in self.children.iter().enumerate() {
			let mut child_path = path.clone();
			child_path.push((i, GenericElement::type_id(child.0.as_ref())));
			child.0.apply_element_keys(child_path);
		}
	}

	fn identify(&mut self, key: ElementInnerKey) {
		let _ = self.inner_key.set(key);
	}

	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		dbus_connection: &Connection,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let old_wrapper: &ElementWrapper<State, E> = old
			.0
			.as_any()
			.downcast_ref()
			.unwrap_or_else(|| panic!("old:{:?}\nnew:{:?}\n", old, self));
		let inner_key = *self.inner_key.get().unwrap();
		let inner = inner_map.get_mut::<State, E>(inner_key).unwrap();
		self.params.update(
			&old_wrapper.params,
			state,
			inner,
			resources.get::<E::Resource>(),
		);

		let mut delta_set = DeltaSet::default();
		delta_set.push_new(old_wrapper.children.iter());
		let old_children: FxHashSet<_> = delta_set.current.iter().cloned().collect();
		delta_set.push_new(self.children.iter());

		// modified possibly
		for new_child in delta_set.current().difference(delta_set.added()) {
			let old_child = old_children.get(new_child).unwrap();

			new_child.0.diff_and_apply(
				old_child.0.spatial_aspect(inner_map),
				old_child,
				dbus_connection,
				state,
				inner_map,
				resources,
			);
		}
		// just removed
		for child in delta_set.removed() {
			child.0.destroy_inner_recursive(inner_map);
		}
		// just added
		for child in delta_set.added() {
			child
				.0
				.create_inner_recursive(&parent_spatial, inner_map, dbus_connection, resources)
				.unwrap();
		}
	}
}
