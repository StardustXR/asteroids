use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use stardust_xr_fusion::{
	node::NodeError,
	spatial::{SpatialRef, Transform},
};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spatial(pub Transform);
impl<State: ValidState> CustomElement<State> for Spatial {
	type Inner = stardust_xr_fusion::spatial::Spatial;

	type Error = NodeError;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::spatial::Spatial::create(info.parent_space, self.0)
	}
	fn diff(&self, old_self: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old_self, inner);
	}
}
impl Default for Spatial {
	fn default() -> Self {
		Spatial(Transform::IDENTITY)
	}
}
impl Transformable for Spatial {
	fn transform(&self) -> &Transform {
		&self.0
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.0
	}
}
