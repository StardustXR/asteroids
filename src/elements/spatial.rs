use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use stardust_xr_fusion::{Error, spatial::Transform};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spatial(pub Transform);
impl<State: ValidState> CustomElement<State> for Spatial {
	type Inner = stardust_xr_fusion::spatial::Spatial;

	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		if self.0 != Transform::IDENTITY {
			info.child_space.set_local_transform(self.0)?;
		}
		Ok(info.child_space)
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
