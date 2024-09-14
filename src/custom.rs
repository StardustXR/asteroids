use crate::{Element, ElementWrapper, Reify};
pub use derive_setters;
use stardust_xr_fusion::spatial::{SpatialAspect, SpatialRef, Transform};
use std::any::Any;
use std::fmt::Debug;
use std::sync::OnceLock;

pub trait ElementTrait<State: Reify>:
    Any + Debug + PartialEq + Send + Sync + Sized + 'static
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

pub struct FnWrapper<Signature: Send + Sync + ?Sized>(pub Box<Signature>);
impl<Signature: Send + Sync + ?Sized> Debug for FnWrapper<Signature> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Function").finish()
    }
}
impl<Signature: Send + Sync + ?Sized> PartialEq for FnWrapper<Signature> {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

pub trait Transformable: Sized {
    fn transform(&self) -> &Transform;
    fn transform_mut(&mut self) -> &mut Transform;
    fn apply_transform(&self, other: &Self, spatial: &impl SpatialAspect) {
        if self.transform().translation != other.transform().translation
            || self.transform().rotation != other.transform().rotation
            || self.transform().scale != other.transform().scale
        {
            let _ = spatial.set_local_transform(*self.transform());
        }
    }

    fn pos(mut self, pos: impl Into<mint::Vector3<f32>>) -> Self {
        self.transform_mut().translation = Some(pos.into());
        self
    }
    fn rot(mut self, rot: impl Into<mint::Quaternion<f32>>) -> Self {
        self.transform_mut().rotation = Some(rot.into());
        self
    }
    fn scl(mut self, scl: impl Into<mint::Vector3<f32>>) -> Self {
        self.transform_mut().scale = Some(scl.into());
        self
    }
}
