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

pub type ElementGenerator<State> = fn(&State) -> Element;

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

pub struct StardustClient<State: ValidState> {
    client: Arc<Client>,
    on_frame: fn(&mut State, &FrameInfo),
    root_view: ElementGenerator<State>,
    state: State,
    vdom_root: Element,
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
        Self::apply_element_keys(Vec::new(), &vdom_root);
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
        // dbg!(&new_vdom);
        Self::apply_element_keys(Vec::new(), &new_vdom);
        Self::diff_and_apply(
            self.client.get_root().spatial_ref(),
            &self.vdom_root,
            &new_vdom,
            &mut self.inner_map,
        );
        self.vdom_root = new_vdom;
    }

    fn apply_element_keys(path: Vec<(TypeId, usize)>, element: &Element) {
        let key = {
            let mut hasher = DefaultHasher::new();
            path.hash(&mut hasher);
            ElementInnerKey(hasher.finish())
        };
        element.apply_inner_key(key);
        for (i, child) in element.children().iter().enumerate() {
            let mut path = path.clone();
            path.push((GenericElement::type_id(element), i));
            Self::apply_element_keys(path, child);
        }
    }
    fn diff_and_apply(
        parent_spatial: SpatialRef,
        old: &Element,
        new: &Element,
        inner_map: &mut ElementInnerMap,
    ) {
        if old.inner_key() == new.inner_key() {
            new.update(old, inner_map);

            let old_children = FxHashSet::from_iter(old.children().iter());
            let new_children = FxHashSet::from_iter(new.children().iter());

            // just removed
            for child in old_children.difference(&new_children) {
                child.destroy_inner_recursive(inner_map);
            }
            // modified possibly
            for child in new_children.intersection(&old_children) {
                let old_child = old_children.get(child).unwrap();
                Self::diff_and_apply(
                    old_child.spatial_aspect(inner_map),
                    old_child,
                    child,
                    inner_map,
                )
            }
            // just added
            for child in new_children.difference(&old_children) {
                let _ = child.create_inner_recursive(&parent_spatial, inner_map);
            }
        } else {
            old.destroy_inner_recursive(inner_map);
            let _ = new.create_inner_recursive(&parent_spatial, inner_map);
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Hash, PartialEq, Eq)]
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
    fn insert<E: ElementTrait>(&mut self, key: ElementInnerKey, inner: E::Inner) {
        self.0.insert(key, Box::new(inner));
    }
    fn get<E: ElementTrait>(&self, key: ElementInnerKey) -> Option<&E::Inner> {
        self.0.get(&key)?.downcast_ref()
    }
    fn get_mut<E: ElementTrait>(&mut self, key: ElementInnerKey) -> Option<&mut E::Inner> {
        self.0.get_mut(&key)?.downcast_mut()
    }
}

#[derive(Debug)]
pub struct Element(Box<dyn GenericElement>);
impl Element {
    fn wrapper<E: ElementTrait>(&self) -> Option<&ElementWrapper<E>> {
        self.0.as_any().downcast_ref()
    }
    fn params<E: ElementTrait>(&self) -> Option<&E> {
        Some(&self.wrapper::<E>()?.params)
    }
    // fn inner<'a, E: ElementTrait>(&self, inner_map: &'a ElementInnerMap) -> Option<&'a E::Inner> {
    //     inner_map.get::<E>(self.wrapper::<E>()?.inner_key)
    // }
    fn children(&self) -> &[Element] {
        self.0.children()
    }
}
impl Clone for Element {
    fn clone(&self) -> Self {
        Element(self.0.clone_box())
    }
}
impl Hash for Element {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.inner_key().hash(state);
    }
}
impl PartialEq for Element {
    fn eq(&self, other: &Self) -> bool {
        self.inner_key() == other.inner_key()
    }
}
impl Eq for Element {}

#[derive(Debug, Clone)]
struct ElementWrapper<E: ElementTrait> {
    params: E,
    inner_key: OnceLock<ElementInnerKey>,
    children: Vec<Element>,
}
trait GenericElement: Any + Debug + Send + Sync {
    fn type_id(&self) -> TypeId;
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String>;
    fn update(&self, old: &Element, inner_map: &mut ElementInnerMap);
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef;
    fn as_any(&self) -> &dyn Any;
    fn inner_key(&self) -> Option<ElementInnerKey>;
    fn apply_inner_key(&self, key: ElementInnerKey);
    fn children(&self) -> &[Element];
    fn clone_box(&self) -> Box<dyn GenericElement>;
}
impl<E: ElementTrait> GenericElement for ElementWrapper<E> {
    fn type_id(&self) -> TypeId {
        TypeId::of::<Self>()
    }
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        let inner = E::create_inner(&self.params, parent).map_err(|e| e.to_string())?;
        inner_map.insert::<E>(self.inner_key().unwrap(), inner);

        let spatial = self.spatial_aspect(inner_map);
        for child in &self.children {
            child.create_inner_recursive(&spatial, inner_map)?;
        }
        Ok(())
    }
    fn update(&self, old: &Element, inner_map: &mut ElementInnerMap) {
        let inner_key = *self.inner_key.get().unwrap();
        let inner = inner_map.get_mut::<E>(inner_key).unwrap();
        self.params.update(old.params::<E>().unwrap(), inner);
    }
    fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
        for child in self.children() {
            child.destroy_inner_recursive(inner_map);
        }
        inner_map.0.remove(&self.inner_key().unwrap());
    }

    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
        let inner_key = *self.inner_key.get().unwrap();
        let inner = inner_map.get::<E>(inner_key).unwrap();
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

    fn children(&self) -> &[Element] {
        &self.children
    }

    fn clone_box(&self) -> Box<dyn GenericElement> {
        Box::new(self.clone())
    }
}
impl GenericElement for Element {
    fn type_id(&self) -> TypeId {
        self.0.type_id()
    }
    fn create_inner_recursive(
        &self,
        parent: &SpatialRef,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        self.0.create_inner_recursive(parent, inner_map)
    }
    fn update(&self, old: &Element, inner_map: &mut ElementInnerMap) {
        self.0.update(old, inner_map)
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
    fn children(&self) -> &[Element] {
        self.0.children()
    }
    fn clone_box(&self) -> Box<dyn GenericElement> {
        self.0.clone_box()
    }
}

pub trait ElementTrait: Debug + Clone + PartialEq + Send + Sync + Sized + 'static {
    type Inner: Send + Sync + 'static;
    type Error: ToString;
    fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error>;
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner);
    fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef;
    fn build(self) -> Element {
        self.with_children([])
    }
    fn with_children(self, children: impl IntoIterator<Item = Element>) -> Element {
        Element(Box::new(ElementWrapper {
            params: self,
            inner_key: OnceLock::new(),
            children: children.into_iter().collect(),
        }))
    }
}
