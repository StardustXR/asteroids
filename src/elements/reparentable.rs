use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use derive_setters::Setters;
use stardust_xr_fusion::{
	node::NodeError,
	spatial::{Spatial, SpatialRef, Transform},
};
use std::{fmt::Debug, path::PathBuf};
use zbus::Connection;

#[derive(Debug, Clone, Copy, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Reparentable {
	enabled: bool,
}
impl Default for Reparentable {
	fn default() -> Self {
		Self { enabled: true }
	}
}
impl<State: ValidState> CustomElement<State> for Reparentable {
	type Inner = ReparentableInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let spatial = Spatial::create(info.parent_space, Transform::identity(), false)?;
		Ok(ReparentableInner {
			connection: context.dbus_connection.clone(),
			outer_spatial: info.parent_space.clone(),
			inner_spatial: spatial,
			path: info.element_path.to_path_buf(),
			reparentable: None,
		})
	}
	fn diff(&self, _old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.enabled {
			inner.enable();
		} else {
			inner.disable();
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.spatial()
	}
}
pub struct ReparentableInner {
	connection: Connection,
	outer_spatial: SpatialRef,
	inner_spatial: Spatial,
	path: PathBuf,
	reparentable: Option<stardust_xr_molecules::reparentable::Reparentable>,
}
impl ReparentableInner {
	fn enable(&mut self) {
		if self.reparentable.is_none() {
			self.reparentable = stardust_xr_molecules::reparentable::Reparentable::create(
				self.connection.clone(),
				self.path.clone(),
				self.outer_spatial.clone(),
				self.inner_spatial.clone(),
				None,
			)
			.ok();
		}
	}
	fn disable(&mut self) {
		if self.reparentable.is_some() {
			self.reparentable.take();
		}
	}
	fn spatial(&self) -> SpatialRef {
		self.inner_spatial.clone().as_spatial_ref()
	}
}
#[tokio::test]
async fn asteroids_reparentable_element() {
	use crate::{
		Transformable,
		client::{self, ClientState},
		custom::CustomElement,
		elements::Lines,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::spatial::BoundingBox;
	use stardust_xr_molecules::lines::{LineExt, bounding_box};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState;
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.turntable";
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			Reparentable::default().build().child(
				Lines::new(
					bounding_box(BoundingBox {
						center: [0.0; 3].into(),
						size: [0.05; 3].into(),
					})
					.into_iter()
					.map(|l| l.thickness(0.002)),
				)
				.pos([0.0, 0.025, 0.0])
				.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await
}
