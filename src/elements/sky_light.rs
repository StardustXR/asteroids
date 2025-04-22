use stardust_xr_fusion::{
	drawable::set_sky_light,
	node::{NodeError, NodeType},
	spatial::SpatialRef,
	values::ResourceID,
};

use crate::{Context, CreateInnerInfo, ValidState, custom::ElementTrait};

#[derive(Debug)]
pub struct SkyLight(pub ResourceID);
impl<State: ValidState> ElementTrait<State> for SkyLight {
	type Inner = SkyLightInner;

	type Resource = ();

	type Error = NodeError;

	fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		set_sky_light(&info.parent_space.client()?, Some(&self.0))?;
		Ok(SkyLightInner(info.parent_space.clone()))
	}

	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if self.0 != old_decl.0 {
			if let Ok(client) = inner.0.client() {
				_ = set_sky_light(&client, Some(&self.0));
			}
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> stardust_xr_fusion::spatial::SpatialRef {
		inner.0.clone()
	}
}
pub struct SkyLightInner(SpatialRef);
impl Drop for SkyLightInner {
	fn drop(&mut self) {
		if let Ok(client) = self.0.client() {
			_ = set_sky_light(&client, None);
		}
	}
}
