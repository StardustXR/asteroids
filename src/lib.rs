use manifest_dir_macros::directory_relative_path;
use serde::{de::DeserializeOwned, Serialize};
use slotmap::{DefaultKey, SlotMap};
use stardust_xr_fusion::{
    client::{Client, ClientState, FrameInfo, RootHandler},
    core::schemas::flex::flexbuffers,
    node::{Node, NodeError, NodeType},
    spatial::SpatialAspect,
};
use std::{any::Any, fmt::Debug, sync::Arc};

mod elements;
pub use elements::*;

pub trait ValidState:
    Debug + Default + Clone + Serialize + DeserializeOwned + Send + Sync + 'static
{
}
impl<T: Debug + Default + Clone + Serialize + DeserializeOwned + Send + Sync + 'static> ValidState
    for T
{
}

pub async fn make_stardust_client<State: ValidState>(root: (Root<State>, Vec<Element>)) {
    let (client, event_loop) = Client::connect_with_async_loop().await.unwrap();
    client.set_base_prefixes(&[directory_relative_path!("res")]);

    let _root = client
        .wrap_root(StardustClient::new(client.clone(), root))
        .unwrap();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => (),
        _ = event_loop => panic!("server crashed"),
    }
}

// pub trait Resultant<State, Output>: Sized {
//     fn eval(&self, state: &mut State) -> Output;
// }
// impl<State, A: Clone + Into<T>, T> Resultant<State, T> for A {
//     fn eval(&self, _state: &mut State) -> T {
//         self.clone().into()
//     }
// }
// impl<State, F: Fn(&mut State) -> T, T> Resultant<State, T> for F {
//     fn eval(&self, state: &mut State) -> T {
//         self(state)
//     }
// }

struct SpatialHack(stardust_xr_fusion::node::Node);
impl NodeType for SpatialHack {
    fn node(&self) -> &stardust_xr_fusion::node::Node {
        &self.0
    }
    fn from_path(client: &Arc<Client>, path: String, destroyable: bool) -> Self {
        SpatialHack(Node::from_path(client, path, destroyable))
    }
    fn alias(&self) -> Self
    where
        Self: Sized,
    {
        SpatialHack(self.0.alias())
    }
}
impl SpatialAspect for SpatialHack {}

pub struct StardustClient<State: ValidState> {
    client: Arc<Client>,
    state: State,
    root: ElementWrapper<Root<State>>,
    inner_map: ElementInnerMap,
}
impl<State: ValidState> StardustClient<State> {
    pub fn new(client: Arc<Client>, root: (Root<State>, Vec<Element>)) -> StardustClient<State> {
        let mut inner_map = ElementInnerMap::default();
        let (root, children) = root;
        let mut root = ElementWrapper {
            decl: root.clone(),
            decl_old: root,
            inner_key: inner_map.insert::<Root<State>>(client.clone()),
            children: children.into_iter().collect(),
        };
        for child in &mut root.children {
            child
                .0
                .create_inner(
                    &SpatialHack(client.get_root().node().alias()),
                    &mut inner_map,
                )
                .unwrap();
        }
        let state = flexbuffers::from_slice(&client.state().data).unwrap_or_default();
        StardustClient {
            client,
            state,
            root,
            inner_map,
        }
    }
}
impl<State: ValidState> RootHandler for StardustClient<State> {
    fn frame(&mut self, info: FrameInfo) {
        self.root.update(&mut self.inner_map);
        (self.root.decl.on_frame)(&mut self.state, &info)
    }
    fn save_state(&mut self) -> ClientState {
        ClientState {
            data: flexbuffers::to_vec(&self.state).unwrap(),
            root: self.client.get_root().alias(),
            spatial_anchors: Default::default(),
        }
    }
}

#[derive(Debug, Default)]
struct ElementInnerMap(SlotMap<DefaultKey, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
    fn insert<E: ElementTrait>(&mut self, inner: E::Inner) -> DefaultKey {
        self.0.insert(Box::new(inner))
    }
    fn get<E: ElementTrait>(&self, key: DefaultKey) -> Option<&E::Inner> {
        self.0.get(key)?.downcast_ref()
    }
    fn get_mut<E: ElementTrait>(&mut self, key: DefaultKey) -> Option<&mut E::Inner> {
        self.0.get_mut(key)?.downcast_mut()
    }
}

pub struct Element(Box<dyn GenericElement>);
// impl Element {
//     fn wrapper<E: ElementTrait>(&self) -> Option<&ElementWrapper<E>> {
//         <dyn Any>::downcast_ref(&self.0)
//     }
//     fn decl<E: ElementTrait>(&self) -> Option<&E> {
//         Some(&self.wrapper::<E>()?.decl)
//     }
//     fn inner<'a, E: ElementTrait>(
//         &'a self,
//         inner_map: &'a mut ElementInnerMap,
//     ) -> Option<&'a E::Inner> {
//         inner_map.get::<E>(self.wrapper::<E>()?.inner_key)
//     }
// }
struct ElementWrapper<E: ElementTrait> {
    decl: E,
    decl_old: E,
    inner_key: DefaultKey,
    children: Vec<Element>,
}
trait GenericElement: Any + Send + Sync {
    fn create_inner(
        &mut self,
        parent: &SpatialHack,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String>;
    fn update(&mut self, inner_map: &mut ElementInnerMap);
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> Option<SpatialHack>;
}
impl<E: ElementTrait> GenericElement for ElementWrapper<E> {
    fn create_inner(
        &mut self,
        parent: &SpatialHack,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        self.inner_key =
            inner_map.insert::<E>(E::create_inner(&self.decl, parent).map_err(|e| e.to_string())?);

        let spatial = self
            .spatial_aspect(inner_map)
            .unwrap_or_else(|| parent.alias());
        for child in &mut self.children {
            let _ = child.create_inner(&spatial, inner_map);
        }
        Ok(())
    }
    fn update(&mut self, inner_map: &mut ElementInnerMap) {
        let Some(inner) = inner_map.get_mut::<E>(self.inner_key) else {
            return;
        };
        self.decl.update(&self.decl_old, inner);
        self.decl_old = self.decl.clone();

        for child in &mut self.children {
            child.update(inner_map);
        }
    }

    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> Option<SpatialHack> {
        Some(SpatialHack(
            self.decl
                .spatial_aspect(inner_map.get::<E>(self.inner_key)?)?
                .node()
                .alias(),
        ))
    }
}
impl GenericElement for Element {
    fn create_inner(
        &mut self,
        parent: &SpatialHack,
        inner_map: &mut ElementInnerMap,
    ) -> Result<(), String> {
        self.0.create_inner(parent, inner_map)
    }
    fn update(&mut self, inner_map: &mut ElementInnerMap) {
        self.0.update(inner_map)
    }
    fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> Option<SpatialHack> {
        self.0.spatial_aspect(inner_map)
    }
}

pub trait ElementTrait: Debug + Send + Sync + Clone + 'static {
    type Inner: Send + Sync + 'static;
    type Error: ToString;
    fn create_inner(&self, parent_space: &impl SpatialAspect) -> Result<Self::Inner, Self::Error>;
    fn update(&self, old_decl: &Self, inner: &mut Self::Inner);
    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect>;
    fn build(self) -> Element {
        self.with_children([])
    }
    fn with_children(self, children: impl IntoIterator<Item = Element>) -> Element {
        Element(Box::new(ElementWrapper {
            decl_old: self.clone(),
            decl: self,
            inner_key: DefaultKey::default(),
            children: children.into_iter().collect(),
        }))
    }
}

#[derive(Debug, Clone)]
pub struct Root<State: ValidState> {
    pub on_frame: fn(&mut State, &FrameInfo),
}
impl<State: ValidState> Default for Root<State> {
    fn default() -> Self {
        Root {
            on_frame: |_, _| (),
        }
    }
}
impl<State: ValidState> ElementTrait for Root<State> {
    type Inner = Arc<Client>;
    type Error = NodeError;

    fn create_inner(&self, parent: &impl SpatialAspect) -> Result<Self::Inner, Self::Error> {
        parent.client()
    }
    fn update(&self, _old_decl: &Self, _inner: &mut Self::Inner) {}
    fn spatial_aspect<'a>(&self, inner: &'a Self::Inner) -> Option<&'a impl SpatialAspect> {
        Some(inner.get_root())
    }
}
impl<State: ValidState> Root<State> {
    pub fn build(self) -> (Self, Vec<Element>) {
        (self, vec![])
    }
    pub fn with_children(
        self,
        children: impl IntoIterator<Item = Element>,
    ) -> (Self, Vec<Element>) {
        (self, children.into_iter().collect())
    }
}
