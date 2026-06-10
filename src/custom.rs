use crate::{ValidState, context::Context, element::ElementWrapper};
pub use derive_setters;
use stardust_xr_fusion::{
	client::FrameInfo,
	spatial::{Spatial, SpatialRef, Transform},
};
use std::{any::Any, error::Error, fmt::Debug, path::PathBuf, sync::Arc};

pub struct CreateInnerInfo {
	pub parent_space: SpatialRef,
	pub child_space: Spatial,
	pub element_path: PathBuf,
}

pub trait CustomElement<State: ValidState>: Any + Debug + Send + Sync + Sized + 'static {
	/// The imperative struct containing non-saved state
	type Inner: Send + Sync + 'static;
	/// Error type for the element
	type Error: Error + Send + Sync + 'static;
	/// Create the inner imperative struct
	fn create_inner(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
	) -> impl Future<Output = Result<Self::Inner, Self::Error>> + Send + Sync;
	/// Update the inner imperative struct with the new state of the node.
	/// You will need to check for changes between `self` and `old_self` and update accordingly.
	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner);
	/// Every frame on the server
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner: &mut Self::Inner,
	) {
	}
	/// Call this to add the element as a child of another one.
	fn build(self) -> ElementWrapper<State, Self, ()> {
		ElementWrapper::<State, Self, ()>::new(self)
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
pub struct CloneFnWrapper<Signature: Send + Sync + ?Sized>(pub Arc<Signature>);
impl<Signature: Send + Sync + ?Sized> Debug for CloneFnWrapper<Signature> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("Function").finish()
	}
}
impl<Signature: Send + Sync + ?Sized> PartialEq for CloneFnWrapper<Signature> {
	fn eq(&self, _other: &Self) -> bool {
		true
	}
}
impl<Signature: Send + Sync + ?Sized> Clone for CloneFnWrapper<Signature> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

pub trait Transformable: Sized {
	fn transform(&self) -> &Transform;
	fn transform_mut(&mut self) -> &mut Transform;
	fn apply_transform(&self, other: &Self, spatial: &Spatial) {
		if self.transform() != other.transform() {
			let _ = spatial.set_local_transform(*self.transform());
		}
	}

	fn pos(mut self, pos: impl Into<mint::Vector3<f32>>) -> Self {
		self.transform_mut().translation = pos.into();
		self
	}
	fn rot(mut self, rot: impl Into<mint::Quaternion<f32>>) -> Self {
		self.transform_mut().rotation = rot.into();
		self
	}
	fn scl(mut self, scl: impl Into<mint::Vector3<f32>>) -> Self {
		self.transform_mut().scale = scl.into();
		self
	}
}
