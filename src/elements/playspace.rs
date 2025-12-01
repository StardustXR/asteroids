use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{
	node::{NodeError, NodeType},
	spatial::{Spatial, SpatialAspect, SpatialRef, Transform},
};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaySpace;
impl<State: ValidState> CustomElement<State> for PlaySpace {
	type Inner = Spatial;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let client = info.parent_space.client().clone();
		let spatial = Spatial::create(info.parent_space, Transform::identity())?;
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
	fn diff(&self, _old_self: &Self, _inner: &mut Self::Inner, _resource: &mut Self::Resource) {}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial_ref()
	}
}

#[tokio::test]
async fn asteroids_playspace_element() {
	use crate::{
		client::{self, ClientState},
		elements::PlaySpace,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState;

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.playspace";
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			PlaySpace
				.build()
				.child(crate::elements::Lines::new([crate::elements::circle(4, 0.0, 0.1)]).build())
		}
	}

	client::run::<TestState>(&[]).await
}
