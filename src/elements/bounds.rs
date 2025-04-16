use std::path::Path;

use crate::{
	Context, ValidState,
	custom::{ElementTrait, Transformable},
};
use stardust_xr_fusion::{
	node::NodeError,
	root::FrameInfo,
	spatial::{BoundingBox, Spatial, SpatialRef, SpatialRefAspect, Transform},
};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};

pub struct BoundsInner {
	spatial: Spatial,
	previous_bounds: Option<BoundingBox>,
	bounds_tx: mpsc::Sender<BoundingBox>,
	bounds_rx: mpsc::Receiver<BoundingBox>,
}

type OnBoundsChange<State> = Box<dyn Fn(&mut State, BoundingBox) + Send + Sync>;
pub struct Bounds<State: ValidState> {
	transform: Transform,
	on_bounds_change: OnBoundsChange<State>,
}
impl<State: ValidState> std::fmt::Debug for Bounds<State> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Bounds").finish()
	}
}
impl<State: ValidState> Bounds<State> {
	pub fn new<F: Fn(&mut State, BoundingBox) + Send + Sync + 'static>(
		on_bounds_change: F,
	) -> Self {
		Self {
			transform: Transform::identity(),
			on_bounds_change: Box::new(on_bounds_change),
		}
	}
}
impl<State: ValidState> ElementTrait<State> for Bounds<State> {
	type Inner = BoundsInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_context: &Context,
		_path: &Path,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let (bounds_tx, bounds_rx) = mpsc::channel(1);
		let spatial = Spatial::create(parent_space, self.transform, false)?;

		tokio::spawn({
			let spatial = spatial.clone();
			let tx = bounds_tx.clone();
			async move {
				if let Ok(bounds) = spatial.get_local_bounding_box().await {
					let _ = tx.send(bounds).await;
				}
			}
		});
		Ok(BoundsInner {
			spatial,
			previous_bounds: None,
			bounds_tx,
			bounds_rx,
		})
	}

	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, &inner.spatial);
	}

	fn frame(&self, info: &FrameInfo, state: &mut State, inner: &mut Self::Inner) {
		// Check if we have new bounds
		if let Ok(current_bounds) = inner.bounds_rx.try_recv() {
			if inner.previous_bounds.as_ref() != Some(&current_bounds) {
				(self.on_bounds_change)(state, current_bounds.clone());
				inner.previous_bounds = Some(current_bounds);
			}
		}

		// Spawn a task to check bounds for next frame with timeout
		let spatial = inner.spatial.clone();
		let tx = inner.bounds_tx.clone();
		let timeout_duration = Duration::from_secs_f32(info.delta * 2.0);

		tokio::spawn(async move {
			let bounds_future = spatial.get_local_bounding_box();
			if let Ok(Ok(bounds)) = timeout(timeout_duration, bounds_future).await {
				let _ = tx.send(bounds).await;
			}
		});
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.spatial.clone().as_spatial_ref()
	}
}
impl<State: ValidState> Transformable for Bounds<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
