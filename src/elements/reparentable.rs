use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use derive_setters::Setters;
use stardust_xr_fusion::{
	node::NodeError,
	spatial::{Spatial, SpatialRef, Transform},
};
use std::{fmt::Debug, path::PathBuf};
use zbus::Connection;

#[derive(Default, Debug, Clone, Copy, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Reparentable {
	enabled: bool,
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
