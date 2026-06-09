use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use stardust_xr_fusion::{
	Error,
	client::FrameInfo,
	spatial::{BoundingBox, Spatial, Transform},
	types::Vec3F,
};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

pub struct SizeConstrainerInner {
	spatial: Spatial,
	previous_bounds: BoundingBox,
	bounds_tx: watch::Sender<BoundingBox>,
	bounds_rx: watch::Receiver<BoundingBox>,
}

pub struct SizeConstrainer {
	transform: Transform,
	max_size: Vec3F,
}
impl std::fmt::Debug for SizeConstrainer {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Bounds").finish()
	}
}
impl SizeConstrainer {
	pub fn new(max_size: impl Into<Vec3F>) -> Self {
		Self {
			transform: Transform::IDENTITY,
			max_size: max_size.into(),
		}
	}
}
impl<State: ValidState> CustomElement<State> for SizeConstrainer {
	type Inner = SizeConstrainerInner;
	type Error = Error;

	async fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (bounds_tx, bounds_rx) = watch::channel(BoundingBox {
			center: [0.0; 3].into(),
			extents: [0.0; 3].into(),
		});

		if let Ok(bounds) = info.child_space.get_local_bounding_box().await {
			let _ = bounds_tx.send(bounds);
		}
		Ok(SizeConstrainerInner {
			spatial: info.child_space,
			previous_bounds: BoundingBox {
				center: [0.0; 3].into(),
				extents: [0.0; 3].into(),
			},
			bounds_tx,
			bounds_rx,
		})
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.spatial);
	}

	fn frame(
		&self,
		_context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		let current_bounds = *inner.bounds_rx.borrow();
		// Check if we have new bounds
		if inner.previous_bounds != current_bounds {
			let scale_factor_x = self.max_size.x / current_bounds.extents.x;
			let scale_factor_y = self.max_size.y / current_bounds.extents.y;
			let scale_factor_z = self.max_size.z / current_bounds.extents.z;

			inner.previous_bounds = current_bounds;
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
}
impl Transformable for SizeConstrainer {
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
		Reify, Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::SizeConstrainer,
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
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			let expected_bounds = BoundingBox {
				center: [0.02, 0.5, 0.7].into(),
				extents: [0.2, 0.6, 5.3].into(),
			};
			SizeConstrainer::new([0.1; 3]).build().child(
				// race condition here, the inner can be made after checking for bounds!
				crate::elements::Lines::new(crate::elements::lines::bounding_box(expected_bounds))
					.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
