use crate::{
	Animatable, AnimatedElement, Context, CreateInnerInfo, ValidState, custom::Transformable,
};
use stardust_xr_fusion::{
	Error,
	spatial::{SpatialRef, Transform},
};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Animatable)]
pub struct Spatial(pub Transform);
impl<State: ValidState> AnimatedElement<State> for Spatial {
	type Inner = (stardust_xr_fusion::spatial::Spatial, SpatialRef);

	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		if self.0 != Transform::IDENTITY {
			info.child_space
				.set_relative_transform(info.parent_space.clone(), self.0)?;
		}
		Ok((info.child_space, info.parent_space))
	}
	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.0, &inner.1);
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
