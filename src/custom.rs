use crate::ValidState;
use crate::context::Context;
use crate::element::ElementWrapper;
pub use derive_setters;
use stardust_xr_fusion::client::FrameInfo;
use stardust_xr_fusion::spatial::{Spatial, SpatialRef, Transform};
use std::any::Any;
use std::fmt::Debug;
use std::path::Path;

pub struct CreateInnerInfo {
	pub parent_space: SpatialRef,
	pub element_path: Path,
}

pub trait CustomElement<State: ValidState>:
	Any + Clone + Debug + Send + Sync + Sized + 'static
{
	/// The imperative struct containing non-saved state
	type Inner: Send + Sync + 'static;
	/// A global shared across the whole View
	type Resource: Default + Send + Sync + 'static;
	/// Error type for the element
	type Error: ToString + Send + Sync + 'static;
	/// Create the inner imperative struct
	fn create_inner(
		&self,
		asteroids_context: &Context,
		info: CreateInnerInfo,
		resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error>;
	/// Update the inner imperative struct with the new state of the node.
	/// You will need to check for changes between `self` and `old_self` and update accordingly.
	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, resource: &mut Self::Resource);
	/// Every frame on the server
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner: &mut Self::Inner,
	) {
	}
	/// Return the SpatialRef that all child elements should be parented under.
	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef;
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

pub trait Transformable: Sized {
	fn transform(&self) -> &Transform;
	fn transform_mut(&mut self) -> &mut Transform;
	fn apply_transform(&self, other: &Self, spatial: &Spatial) {
		if self.transform().translation != other.transform().translation
			|| self.transform().rotation != other.transform().rotation
			|| self.transform().scale != other.transform().scale
		{
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
