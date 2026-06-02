use stardust_xr_fusion::{
	drawable::set_sky_light,
	node::{Error, NodeType},
	spatial::SpatialRef,
	types::Resource,
};

use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};

#[derive(Debug)]
pub struct SkyLight(pub Resource);
impl<State: ValidState> CustomElement<State> for SkyLight {
	type Inner = SkyLightInner;

	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		set_sky_light(info.parent_space.client(), Some(&self.0))?;
		Ok(SkyLightInner(info.parent_space.clone()))
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner) {
		if self.0 != old_self.0 {
			_ = set_sky_light(inner.0.client(), Some(&self.0));
		}
	}
}
pub struct SkyLightInner(SpatialRef);
impl Drop for SkyLightInner {
	fn drop(&mut self) {
		_ = set_sky_light(self.0.client(), None);
	}
}
