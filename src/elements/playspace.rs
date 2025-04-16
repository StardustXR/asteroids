use crate::{Context, ValidState, custom::ElementTrait};
use stardust_xr_fusion::{
	node::{NodeError, NodeType},
	spatial::{Spatial, SpatialAspect, SpatialRef, Transform},
};
use std::{fmt::Debug, path::Path};

// TODO: implement bounds
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaySpace;
impl<State: ValidState> ElementTrait<State> for PlaySpace {
	type Inner = Spatial;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		spatial_parent: &SpatialRef,
		_context: &Context,
		_path: &Path,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let client = spatial_parent.client().unwrap();
		let spatial = Spatial::create(spatial_parent, Transform::identity(), false)?;
		tokio::spawn({
			let spatial = spatial.clone();
			async move {
				if let Some(play_space) = stardust_xr_fusion::objects::play_space(&client).await {
					spatial.set_spatial_parent(&play_space.spatial).unwrap();
				}
			}
		});
		Ok(spatial)
	}
	fn update(
		&self,
		_old_decl: &Self,
		_state: &mut State,
		_inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial_ref()
	}
}
