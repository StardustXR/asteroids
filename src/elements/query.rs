use std::ops::{Deref, DerefMut};

use crate::{CustomElement, FnWrapper, ValidState};
use gluon_ipc::{Handler, Node, RefExt};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	fields::{FieldRef, FieldSample},
	query::{InterfaceDependency, QueriedInterface, QueryableId},
	spatial::{Spatial, SpatialRef},
	spatial_query::{
		self, Point, PointsQueryHandle, PointsQueryHandler, PointsQueryHandlerHandler,
	},
	types::Vec3F,
};
use tokio::sync::mpsc;

pub trait QueryableInterfaces: Sized + 'static {
	fn interface_dependencies() -> Vec<InterfaceDependency>;
	fn from_queried(interfaces: &[QueriedInterface]) -> Option<Self>;
}

trait QueryableInterface: Sized + 'static {
	fn interface_dependency() -> InterfaceDependency;
	fn from_queried(interfaces: &[QueriedInterface]) -> Option<Self>;
}
impl<I: RefExt> QueryableInterface for I {
	fn interface_dependency() -> InterfaceDependency {
		InterfaceDependency {
			id: I::ID.to_string(),
			optional: false,
		}
	}
	fn from_queried(interfaces: &[QueriedInterface]) -> Option<Self> {
		interfaces
			.iter()
			.find(|i| i.interface_id == I::ID)
			.map(|i| I::from_ref(i.interface.clone()))
	}
}

// a local wrapper since a blanket impl over Option<I> would overlap with the one over I
#[derive(Debug, Clone)]
pub struct Optional<I>(pub Option<I>);
impl<I> Deref for Optional<I> {
	type Target = Option<I>;

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}
impl<I> DerefMut for Optional<I> {
	fn deref_mut(&mut self) -> &mut Self::Target {
		&mut self.0
	}
}
impl<I: RefExt> QueryableInterface for Optional<I> {
	fn interface_dependency() -> InterfaceDependency {
		InterfaceDependency {
			id: I::ID.to_string(),
			optional: true,
		}
	}
	fn from_queried(interfaces: &[QueriedInterface]) -> Option<Self> {
		Some(Optional(I::from_queried(interfaces)))
	}
}

macro_rules! impl_queryable_interfaces {
	() => {};
	($first:ident $(, $rest:ident)*) => {
		impl<$first: QueryableInterface, $($rest: QueryableInterface),*> QueryableInterfaces for ($first, $($rest,)*) {
			fn interface_dependencies() -> Vec<InterfaceDependency> {
				vec![$first::interface_dependency(), $($rest::interface_dependency()),*]
			}
			fn from_queried(interfaces: &[QueriedInterface]) -> Option<Self> {
				Some(($first::from_queried(interfaces)?, $($rest::from_queried(interfaces)?,)*))
			}
		}
		impl_queryable_interfaces!($($rest),*);
	};
}
impl_queryable_interfaces!(I1, I2, I3, I4, I5, I6, I7, I8, I9, I10, I11, I12);

#[derive(Debug, Clone)]
pub struct SampledQueryable<I: QueryableInterfaces> {
	pub spatial: SpatialRef,
	pub field: FieldRef,
	pub interfaces: I,
	pub sample: FieldSample,
}

// derive_where so the default doesn't demand I: Default, which no interface proxy has
#[derive(Debug, Clone)]
#[derive_where::derive_where(Default)]
pub struct SampleQueryCache<I: QueryableInterfaces>(
	pub FxHashMap<QueryableId, SampledQueryable<I>>,
);

pub type OnEnteredSampled<State, I> =
	FnWrapper<dyn Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, FieldSample) + Send + Sync>;
pub type OnInterfacesChangedSampled<State, I> =
	FnWrapper<dyn Fn(&mut State, QueryableId, I) + Send + Sync>;
pub type OnMovedSampled<State> =
	FnWrapper<dyn Fn(&mut State, QueryableId, FieldSample) + Send + Sync>;
pub type OnLeftSampled<State> = FnWrapper<dyn Fn(&mut State, QueryableId) + Send + Sync>;

pub struct PointsQuery<State: ValidState, I: QueryableInterfaces> {
	points: Vec<Vec3F>,
	margin: f32,
	on_entered: OnEnteredSampled<State, I>,
	on_interfaces_changed: OnInterfacesChangedSampled<State, I>,
	on_moved: OnMovedSampled<State>,
	on_left: OnLeftSampled<State>,
}
impl<State: ValidState, I: QueryableInterfaces> PointsQuery<State, I> {
	pub fn new<P: Into<Vec3F>>(
		points: impl IntoIterator<Item = P>,
		on_entered: impl Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, FieldSample)
		+ Send
		+ Sync
		+ 'static,
		on_interfaces_changed: impl Fn(&mut State, QueryableId, I) + Send + Sync + 'static,
		on_moved: impl Fn(&mut State, QueryableId, FieldSample) + Send + Sync + 'static,
		on_left: impl Fn(&mut State, QueryableId) + Send + Sync + 'static,
	) -> Self {
		Self {
			points: points.into_iter().map(|i| i.into()).collect(),
			margin: 0.0,
			on_entered: FnWrapper(Box::new(on_entered)),
			on_interfaces_changed: FnWrapper(Box::new(on_interfaces_changed)),
			on_moved: FnWrapper(Box::new(on_moved)),
			on_left: FnWrapper(Box::new(on_left)),
		}
	}
	pub fn new_cached<P: Into<Vec3F>>(
		points: impl IntoIterator<Item = P>,
		cache: impl Fn(&mut State) -> &mut SampleQueryCache<I> + Clone + Send + Sync + 'static,
	) -> Self {
		let entered = cache.clone();
		let changed = cache.clone();
		let moved = cache.clone();
		Self::new(
			points,
			move |state, id, field, spatial, interfaces, sample| {
				entered(state).0.insert(
					id,
					SampledQueryable {
						spatial,
						field,
						interfaces,
						sample,
					},
				);
			},
			move |state, id, interfaces| {
				if let Some(q) = changed(state).0.get_mut(&id) {
					q.interfaces = interfaces;
				}
			},
			move |state, id, sample| {
				if let Some(q) = moved(state).0.get_mut(&id) {
					q.sample = sample;
				}
			},
			move |state, id| {
				cache(state).0.remove(&id);
			},
		)
	}
	pub fn margin(mut self, margin: f32) -> Self {
		self.margin = margin;
		self
	}

	fn query_points(&self) -> Vec<Point> {
		self.points
			.iter()
			.map(|&point| Point {
				point,
				margin: self.margin,
			})
			.collect()
	}
}
impl<State: ValidState, I: QueryableInterfaces> std::fmt::Debug for PointsQuery<State, I> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("PointsQuery")
			.field("points", &self.points)
			.field("margin", &self.margin)
			.finish()
	}
}
enum QueryEvent {
	Entered(
		QueryableId,
		FieldRef,
		SpatialRef,
		Vec<QueriedInterface>,
		FieldSample,
	),
	InterfacesChanged(QueryableId, Vec<QueriedInterface>),
	Moved(QueryableId, FieldSample),
	Left(QueryableId),
}

pub struct PointsQueryElementInner {
	_space: Spatial,
	_node: Node<PointsQueryInner>,
	handle: PointsQueryHandle,
	events: mpsc::UnboundedReceiver<QueryEvent>,
}
impl<State: ValidState, I: QueryableInterfaces> CustomElement<State> for PointsQuery<State, I> {
	type Inner = PointsQueryElementInner;
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		context: &crate::Context,
		info: crate::CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (tx, events) = mpsc::unbounded_channel();
		let (node, handler) = PointsQueryHandler::new_node(PointsQueryInner { tx })?;
		let handle = context
			.stardust_client
			.spatial_query_interface()
			.points_query(spatial_query::PointsQuery {
				handler: handler.into(),
				interfaces: I::interface_dependencies(),
				reference_spatial: info.parent_space,
				points: self.query_points(),
			})
			.await??;
		Ok(PointsQueryElementInner {
			_space: info.child_space,
			_node: node,
			handle,
			events,
		})
	}

	fn diff(&self, old_self: &Self, _context: &crate::Context, inner: &mut Self::Inner) {
		if self.points != old_self.points || self.margin != old_self.margin {
			let _ = inner.handle.update(self.query_points());
		}
	}

	fn frame(
		&self,
		_context: &crate::Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		while let Ok(event) = inner.events.try_recv() {
			match event {
				QueryEvent::Entered(id, field, spatial, interfaces, sample) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_entered.0)(state, id, field, spatial, interfaces, sample);
					}
				}
				QueryEvent::InterfacesChanged(id, interfaces) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_interfaces_changed.0)(state, id, interfaces);
					}
				}
				QueryEvent::Moved(id, sample) => (self.on_moved.0)(state, id, sample),
				QueryEvent::Left(id) => (self.on_left.0)(state, id),
			}
		}
	}
}

#[derive(Handler)]
pub struct PointsQueryInner {
	tx: mpsc::UnboundedSender<QueryEvent>,
}
impl PointsQueryHandlerHandler for PointsQueryInner {
	async fn entered(
		&self,
		_ctx: gluon_ipc::Context,
		obj: QueryableId,
		field: FieldRef,
		spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		spatial_info: FieldSample,
	) {
		let _ = self.tx.send(QueryEvent::Entered(
			obj,
			field,
			spatial,
			interfaces,
			spatial_info,
		));
	}

	async fn interfaces_changed(
		&self,
		_ctx: gluon_ipc::Context,
		obj: QueryableId,
		interfaces: Vec<QueriedInterface>,
	) {
		let _ = self.tx.send(QueryEvent::InterfacesChanged(obj, interfaces));
	}

	async fn moved(&self, _ctx: gluon_ipc::Context, obj: QueryableId, spatial_info: FieldSample) {
		let _ = self.tx.send(QueryEvent::Moved(obj, spatial_info));
	}

	async fn left(&self, _ctx: gluon_ipc::Context, obj: QueryableId) {
		let _ = self.tx.send(QueryEvent::Left(obj));
	}
}

#[tokio::test]
async fn asteroids_points_query_element() {
	use crate::{
		Context, Entity, Tasker,
		client::{self, ClientState},
		components::Derezzable,
		custom::{CustomElement, Transformable},
		elements::{GrabRing, Lines, Spatial},
	};
	use mint::Vector3;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{fields::Shape, types::rgba_linear};
	use stardust_xr_molecules::{
		derezzable::protocol::Derezzable as DerezzableProxy,
		lines::{LineExt, line_from_points, shape},
	};

	#[derive(Serialize, Deserialize)]
	struct TestState {
		center: Vector3<f32>,
		#[serde(skip)]
		derezzables: SampleQueryCache<(DerezzableProxy,)>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				center: [0.0; 3].into(),
				derezzables: SampleQueryCache::default(),
			}
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.points_query";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
			_props: (),
		) -> impl crate::Element<Self> {
			let target = Shape::Box {
				size: [0.1; 3].into(),
			};
			Spatial::default()
				.build()
				.child(
					GrabRing::new(self.center, |s: &mut Self, p| s.center = p)
						.build()
						.child(
							PointsQuery::new_cached([[0.0; 3]], |s: &mut Self| &mut s.derezzables)
								.margin(1.0)
								.build()
								.child(
									Lines::new(self.derezzables.0.values().map(|q| {
										line_from_points(vec![
											[0.0; 3].into(),
											q.sample.closest_point,
										])
										.color(rgba_linear!(0.1, 1.0, 0.1, 1.0))
										.thickness(0.005)
									}))
									.build(),
								),
						),
				)
				.child(
					Entity::new(target.clone())
						.pos([0.3, 0.0, 0.0])
						.component(Derezzable::new(|_| {}))
						.build()
						.child(
							Lines::new(shape(target).into_iter().map(|l| {
								l.color(rgba_linear!(1.0, 0.1, 0.1, 1.0)).thickness(0.005)
							}))
							.build(),
						),
				)
		}
	}
	client::run::<TestState>(&[]).await.unwrap()
}
