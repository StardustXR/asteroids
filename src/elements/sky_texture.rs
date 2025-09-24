use stardust_xr_fusion::{
	drawable::set_sky_tex,
	node::{NodeError, NodeType},
	spatial::SpatialRef,
	values::ResourceID,
};

use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};

#[derive(Debug)]
pub struct SkyTexture(pub ResourceID);
impl<State: ValidState> CustomElement<State> for SkyTexture {
	type Inner = SkyTexInner;

	type Resource = ();

	type Error = NodeError;

	fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		set_sky_tex(info.parent_space.client(), Some(&self.0))?;
		Ok(SkyTexInner(info.parent_space.clone()))
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.0 != old_self.0 {
			_ = set_sky_tex(inner.0.client(), Some(&self.0));
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> stardust_xr_fusion::spatial::SpatialRef {
		inner.0.clone()
	}
}
pub struct SkyTexInner(SpatialRef);
impl Drop for SkyTexInner {
	fn drop(&mut self) {
		_ = set_sky_tex(self.0.client(), None);
	}
}
