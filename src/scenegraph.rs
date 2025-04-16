use crate::{Context, ValidState, custom::ElementTrait, util::DeltaSet};
use regex::Regex;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::{
	any::{Any, TypeId},
	collections::hash_map::DefaultHasher,
	fmt::Debug,
	hash::{Hash, Hasher},
	marker::PhantomData,
	path::PathBuf,
	sync::OnceLock,
};

#[derive(Default)]
pub(crate) struct ResourceRegistry(FxHashMap<TypeId, Box<dyn Any>>);
impl ResourceRegistry {
	pub fn get<R: Default + Send + Sync + 'static>(&mut self) -> &mut R {
		let type_id = TypeId::of::<R>();
		self.0
			.entry(type_id)
			.or_insert_with(|| Box::new(R::default()))
			.downcast_mut::<R>()
			.unwrap()
	}
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ElementInnerKey(u64);

#[derive(Debug, Default)]
pub(crate) struct ElementInnerMap(FxHashMap<ElementInnerKey, Box<dyn Any + Send + Sync>>);
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
pub struct Element<State: ValidState>(pub(crate) Box<dyn GenericElement<State>>);
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
		GenericElement::type_id(&*self.element.0)
	}

	fn type_name(&self) -> String {
		GenericElement::type_name(&*self.element.0)
	}

	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		context: &Context,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		self.element
			.0
			.create_inner_recursive(parent, inner_map, context, resources)
	}
	fn frame_recursive(
		&self,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		let Some(mapped) = (self.mapper)(state) else {
			return;
		};
		self.element.0.frame_recursive(info, mapped, inner_map);
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
	fn apply_element_keys(&self, path: Vec<(usize, TypeId, String)>) {
		self.element.0.apply_element_keys(path)
	}
	fn identify(&mut self, key: ElementInnerKey) {
		self.element.0.identify(key);
	}
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		context: &Context,
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
			context,
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

pub(crate) trait GenericElement<State: ValidState>: Any + Debug + Send + Sync {
	fn type_id(&self) -> TypeId;
	fn type_name(&self) -> String;
	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		context: &Context,
		resources: &mut ResourceRegistry,
	) -> Result<(), String>;
	fn frame_recursive(&self, info: &FrameInfo, state: &mut State, inner_map: &mut ElementInnerMap);
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
	fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef;
	fn as_any(&self) -> &dyn Any;
	fn inner_key(&self) -> Option<ElementInnerKey>;
	fn apply_element_keys(&self, path: Vec<(usize, TypeId, String)>);
	fn identify(&mut self, key: ElementInnerKey);
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		context: &Context,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);
}

#[derive_where::derive_where(Debug)]
pub(crate) struct ElementWrapper<State: ValidState, E: ElementTrait<State>> {
	pub(crate) params: E,
	pub(crate) path: OnceLock<Vec<(usize, TypeId)>>,
	pub(crate) inner_key: OnceLock<ElementInnerKey>,
	pub(crate) element_path: OnceLock<PathBuf>,
	pub(crate) children: Vec<Element<State>>,
}
impl<State: ValidState, E: ElementTrait<State>> GenericElement<State> for ElementWrapper<State, E> {
	fn type_id(&self) -> TypeId {
		self.params.type_id()
	}
	fn type_name(&self) -> String {
		let regex = Regex::new(r"([^<>:]+::)*(?<name>[^<:]+).*").unwrap();
		let type_name = std::any::type_name::<E>();
		regex.replace_all(type_name, "$name").to_string()
		// type_name.to_string()
	}
	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		context: &Context,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		let Some(element_path) = self.element_path.get() else {
			return Err("Internal: Couldn't get element path?".to_string());
		};
		let inner = E::create_inner(
			&self.params,
			parent,
			context,
			element_path,
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
				.create_inner_recursive(&spatial, inner_map, context, resources)?;
		}
		Ok(())
	}
	fn frame_recursive(
		&self,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		let Some(inner_key) = self.inner_key.get() else {
			return;
		};
		let Some(inner) = inner_map.get_mut::<State, E>(*inner_key) else {
			return;
		};
		self.params.frame(info, state, inner);

		for child in &self.children {
			child.0.frame_recursive(info, state, inner_map);
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

	fn apply_element_keys(&self, path: Vec<(usize, TypeId, String)>) {
		let path = if let Some(key) = self.inner_key.get().cloned() {
			vec![(
				key.0 as usize,
				GenericElement::type_id(self),
				GenericElement::type_name(self).to_string(),
			)]
		} else {
			let mut hasher = DefaultHasher::new();
			path.hash(&mut hasher);
			let key = ElementInnerKey(hasher.finish());
			let _ = self.inner_key.set(key);

			// Construct the element path
			let element_path =
				PathBuf::from("/").join(format!("{}_{}", GenericElement::type_name(self), key.0));
			let _ = self.element_path.set(element_path);

			let _ = self.path.get_or_init(|| {
				path.iter()
					.map(|(order, _type, _name)| (*order, *_type))
					.collect()
			});
			path
		};

		// Apply keys to children recursively
		for (i, child) in self.children.iter().enumerate() {
			let mut child_path = path.clone();
			child_path.push((
				i,
				GenericElement::type_id(child.0.as_ref()),
				GenericElement::type_name(child.0.as_ref()).to_string(),
			));
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
		context: &Context,
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
		let old_children: FxHashSet<_> = delta_set.current().iter().cloned().collect();
		delta_set.push_new(self.children.iter());

		// modified possibly
		for new_child in delta_set.current().difference(delta_set.added()) {
			let old_child = old_children.get(new_child).unwrap();

			new_child.0.diff_and_apply(
				old_child.0.spatial_aspect(inner_map),
				old_child,
				context,
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
				.create_inner_recursive(&parent_spatial, inner_map, context, resources)
				.unwrap();
		}
	}
}
