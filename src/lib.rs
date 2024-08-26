use manifest_dir_macros::directory_relative_path;
use rustc_hash::{FxHashMap, FxHashSet};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use stardust_xr_fusion::{
    client::Client,
    core::schemas::flex::flexbuffers,
    node::{MethodResult, NodeType},
    root::{ClientState, FrameInfo, RootAspect, RootHandler},
    spatial::{SpatialRef, SpatialRefAspect},
};
use std::{
    any::{Any, TypeId},
    collections::hash_map::DefaultHasher,
    fmt::Debug,
    hash::{Hash, Hasher},
    sync::{Arc, OnceLock},
};

mod elements;
pub use elements::*;

pub trait Identify {
    type Id: Hash + Eq;
    fn id(&self) -> &Self::Id;
}
impl<T: Hash + Eq> Identify for T {
    type Id = Self;
    fn id(&self) -> &Self::Id {
        self
    }
}

pub trait ValidState:
    Default + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static
{
}
impl<T: Default + PartialEq + Serialize + DeserializeOwned + Send + Sync + 'static> ValidState
    for T
{
}

pub type ElementGenerator<State> = fn(&State) -> Element<State>;

pub async fn make_stardust_client<State: ValidState>(
    on_frame: fn(&mut State, &FrameInfo),
    root: ElementGenerator<State>,
) {
    let (client, event_loop) = Client::connect_with_async_loop().await.unwrap();
    client
        .set_base_prefixes(&[directory_relative_path!("res")])
        .unwrap();

    let asteroids = StardustClient::new(client.clone(), on_frame, root);
    let _root = client.get_root().alias().wrap(asteroids).unwrap();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => (),
        _ = event_loop => panic!("server crashed"),
    }
}

pub trait SpatialRefExt {
    fn spatial_ref(&self) -> SpatialRef;
}
impl<S: SpatialRefAspect> SpatialRefExt for S {
    fn spatial_ref(&self) -> SpatialRef {
        SpatialRef(self.node().alias())
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

pub struct StardustClient<State: ValidState> {
    client: Arc<Client>,
    on_frame: fn(&mut State, &FrameInfo),
    root_view: ElementGenerator<State>,
    state: State,
    vdom_root: Element<State>,
    inner_map: ElementInnerMap,
}
impl<State: ValidState> StardustClient<State> {
    pub fn new(
        client: Arc<Client>,
        on_frame: fn(&mut State, &FrameInfo),
        root_view: ElementGenerator<State>,
    ) -> StardustClient<State> {
        let mut inner_map = ElementInnerMap::default();
        let state = client
            .get_state()
            .data
            .as_ref()
            .and_then(|m| flexbuffers::from_slice(m).ok())
            .unwrap_or_default();
        let vdom_root = root_view(&state);
        Self::apply_element_keys(vec![(0, GenericElement::type_id(&vdom_root))], &vdom_root);
        vdom_root
            .create_inner_recursive(&client.get_root().spatial_ref(), &mut inner_map)
            .unwrap();
        StardustClient {
            client,
            on_frame,
            root_view,
            state,
            vdom_root,
            inner_map,
        }
    }

    pub fn update(&mut self) {
        let new_vdom = (self.root_view)(&self.state);
        Self::apply_element_keys(vec![(0, GenericElement::type_id(&new_vdom))], &new_vdom);
        // dbg!(&self.vdom_root);
        // dbg!(&new_vdom);
        Self::diff_and_apply(
            self.client.get_root().spatial_ref(),
            [&self.vdom_root].into_iter(),
            [&new_vdom].into_iter(),
            &mut self.state,
            &mut self.inner_map,
        );
        self.vdom_root = new_vdom;
    }

    fn apply_element_keys(path: Vec<(usize, TypeId)>, element: &Element<State>) {
        let key = {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            ElementInnerKey(hasher.finish())
        };
        element.apply_inner_key(key);
        // println!("{path:?}: {element:?}");
        for (i, child) in element.children().iter().enumerate() {
            let mut path = path.clone();
            path.push((i, GenericElement::type_id(child)));
            Self::apply_element_keys(path, child);
        }
    }
    fn diff_and_apply<'a>(
        parent_spatial: SpatialRef,
        old: impl Iterator<Item = &'a Element<State>>,
        new: impl Iterator<Item = &'a Element<State>>,
        state: &mut State,
        inner_map: &mut ElementInnerMap,
    ) {
        let mut delta_set = DeltaSet::default();
        delta_set.push_new(old);
        let old_children: FxHashSet<_> = delta_set.current.iter().cloned().collect();
        delta_set.push_new(new);

        // modified possibly
        for new_child in delta_set.current().difference(delta_set.added()) {
            let old_child = old_children.get(new_child).unwrap();
            new_child.update(old_child, state, inner_map);
            Self::diff_and_apply(
                old_child.spatial_aspect(inner_map),
                old_child.children().iter(),
                new_child.children().iter(),
                state,
                inner_map,
            )
        }
        // just removed
        for child in delta_set.removed() {
            // println!("removing element:");
            // println!("\t{:?}", child.inner_key().unwrap());
            child.destroy_inner_recursive(inner_map);
        }
        // just added (put after so the inner map's capacity can remain the same on swaps)
        for child in delta_set.added() {
            // println!("adding element:");
            // println!("\t{:?}", child.inner_key().unwrap());
            child
                .create_inner_recursive(&parent_spatial, inner_map)
                .unwrap();
        }
    }
}
impl<State: ValidState> RootHandler for StardustClient<State> {
    fn frame(&mut self, info: FrameInfo) {
        self.update();
        (self.on_frame)(&mut self.state, &info);
    }
    fn save_state(&mut self) -> MethodResult<ClientState> {
        ClientState::from_data_root(
            Some(flexbuffers::to_vec(&self.state)?),
            self.client.get_root(),
        )
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
struct ElementInnerKey(u64);
impl ElementInnerKey {
    pub fn from_identifiable<I: Identify>(i: &I) -> Self {
        let mut hasher = DefaultHasher::new();
        i.id().hash(&mut hasher);
        ElementInnerKey(hasher.finish())
    }

    // pub fn new_random() -> Self {
    //     let random_value: u64 = rand::thread_rng().gen();
    //     ElementInnerKey(random_value)
    // }
}

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
    fn wrapper<E: ElementTrait<State>>(&self) -> Option<&ElementWrapper<State, E>> {
        self.0.as_any().downcast_ref()
    }
    fn params<E: ElementTrait<State>>(&self) -> Option<&E> {
        Some(&self.wrapper::<E>()?.params)
    }
    // fn inner<'a, E: ElementTrait>(&self, inner_map: &'a ElementInnerMap) -> Option<&'a E::Inner> {
    //     inner_map.get::<E>(self.wrapper::<E>()?.inner_key)
    // }
    fn children(&self) -> &[Element<State>] {
        self.0.children()
    }
}
impl<State: ValidState> Clone for Element<State> {
    fn clone(&self) -> Self {
        Element(self.0.clone_box())
    }
}
impl<State: ValidState> Hash for Element<State> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner_key().hash(state);
    }
}
impl<State: ValidState> PartialEq for Element<State> {
    fn eq(&self, other: &Self) -> bool {
        self.inner_key() == other.inner_key()
    }
}
impl<State: ValidState> Eq for Element<State> {}

#[derive_where::derive_where(Debug, Clone)]
struct ElementWrapper<State: ValidState, E: ElementTrait<State>> {
    params: E,
    inner_key: OnceLock<ElementInnerKey>,
    children: Vec<Element<State>>,
}
// impl<State: ValidState, E: ElementTrait<State>> Debug for ElementWrapper<E, State> {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         f.debug_struct("ElementWrapper")
//             .field("params", &self.params)
//             .field("inner_key", &self.inner_key)
//             .field("children", &self.children)
//             .finish()
//     }
// }
// impl<State: ValidState, E: ElementTrait<State>> Clone for ElementWrapper<E, State> {
//     fn clone(&self) -> Self {
//         ElementWrapper {
//             params: self.params.clone(),
//             inner_key: self.inner_key.clone(),
//             children: self.children.clone(),
//         }
//     }
// }
trait GenericElement<State: ValidState>: Any + Debug + Send + Sync {
    fn type_id(&self) -> TypeId;
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String>;
    fn update(&self, old: &Element<State>, state: &mut State, inner_map: &mut ElementInnerMap);
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef;
    fn as_any(&self) -> &dyn Any;
    fn inner_key(&self) -> Option<ElementInnerKey>;
    fn apply_inner_key(&self, key: ElementInnerKey);
    fn children(&self) -> &[Element<State>];
    fn clone_box(&self) -> Box<dyn GenericElement<State>>;
}
impl<State: ValidState, E: ElementTrait<State>> GenericElement<State> for ElementWrapper<State, E> {
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
    fn update(&self, old: &Element<State>, state: &mut State, inner_map: &mut ElementInnerMap) {
        let inner_key = *self.inner_key.get().unwrap();
        let inner = inner_map
            .get_mut::<State, E>(inner_key)
            .unwrap_or_else(|| panic!("old:{old:?}\nnew:{self:?}\n"));
        self.params.update(old.params::<E>().unwrap(), state, inner);
    }
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
        for child in self.children() {
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
    fn apply_inner_key(&self, key: ElementInnerKey) {
        let _ = self.inner_key.set(key);
    }

    fn children(&self) -> &[Element<State>] {
        &self.children
    }

    fn clone_box(&self) -> Box<dyn GenericElement<State>> {
        Box::new(self.clone())
    }
}
impl<State: ValidState> GenericElement<State> for Element<State> {
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
    fn update(&self, old: &Element<State>, state: &mut State, inner_map: &mut ElementInnerMap) {
        self.0.update(old, state, inner_map)
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
    fn apply_inner_key(&self, key: ElementInnerKey) {
        self.0.apply_inner_key(key)
    }
    fn children(&self) -> &[Element<State>] {
        self.0.children()
    }
    fn clone_box(&self) -> Box<dyn GenericElement<State>> {
        self.0.clone_box()
    }
}

pub trait ElementTrait<State: ValidState>:
    Any + Debug + Clone + PartialEq + Send + Sync + Sized + 'static
{
    type Inner: Send + Sync + 'static;
    type Error: ToString;
    fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error>;
    fn update(&self, old_decl: &Self, state: &mut State, inner: &mut Self::Inner);
    fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef;
    fn build(self) -> Element<State> {
        self.with_children([])
    }
    fn with_children(self, children: impl IntoIterator<Item = Element<State>>) -> Element<State> {
        Element(Box::new(ElementWrapper::<State, Self> {
            params: self,
            inner_key: OnceLock::new(),
            children: children.into_iter().collect(),
        }))
    }
}
