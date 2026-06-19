use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{
	Error,
	client::FrameInfo,
	spatial::{BoundingBox, Spatial, Transform},
	types::Vec3F,
};
use tokio::sync::watch;
use tokio::time::{Duration, timeout};

/// Closure that turns the child's current bounds into the transform to apply to it.
///
/// This is just an alias for the closure bound so it doesn't have to be spelled out
/// everywhere; any matching `Fn` implements it automatically.
pub trait Constrain: Fn(BoundingBox) -> Transform + Clone + Send + Sync + 'static {}
impl<F: Fn(BoundingBox) -> Transform + Clone + Send + Sync + 'static> Constrain for F {}

pub struct BoundsTransformerInner {
	spatial: Spatial,
	/// The last bounds we acted on, so we only push a new transform when they change.
	last_bounds: watch::Sender<Option<BoundingBox>>,
}

/// Watches the bounds of its children and feeds them through a closure to produce
/// a transform for the child space, every frame.
///
/// This lets you arbitrarily reshape content from its measured bounds: scale it to
/// fit a box, recenter it, nudge it up, etc. The transform is applied to the child
/// space the moment fresh bounds come back from the server, so there's no extra
/// frame of latency.
///
/// ```ignore
/// // Scale-to-fit a 10cm box:
/// BoundsTransformer::scale_to_fit([0.1; 3])
///
/// // Or anything you like:
/// BoundsTransformer::new(|bounds| Transform::from_translation([0.0, bounds.extents.y, 0.0]))
/// ```
pub struct BoundsTransformer<F: Constrain> {
	constrain: F,
}
impl<F: Constrain> std::fmt::Debug for BoundsTransformer<F> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BoundsTransformer").finish()
	}
}
impl<F: Constrain> BoundsTransformer<F> {
	/// Build a transformer from an arbitrary `bounds -> transform` closure.
	pub fn new(constrain: F) -> Self {
		Self { constrain }
	}
}
impl BoundsTransformer<fn(BoundingBox) -> Transform> {
	/// Uniformly scale the children so they fit within `max_size` (in meters),
	/// preserving aspect ratio.
	pub fn scale_to_fit(max_size: impl Into<Vec3F>) -> BoundsTransformer<impl Constrain> {
		let max = max_size.into();
		BoundsTransformer::new(move |bounds: BoundingBox| {
			let extents = bounds.extents;
			let factor = (max.x / extents.x)
				.min(max.y / extents.y)
				.min(max.z / extents.z);
			// Empty/degenerate bounds produce inf/NaN; fall back to no scaling.
			let factor = if factor.is_finite() { factor } else { 1.0 };
			Transform::from_scale([factor; 3])
		})
	}
}
impl<State: ValidState, F: Constrain> CustomElement<State> for BoundsTransformer<F> {
	type Inner = BoundsTransformerInner;
	type Error = Error;

	async fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let last_bounds = watch::Sender::new(None);

		// Apply an initial transform from whatever the children measure right now.
		if let Ok(bounds) = info.child_space.get_local_bounding_box().await {
			let _ = info
				.child_space
				.set_local_transform((self.constrain)(bounds));
			last_bounds.send_replace(Some(bounds));
		}

		Ok(BoundsTransformerInner {
			spatial: info.child_space,
			last_bounds,
		})
	}

	fn diff(&self, _old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		// Closures compare equal, so we can't tell if `constrain` actually changed.
		// Forget the last bounds to force a re-apply on the next poll in case it did.
		inner.last_bounds.send_replace(None);
	}

	fn frame(
		&self,
		_context: &Context,
		info: &FrameInfo,
		_state: &mut State,
		inner: &mut Self::Inner,
	) {
		let spatial = inner.spatial.clone();
		let last_bounds = inner.last_bounds.clone();
		let constrain = self.constrain.clone();
		let timeout_duration = Duration::from_secs_f32(info.delta * 2.0);

		tokio::spawn(async move {
			let Ok(Ok(bounds)) = timeout(timeout_duration, spatial.get_local_bounding_box()).await
			else {
				return;
			};

			// Atomically record the new bounds, only proceeding if they actually changed
			// (so racing frame tasks don't double-apply the same bounds).
			let changed = last_bounds.send_if_modified(|last| {
				if *last != Some(bounds) {
					*last = Some(bounds);
					true
				} else {
					false
				}
			});
			if changed {
				let _ = spatial.set_local_transform((constrain)(bounds));
			}
		});
	}
}

#[tokio::test]
async fn asteroids_bounds_element() {
	use crate::{
		Reify, Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::BoundsTransformer,
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
			BoundsTransformer::scale_to_fit([0.1; 3]).build().child(
				// race condition here, the inner can be made after checking for bounds!
				crate::elements::Lines::new(crate::elements::lines::bounding_box(expected_bounds))
					.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
