use crate::{Context, CreateInnerInfo, ValidState, custom::ElementTrait, util::DeltaSet};
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::{
	any::{Any, TypeId},
	collections::hash_map::DefaultHasher,
	fmt::Debug,
	hash::{Hash, Hasher},
	marker::PhantomData,
	path::Path,
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

	fn type_name(&self) -> &'static str {
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
	fn inner_key(&self) -> Option<ElementInnerKey> {
		self.element.0.inner_key()
	}
	fn apply_element_keys(&self, parent_path: &[(usize, TypeId, &'static str)], order: usize) {
		self.element.0.apply_element_keys(parent_path, order)
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
		let old_upcast: &(dyn Any + 'static) = &*old.0;
		let old_mapper: &Self = old_upcast.downcast_ref().unwrap();
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
	fn type_name(&self) -> &'static str;
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
	fn inner_key(&self) -> Option<ElementInnerKey>;
	fn apply_element_keys(&self, parent_path: &[(usize, TypeId, &'static str)], order: usize);
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
	pub(crate) path: OnceLock<Vec<(usize, TypeId, &'static str)>>,
	pub(crate) inner_key: OnceLock<ElementInnerKey>,
	pub(crate) children: Vec<Element<State>>,
}
impl<State: ValidState, E: ElementTrait<State>> GenericElement<State> for ElementWrapper<State, E> {
	fn type_id(&self) -> TypeId {
		self.params.type_id()
	}
	fn type_name(&self) -> &'static str {
		let type_name = std::any::type_name::<E>();

		// Find end boundary (first < or end of string)
		let end = type_name.find('<').unwrap_or(type_name.len());

		// Find start boundary (last : before end)
		let start = type_name[..end].rfind(':').map(|i| i + 1).unwrap_or(0);

		&type_name[start..end]
	}
	#[tracing::instrument(level = "debug", skip(inner_map, context, resources))]
	fn create_inner_recursive(
		&self,
		parent_space: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		context: &Context,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		let Some(path) = self.path.get() else {
			return Err("Internal: Couldn't get path?".to_string());
		};
		let element_path = path
			.iter()
			.map(|(order, _, name)| format!("/{name}_{order}"))
			.reduce(|acc, item| acc + &item)
			.unwrap_or("/Unknown_0".to_string());
		let create_info = CreateInnerInfo {
			parent_space,
			element_path: Path::new(&element_path),
		};
		let inner = E::create_inner(
			&self.params,
			context,
			create_info,
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
	#[tracing::instrument(level = "debug", skip(inner_map, state))]
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
	#[tracing::instrument(level = "debug", skip(inner_map))]
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

	#[tracing::instrument(level = "debug")]
	fn apply_element_keys(&self, parent_path: &[(usize, TypeId, &'static str)], order: usize) {
		let self_path_segment = (
			self.inner_key.get().map(|k| k.0 as usize).unwrap_or(order),
			GenericElement::type_id(self),
			GenericElement::type_name(self),
		);

		let path = self.path.get_or_init(|| {
			if self.inner_key.get().is_some() {
				vec![self_path_segment]
			} else {
				let mut path = parent_path.to_vec();
				path.push(self_path_segment);
				path
			}
		});

		let _ = self.inner_key.get_or_init(|| {
			let mut hasher = DefaultHasher::new();
			path.hash(&mut hasher);
			ElementInnerKey(hasher.finish())
		});

		// Apply keys to children recursively
		for (i, child) in self.children.iter().enumerate() {
			child.0.apply_element_keys(path, i);
		}
	}

	fn identify(&mut self, key: ElementInnerKey) {
		let _ = self.inner_key.set(key);
	}

	#[tracing::instrument(level = "debug", skip_all)]
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		context: &Context,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let old_upcast: &(dyn Any + 'static) = &*old.0;
		let old_wrapper: &ElementWrapper<State, E> =
			old_upcast.downcast_ref().unwrap_or_else(|| {
				old.0.destroy_inner_recursive(inner_map);
				self.create_inner_recursive(&parent_spatial, inner_map, context, resources)
					.expect("Could not create inner for new root element for swap");

				// panic!("old:{:#?}\nnew:{:#?}\n", old, self)
				self
			});
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
