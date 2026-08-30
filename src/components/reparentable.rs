use crate::{Component, ComponentCreateInfo, Context, Inners, ValidState};
use derive_setters::Setters;
use stardust_xr_fusion::{
	Error, Result,
	fields::Field,
	spatial::{Spatial, SpatialRef},
};
use tokio::sync::mpsc;

#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct Reparentable {
	enabled: bool,
}
impl Default for Reparentable {
	fn default() -> Self {
		Self { enabled: true }
	}
}

struct ActiveReparentable {
	// the entity owns the shared field; we just keep the molecules reparentable alive
	_reparentable: stardust_xr_molecules::reparentable::Reparentable,
}

async fn make_active(
	context: &Context,
	spatial: Spatial,
	parent: &SpatialRef,
	field: Field,
) -> Result<ActiveReparentable> {
	let reparentable = stardust_xr_molecules::reparentable::Reparentable::new(
		&context.stardust_client,
		spatial,
		parent.clone(),
		field,
	)
	.await?;
	Ok(ActiveReparentable {
		_reparentable: reparentable,
	})
}

pub struct ReparentableInner {
	context: Context,
	spatial: Spatial,
	parent_space: SpatialRef,
	field: Field,
	active: Option<ActiveReparentable>,
	pending_tx: mpsc::UnboundedSender<ActiveReparentable>,
	pending_rx: mpsc::UnboundedReceiver<ActiveReparentable>,
}

impl<State: ValidState> Component<State> for Reparentable {
	type Inner = ReparentableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner> {
		let active = if self.enabled {
			Some(make_active(context, info.spatial.clone(), info.parent_space, info.field.clone()).await?)
		} else {
			None
		};
		let (pending_tx, pending_rx) = mpsc::unbounded_channel();
		Ok(ReparentableInner {
			context: context.clone(),
			spatial: info.spatial.clone(),
			parent_space: info.parent_space.clone(),
			field: info.field.clone(),
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
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		while let Ok(active) = inner.pending_rx.try_recv() {
			inner.active = Some(active);
		}
	}

	fn diff(
		&self,
		old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		if self.enabled != old_self.enabled {
			if self.enabled {
				let context = inner.context.clone();
				let spatial = inner.spatial.clone();
				let parent = inner.parent_space.clone();
				let field = inner.field.clone();
				let tx = inner.pending_tx.clone();
				tokio::spawn(async move {
					if let Ok(active) = make_active(&context, spatial, &parent, field).await {
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
		Context, Entity, Tasker, Transformable,
		client::{self, ClientState},
		custom::CustomElement,
		elements::Lines,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{fields::Shape, spatial::BoundingBox};
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
			Entity::new(Shape::Sphere { radius: 0.05 })
				.component(Reparentable::default())
				.build()
				.child(
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
