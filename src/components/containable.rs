use crate::{
	CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState,
	components::{Grabbable, GrabbableInner},
};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	Error, client::FrameInfo, fields::FieldSample, query::QueryableId, spatial::SpatialRef,
};
use std::sync::Arc;

pub type Containers = FxHashMap<QueryableId, (FieldSample, SpatialRef)>;
type Evaluator = CloneFnWrapper<dyn Fn(&Containers) -> Option<SpatialRef> + Send + Sync>;

/// lets the entity get swallowed by any [`Container`](super::Container) it's moved into, riding
/// along with it afterwards
///
/// with a [`Grabbable`] alongside it, it waits until you let go before settling in, otherwise it
/// follows a container the moment it's inside one
#[derive(Debug, Clone)]
pub struct Containable {
	evaluator: Evaluator,
}
impl Default for Containable {
	fn default() -> Self {
		Self {
			evaluator: CloneFnWrapper(Arc::new(innermost_container)),
		}
	}
}
impl Containable {
	/// pick which of the containers it's currently inside to land in, `None` to go back home
	pub fn evaluator<F: Fn(&Containers) -> Option<SpatialRef> + Send + Sync + 'static>(
		mut self,
		f: F,
	) -> Self {
		self.evaluator = CloneFnWrapper(Arc::new(f));
		self
	}
}
impl<State: ValidState> Component<State> for Containable {
	type Inner = ContainableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let evaluator = self.evaluator.clone();
		// the anchor is what moves, but the query point rides the spatial so it follows the object
		// while it's being dragged
		let containable = stardust_xr_molecules::container::Containable::new(
			&context.stardust_client,
			info.anchor.clone(),
			info.parent_space.clone(),
			info.spatial.spatial_ref().await?,
			move |containers| (evaluator.0)(containers),
		)
		.await?;

		Ok(ContainableInner {
			containable: Arc::new(containable),
			grabbed: false,
		})
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		// read the sibling out first, Inners accessors borrow self so we can't hold both
		let grabbing = inners
			.get::<Grabbable<State>>()
			.map(GrabbableInner::grabbing);
		let inner = inners.self_inner();

		// merely having a Grabbable is what makes containment wait for a release
		inner.containable.set_auto_reparent(grabbing.is_none());

		let grabbing = grabbing.unwrap_or_default();
		if inner.grabbed && !grabbing {
			let containable = inner.containable.clone();
			tokio::spawn(async move { containable.reparent().await });
		}
		inner.grabbed = grabbing;
	}
}

pub struct ContainableInner {
	containable: Arc<stardust_xr_molecules::container::Containable>,
	grabbed: bool,
}

/// the tightest container the point is actually inside of, the default pick for a [`Containable`]
pub fn innermost_container(containers: &Containers) -> Option<SpatialRef> {
	containers
		.values()
		.filter(|(sample, _)| sample.distance < 0.0)
		.max_by(|(a, _), (b, _)| a.distance.total_cmp(&b.distance))
		.map(|(_, spatial)| spatial.clone())
}

#[tokio::test]
async fn asteroids_containable_component() {
	use crate::{
		Context, Entity, Tasker, Transformable,
		client::{self, ClientState},
		components::{Container, PointerMode},
		custom::CustomElement,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{
		fields::Shape,
		types::{Vec3F, rgba_linear},
	};
	use stardust_xr_molecules::lines::LineExt as _;

	#[derive(Debug, Serialize, Deserialize)]
	struct TestState {
		container: Vec3F,
		containable: Vec3F,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				container: [0.0, 0.0, -0.5].into(),
				containable: [0.0, 0.4, -0.5].into(),
			}
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.containable";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			let container = Shape::Box {
				size: [0.3; 3].into(),
			};
			let containable = Shape::Box {
				size: [0.05; 3].into(),
			};

			crate::elements::Spatial::default()
				.build()
				.child(
					Entity::new(container.clone())
						.pos(self.container)
						.component(Container)
						.component(
							Grabbable::new(|state: &mut Self, pose| {
								state.container = pose.position;
							})
							.pointer_mode(PointerMode::Move),
						)
						.build()
						.child(
							crate::elements::Lines::new(
								stardust_xr_molecules::lines::shape(container)
									.into_iter()
									.map(|l| {
										l.color(rgba_linear!(0.0, 0.75, 1.0, 1.0)).thickness(0.005)
									}),
							)
							.build(),
						),
				)
				.child(
					Entity::new(containable.clone())
						.pos(self.containable)
						.component(
							Grabbable::new(|state: &mut Self, pose| {
								state.containable = pose.position;
							})
							.pointer_mode(PointerMode::Move),
						)
						.component(Containable::default())
						.build()
						.child(
							crate::elements::Lines::new(
								stardust_xr_molecules::lines::shape(containable)
									.into_iter()
									.map(|l| {
										l.color(rgba_linear!(1.0, 0.5, 0.0, 1.0)).thickness(0.005)
									}),
							)
							.build(),
						),
				)
		}
	}

	client::run::<TestState>(&[]).await.unwrap()
}
