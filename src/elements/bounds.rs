use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
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
impl<State: ValidState> CustomElement<State> for Bounds<State> {
	type Inner = BoundsInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let (bounds_tx, bounds_rx) = mpsc::channel(1);
		let spatial = Spatial::create(info.parent_space, self.transform, false)?;

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

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		self.apply_transform(old_self, &inner.spatial);
	}

	fn frame(
		&self,
		_context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
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

#[tokio::test]
async fn asteroids_bounds_element() {
	use crate::{
		Reify,
		client::{self, ClientState},
		custom::CustomElement,
		elements::Bounds,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::spatial::BoundingBox;

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState {
		latest_bounds: Option<BoundingBox>,
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.bounds";
	}
	impl Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			let bounding_box = BoundingBox {
				center: [0.02, 0.5, 0.7].into(),
				size: [0.2, 0.6, 5.3].into(),
			};
			Bounds::new(move |state: &mut TestState, bounds| {
				assert!((bounds.center.x - bounding_box.center.x).abs() < 0.01);
				assert!((bounds.center.y - bounding_box.center.y).abs() < 0.01);
				assert!((bounds.center.z - bounding_box.center.z).abs() < 0.01);
				assert!((bounds.size.x - bounding_box.size.x).abs() < 0.01);
				assert!((bounds.size.y - bounding_box.size.y).abs() < 0.01);
				assert!((bounds.size.z - bounding_box.size.z).abs() < 0.01);
				state.latest_bounds.replace(bounds);
			})
			.build()
			.child(
				crate::elements::Lines::new(crate::elements::lines::bounding_box(
					bounding_box.clone(),
				))
				.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await
}
