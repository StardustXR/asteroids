use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use derive_setters::Setters;
use stardust_xr_fusion::{
	Error, Result,
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, SpatialRef},
};
use std::fmt::Debug;
use tokio::sync::mpsc;

#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct Reparentable {
	enabled: bool,
	pub shape: Shape,
}
impl Default for Reparentable {
	fn default() -> Self {
		Self {
			enabled: true,
			shape: Shape::Sphere { radius: 0.05 },
		}
	}
}

struct ActiveReparentable {
	_field: Field,
	_reparentable: stardust_xr_molecules::reparentable::Reparentable,
}

async fn make_active(
	context: &Context,
	spatial: Spatial,
	parent: &SpatialRef,
	shape: Shape,
) -> Result<ActiveReparentable> {
	let (field, _) = Field::create(&context.stardust_client, &spatial, shape).await?;
	let reparentable = stardust_xr_molecules::reparentable::Reparentable::new(
		&context.stardust_client,
		spatial,
		parent.clone(),
		field.clone(),
	)
	.await?;
	Ok(ActiveReparentable {
		_field: field,
		_reparentable: reparentable,
	})
}

pub struct ReparentableInner {
	context: Context,
	child_space: Spatial,
	parent_space: SpatialRef,
	active: Option<ActiveReparentable>,
	pending_tx: mpsc::UnboundedSender<ActiveReparentable>,
	pending_rx: mpsc::UnboundedReceiver<ActiveReparentable>,
}

impl<State: ValidState> CustomElement<State> for Reparentable {
	type Inner = ReparentableInner;
	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let active = if self.enabled {
			Some(
				make_active(
					context,
					info.child_space.clone(),
					&info.parent_space,
					self.shape.clone(),
				)
				.await?,
			)
		} else {
			None
		};
		let (pending_tx, pending_rx) = mpsc::unbounded_channel();
		Ok(ReparentableInner {
			context: context.clone(),
			child_space: info.child_space,
			parent_space: info.parent_space.clone(),
			active,
			pending_tx,
			pending_rx,
		})
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		_state: &mut State,
		inner: &mut Self::Inner,
	) {
		while let Ok(active) = inner.pending_rx.try_recv() {
			inner.active = Some(active);
		}
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.enabled != old_self.enabled {
			if self.enabled {
				let context = inner.context.clone();
				let spatial = inner.child_space.clone();
				let parent = inner.parent_space.clone();
				let shape = self.shape.clone();
				let tx = inner.pending_tx.clone();
				tokio::spawn(async move {
					if let Ok(active) = make_active(&context, spatial, &parent, shape).await {
						let _ = tx.send(active);
					}
				});
			} else {
				inner.active.take();
			}
		}
	}
}

#[tokio::test]
async fn asteroids_reparentable_element() {
	use crate::{
		Tasker, Transformable,
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
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Reparentable::default().build().child(
				Lines::new(
					bounding_box(BoundingBox {
						center: [0.0; 3].into(),
						extents: [0.05; 3].into(),
					})
					.into_iter()
					.map(|l| l.thickness(0.002)),
				)
				.pos([0.0, 0.025, 0.0])
				.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
