use custom::ElementTrait;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use stardust_xr_fusion::spatial::{Spatial, SpatialRef, SpatialRefAspect, Transform};
use std::{
    any::{Any, TypeId},
    collections::hash_map::DefaultHasher,
    fmt::Debug,
    hash::{Hash, Hasher},
    marker::PhantomData,
    sync::OnceLock,
};

pub mod client;
pub mod custom;
pub mod elements;

// pub trait Identify {
//     type Id: Hash + Eq;
//     fn id(&self) -> &Self::Id;
// }
// impl<T: Hash + Eq> Identify for T {
//     type Id = Self;
//     fn id(&self) -> &Self::Id {
//         self
//     }
// }

pub trait RootState:
    Reify + Default + Serialize + DeserializeOwned + Send + Sync + 'static
{
}
impl<T: Reify + Default + Serialize + DeserializeOwned + Send + Sync + 'static> RootState for T {}

pub trait Reify: Sized + Send + Sync + 'static {
    fn reify(&self) -> Element<Self>;

    fn reify_substate<
        SuperState: Reify,
        F: Fn(&mut SuperState) -> &mut Self + Send + Sync + 'static,
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

pub struct View<State: Reify> {
    _root: Spatial,
    vdom_root: Element<State>,
    inner_map: ElementInnerMap,
}
impl<State: Reify> View<State> {
    pub fn new(state: &State, parent_spatial: &impl SpatialRefAspect) -> View<State> {
        let root = Spatial::create(parent_spatial, Transform::identity(), false).unwrap();
        let mut inner_map = ElementInnerMap::default();
        let vdom_root = state.reify();
        vdom_root.apply_element_keys(vec![(0, GenericElement::type_id(&vdom_root))]);
        vdom_root
            .create_inner_recursive(&root.clone().as_spatial_ref(), &mut inner_map)
            .unwrap();
        View {
            _root: root,
            vdom_root,
            inner_map,
        }
    }

    pub fn update(&mut self, state: &mut State) {
        let new_vdom = state.reify();
        new_vdom.apply_element_keys(vec![(0, GenericElement::type_id(&new_vdom))]);
        new_vdom.diff_and_apply(
            self.vdom_root.spatial_aspect(&self.inner_map),
            &self.vdom_root,
            state,
            &mut self.inner_map,
        );
        self.vdom_root = new_vdom;
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct ElementInnerKey(u64);
// impl ElementInnerKey {
// pub fn from_identifiable<I: Identify>(i: &I) -> Self {
//     let mut hasher = DefaultHasher::new();
//     i.id().hash(&mut hasher);
//     ElementInnerKey(hasher.finish())
// }

// pub fn new_random() -> Self {
//     let random_value: u64 = rand::thread_rng().gen();
//     ElementInnerKey(random_value)
// }
// }

#[derive(Debug, Default)]
struct ElementInnerMap(FxHashMap<ElementInnerKey, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
    fn insert<State: Reify, E: ElementTrait<State>>(
        &mut self,
        key: ElementInnerKey,
        inner: E::Inner,
    ) {
        self.0.insert(key, Box::new(inner));
    }
    fn get<State: Reify, E: ElementTrait<State>>(&self, key: ElementInnerKey) -> Option<&E::Inner> {
        self.0.get(&key)?.downcast_ref()
    }
    fn get_mut<State: Reify, E: ElementTrait<State>>(
        &mut self,
        key: ElementInnerKey,
    ) -> Option<&mut E::Inner> {
        self.0.get_mut(&key)?.downcast_mut()
    }
}

#[derive_where::derive_where(Debug)]
pub struct Element<State: Reify>(Box<dyn GenericElement<State>>);
impl<State: Reify> Element<State> {
    pub fn map<NewState: Reify, F: Fn(&mut NewState) -> &mut State + Send + Sync + 'static>(
        self,
        mapper: F,
    ) -> Element<NewState> {
        Element(Box::new(MappedElement::<NewState, State, F> {
            element: self,
            mapper,
            data: PhantomData,
        }))
    }
}
impl<State: Reify> Hash for Element<State> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner_key().hash(state);
    }
}
impl<State: Reify> PartialEq for Element<State> {
    fn eq(&self, other: &Self) -> bool {
        self.inner_key() == other.inner_key()
    }
}
impl<State: Reify> Eq for Element<State> {}

pub struct MappedElement<
    State: Reify,
    SubState: Reify,
    F: Fn(&mut State) -> &mut SubState + Send + Sync + 'static,
> {
    element: Element<SubState>,
    mapper: F,
    data: PhantomData<State>,
}
impl<State: Reify, SubState: Reify, F: Fn(&mut State) -> &mut SubState + Send + Sync + 'static>
    GenericElement<State> for MappedElement<State, SubState, F>
{
    fn type_id(&self) -> TypeId {
        TypeId::of::<(State, SubState)>()
    }

    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        self.element.create_inner_recursive(parent, inner_map)
    }

    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
        self.element.destroy_inner_recursive(inner_map)
    }

    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
        self.element.spatial_aspect(inner_map)
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn inner_key(&self) -> Option<ElementInnerKey> {
        self.element.inner_key()
    }

    fn apply_element_keys(&self, path: Vec<(usize, TypeId)>) {
        self.element.apply_element_keys(path)
    }

    fn diff_and_apply(
        &self,
        parent_spatial: SpatialRef,
        old: &Element<State>,
        state: &mut State,
        inner_map: &mut ElementInnerMap,
    ) {
        let old_mapper: &Self = old.0.as_any().downcast_ref().unwrap();
        self.element.diff_and_apply(
            parent_spatial,
            &old_mapper.element,
            (self.mapper)(state),
            inner_map,
        )
    }
}

impl<State: Reify, SubState: Reify, F: Fn(&mut State) -> &mut SubState + Send + Sync + 'static>
    Debug for MappedElement<State, SubState, F>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("MappedElement").field(&self.element).finish()
    }
}

#[derive_where::derive_where(Debug)]
struct ElementWrapper<State: Reify, E: ElementTrait<State>> {
    params: E,
    inner_key: OnceLock<ElementInnerKey>,
    children: Vec<Element<State>>,
}
trait GenericElement<State: Reify>: Any + Debug + Send + Sync {
    fn type_id(&self) -> TypeId;
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String>;
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef;
    fn as_any(&self) -> &dyn Any;
    fn inner_key(&self) -> Option<ElementInnerKey>;
    fn apply_element_keys(&self, path: Vec<(usize, TypeId)>);
    fn diff_and_apply(
        &self,
        parent_spatial: SpatialRef,
        old: &Element<State>,
        state: &mut State,
        inner_map: &mut ElementInnerMap,
    );
}
impl<State: Reify, E: ElementTrait<State>> GenericElement<State> for ElementWrapper<State, E> {
    fn type_id(&self) -> TypeId {
        self.params.type_id()
    }
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        let inner = E::create_inner(&self.params, parent).map_err(|e| e.to_string())?;
        inner_map.insert::<State, E>(self.inner_key().unwrap(), inner);

        let spatial = self.spatial_aspect(inner_map);
        for child in &self.children {
            child.create_inner_recursive(&spatial, inner_map)?;
        }
        Ok(())
    }
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
        for child in &self.children {
            child.destroy_inner_recursive(inner_map);
        }
        inner_map.0.remove(&self.inner_key().unwrap());
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
        let key = {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            ElementInnerKey(hasher.finish())
        };
        let _ = self.inner_key.set(key);

        for (i, child) in self.children.iter().enumerate() {
            let mut child_path = path.clone();
            child_path.push((i, GenericElement::type_id(child)));
            child.apply_element_keys(child_path);
        }
    }

    fn diff_and_apply(
        &self,
        parent_spatial: SpatialRef,
        old: &Element<State>,
        state: &mut State,
        inner_map: &mut ElementInnerMap,
    ) {
        let old_wrapper: &ElementWrapper<State, E> = old
            .0
            .as_any()
            .downcast_ref()
            .unwrap_or_else(|| panic!("old:{:?}\nnew:{:?}\n", old, self));
        let inner_key = *self.inner_key.get().unwrap();
        let inner = inner_map.get_mut::<State, E>(inner_key).unwrap();
        self.params.update(&old_wrapper.params, state, inner);

        let mut delta_set = DeltaSet::default();
        delta_set.push_new(old_wrapper.children.iter());
        let old_children: FxHashSet<_> = delta_set.current.iter().cloned().collect();
        delta_set.push_new(self.children.iter());

        // modified possibly
        for new_child in delta_set.current().difference(delta_set.added()) {
            let old_child = old_children.get(new_child).unwrap();

            new_child.diff_and_apply(
                old_child.spatial_aspect(inner_map),
                old_child,
                state,
                inner_map,
            );
        }
        // just removed
        for child in delta_set.removed() {
            child.destroy_inner_recursive(inner_map);
        }
        // just added
        for child in delta_set.added() {
            child
                .create_inner_recursive(&parent_spatial, inner_map)
                .unwrap();
        }
    }
}
impl<State: Reify> GenericElement<State> for Element<State> {
    fn type_id(&self) -> TypeId {
        GenericElement::type_id(self.0.as_ref())
    }
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        self.0.create_inner_recursive(parent, inner_map)
    }
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
        self.0.destroy_inner_recursive(inner_map)
    }
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
        self.0.spatial_aspect(inner_map)
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn inner_key(&self) -> Option<ElementInnerKey> {
        self.0.inner_key()
    }
    fn apply_element_keys(&self, path: Vec<(usize, TypeId)>) {
        self.0.apply_element_keys(path)
    }
    fn diff_and_apply(
        &self,
        parent_spatial: SpatialRef,
        old: &Element<State>,
        state: &mut State,
        inner_map: &mut ElementInnerMap,
    ) {
        self.0.diff_and_apply(parent_spatial, old, state, inner_map)
    }
}
