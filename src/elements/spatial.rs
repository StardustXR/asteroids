use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use derive_setters::Setters;
use stardust_xr_fusion::{
	node::NodeError,
	spatial::{SpatialAspect, SpatialRef, Transform},
};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Spatial {
	transform: Transform,
	zoneable: bool,
}
impl<State: ValidState> CustomElement<State> for Spatial {
	type Inner = stardust_xr_fusion::spatial::Spatial;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::spatial::Spatial::create(
			info.parent_space,
			self.transform,
			self.zoneable,
		)
	}
	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, inner);
		if self.zoneable != old_decl.zoneable {
			let _ = inner.set_zoneable(self.zoneable);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial_ref()
	}
}
impl Default for Spatial {
	fn default() -> Self {
		Spatial {
			transform: Transform::none(),
			zoneable: false,
		}
	}
}
impl Transformable for Spatial {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
