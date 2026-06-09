use stardust_xr_fusion::{
	drawable::set_sky_tex,
	node::{Error, NodeType},
	spatial::SpatialRef,
	types::Resource,
};

use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};

#[derive(Debug)]
pub struct SkyTexture(pub Resource);
impl<State: ValidState> CustomElement<State> for SkyTexture {
	type Inner = SkyTexInner;

	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		set_sky_tex(info.parent_space.client(), Some(&self.0))?;
		Ok(SkyTexInner(info.parent_space.clone()))
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.0 != old_self.0 {
			_ = set_sky_tex(inner.0.client(), Some(&self.0));
		}
	}
}
pub struct SkyTexInner(SpatialRef);
impl Drop for SkyTexInner {
	fn drop(&mut self) {
		_ = set_sky_tex(self.0.client(), None);
	}
}
