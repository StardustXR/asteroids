use stardust_xr_fusion::{
	drawable::set_sky_tex,
	node::{NodeError, NodeType},
	spatial::SpatialRef,
	values::ResourceID,
};

use crate::{custom::ElementTrait, ValidState};

#[derive(Debug)]
pub struct SkyTexture(pub ResourceID);
impl<State: ValidState> ElementTrait<State> for SkyTexture {
	type Inner = SpatialRef;

	type Resource = ();

	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &stardust_xr_fusion::spatial::SpatialRef,
		_dbus_connection: &stardust_xr_fusion::core::schemas::zbus::Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		set_sky_tex(&parent_space.client()?, &self.0)?;
		Ok(parent_space.clone())
	}

	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if self.0 != old_decl.0 {
			if let Ok(client) = inner.client() {
				_ = set_sky_tex(&client, &self.0);
			}
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> stardust_xr_fusion::spatial::SpatialRef {
		inner.clone()
	}
}
