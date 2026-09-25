use std::ops::{Deref, DerefMut};

use crate::{CustomElement, FnWrapper, ValidState};
use gluon_ipc::{Handler, Node, RefExt};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	fields::{FieldRef, FieldSample, RayMarchResult},
	query::{InterfaceDependency, QueriedInterface, QueryableId},
	spatial::{Spatial, SpatialRef},
	spatial_query::{
		self, BeamQueryHandle, BeamQueryHandler, BeamQueryHandlerHandler, Point, PointsQueryHandle,
		PointsQueryHandler, PointsQueryHandlerHandler,
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
pub struct PointsQueryCache<I: QueryableInterfaces>(
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
		cache: impl Fn(&mut State) -> &mut PointsQueryCache<I> + Clone + Send + Sync + 'static,
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
// S is whatever each query type reports per object
pub(crate) enum QueryEvent<S> {
	Entered(QueryableId, FieldRef, SpatialRef, Vec<QueriedInterface>, S),
	InterfacesChanged(QueryableId, Vec<QueriedInterface>),
	Moved(QueryableId, S),
	Left(QueryableId),
}

pub struct PointsQueryElementInner {
	_space: Spatial,
	_node: Node<PointsQueryInner>,
	handle: PointsQueryHandle,
	events: mpsc::UnboundedReceiver<QueryEvent<FieldSample>>,
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
	tx: mpsc::UnboundedSender<QueryEvent<FieldSample>>,
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

#[derive(Debug, Clone)]
pub struct IntersectedQueryable<I: QueryableInterfaces> {
	pub spatial: SpatialRef,
	pub field: FieldRef,
	pub interfaces: I,
	pub ray: RayMarchResult,
}

#[derive(Debug, Clone)]
#[derive_where::derive_where(Default)]
pub struct BeamQueryCache<I: QueryableInterfaces>(
	pub FxHashMap<QueryableId, IntersectedQueryable<I>>,
);

pub type OnIntersected<State, I> = FnWrapper<
	dyn Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, RayMarchResult) + Send + Sync,
>;
pub type OnInterfacesChangedIntersected<State, I> =
	FnWrapper<dyn Fn(&mut State, QueryableId, I) + Send + Sync>;
pub type OnMovedIntersected<State> =
	FnWrapper<dyn Fn(&mut State, QueryableId, RayMarchResult) + Send + Sync>;
pub type OnLeftIntersected<State> = FnWrapper<dyn Fn(&mut State, QueryableId) + Send + Sync>;

pub struct BeamQuery<State: ValidState, I: QueryableInterfaces> {
	origin: Vec3F,
	direction: Vec3F,
	max_length: f32,
	margin: f32,
	on_intersected: OnIntersected<State, I>,
	on_interfaces_changed: OnInterfacesChangedIntersected<State, I>,
	on_moved: OnMovedIntersected<State>,
	on_left: OnLeftIntersected<State>,
}
impl<State: ValidState, I: QueryableInterfaces> BeamQuery<State, I> {
	pub fn new(
		origin: impl Into<Vec3F>,
		direction: impl Into<Vec3F>,
		on_intersected: impl Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, RayMarchResult)
		+ Send
		+ Sync
		+ 'static,
		on_interfaces_changed: impl Fn(&mut State, QueryableId, I) + Send + Sync + 'static,
		on_moved: impl Fn(&mut State, QueryableId, RayMarchResult) + Send + Sync + 'static,
		on_left: impl Fn(&mut State, QueryableId) + Send + Sync + 'static,
	) -> Self {
		Self {
			origin: origin.into(),
			direction: direction.into(),
			max_length: f32::MAX,
			margin: 0.0,
			on_intersected: FnWrapper(Box::new(on_intersected)),
			on_interfaces_changed: FnWrapper(Box::new(on_interfaces_changed)),
			on_moved: FnWrapper(Box::new(on_moved)),
			on_left: FnWrapper(Box::new(on_left)),
		}
	}
	pub fn new_cached(
		origin: impl Into<Vec3F>,
		direction: impl Into<Vec3F>,
		cache: impl Fn(&mut State) -> &mut BeamQueryCache<I> + Clone + Send + Sync + 'static,
	) -> Self {
		let intersected = cache.clone();
		let changed = cache.clone();
		let moved = cache.clone();
		Self::new(
			origin,
			direction,
			move |state, id, field, spatial, interfaces, ray| {
				intersected(state).0.insert(
					id,
					IntersectedQueryable {
						spatial,
						field,
						interfaces,
						ray,
					},
				);
			},
			move |state, id, interfaces| {
				if let Some(q) = changed(state).0.get_mut(&id) {
					q.interfaces = interfaces;
				}
			},
			move |state, id, ray| {
				if let Some(q) = moved(state).0.get_mut(&id) {
					q.ray = ray;
				}
			},
			move |state, id| {
				cache(state).0.remove(&id);
			},
		)
	}
	pub fn max_length(mut self, max_length: f32) -> Self {
		self.max_length = max_length;
		self
	}
	pub fn margin(mut self, margin: f32) -> Self {
		self.margin = margin;
		self
	}
}
impl<State: ValidState, I: QueryableInterfaces> std::fmt::Debug for BeamQuery<State, I> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("BeamQuery")
			.field("origin", &self.origin)
			.field("direction", &self.direction)
			.field("max_length", &self.max_length)
			.field("margin", &self.margin)
			.finish()
	}
}

pub struct BeamQueryElementInner {
	_space: Spatial,
	_node: Node<BeamQueryInner>,
	handle: BeamQueryHandle,
	events: mpsc::UnboundedReceiver<QueryEvent<RayMarchResult>>,
}
impl<State: ValidState, I: QueryableInterfaces> CustomElement<State> for BeamQuery<State, I> {
	type Inner = BeamQueryElementInner;
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		context: &crate::Context,
		info: crate::CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (tx, events) = mpsc::unbounded_channel();
		let (node, handler) = BeamQueryHandler::new_node(BeamQueryInner { tx })?;
		let handle = context
			.stardust_client
			.spatial_query_interface()
			.beam_query(spatial_query::BeamQuery {
				handler: handler.into(),
				interfaces: I::interface_dependencies(),
				reference_spatial: info.parent_space,
				origin: self.origin,
				direction: self.direction,
				max_length: self.max_length,
				margin: self.margin,
			})
			.await??;
		Ok(BeamQueryElementInner {
			_space: info.child_space,
			_node: node,
			handle,
			events,
		})
	}

	fn diff(&self, old_self: &Self, _context: &crate::Context, inner: &mut Self::Inner) {
		if self.origin != old_self.origin
			|| self.direction != old_self.direction
			|| self.max_length != old_self.max_length
			|| self.margin != old_self.margin
		{
			let _ = inner
				.handle
				.update(self.origin, self.direction, self.max_length, self.margin);
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
				QueryEvent::Entered(id, field, spatial, interfaces, ray) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_intersected.0)(state, id, field, spatial, interfaces, ray);
					}
				}
				QueryEvent::InterfacesChanged(id, interfaces) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_interfaces_changed.0)(state, id, interfaces);
					}
				}
				QueryEvent::Moved(id, ray) => (self.on_moved.0)(state, id, ray),
				QueryEvent::Left(id) => (self.on_left.0)(state, id),
			}
		}
	}
}

#[derive(Handler)]
pub struct BeamQueryInner {
	tx: mpsc::UnboundedSender<QueryEvent<RayMarchResult>>,
}
impl BeamQueryHandlerHandler for BeamQueryInner {
	async fn intersected(
		&self,
		_ctx: gluon_ipc::Context,
		obj: QueryableId,
		field: FieldRef,
		spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		spatial_info: RayMarchResult,
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

	async fn moved(
		&self,
		_ctx: gluon_ipc::Context,
		obj: QueryableId,
		spatial_info: RayMarchResult,
	) {
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
		derezzables: PointsQueryCache<(DerezzableProxy,)>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				center: [0.0; 3].into(),
				derezzables: PointsQueryCache::default(),
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

#[tokio::test]
async fn asteroids_beam_query_element() {
	use crate::{
		Context, Entity, Tasker,
		client::{self, ClientState},
		components::{Derezzable, Grabbable},
		custom::{CustomElement, Transformable},
		elements::{Lines, Spatial},
	};
	use glam::Vec3;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{
		fields::Shape,
		types::{Posef, rgba_linear},
	};
	use stardust_xr_molecules::{
		derezzable::protocol::Derezzable as DerezzableProxy,
		lines::{LineExt, line_from_points, shape},
	};

	#[derive(Serialize, Deserialize)]
	struct TestState {
		targets: [Posef; 3],
		#[serde(skip)]
		derezzables: BeamQueryCache<(DerezzableProxy,)>,
		#[serde(skip)]
		positions: FxHashMap<QueryableId, Vec3>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				targets: [[0.15, 0.3, -0.5], [-0.2, 0.6, -0.5], [0.0, 0.8, -0.35]].map(|p| Posef {
					position: p.into(),
					..Default::default()
				}),
				derezzables: BeamQueryCache::default(),
				positions: FxHashMap::default(),
			}
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.beam_query";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			context: &Context,
			tasks: impl Tasker<Self>,
			_props: (),
		) -> impl crate::Element<Self> {
			let origin = Vec3::new(0.0, 0.0, -0.5);
			let length = 1.0;

			// the beam hangs off an identity spatial under the client root, so root space is beam space
			let locate = {
				let client = context.stardust_client.clone();
				let tasks = tasks.clone();
				move |id: QueryableId, spatial: SpatialRef| {
					let client = client.clone();
					tasks.spawn(
						async move {
							client
								.spatial_interface()
								.get_relative_transform(client.root().clone(), spatial)
								.await
						},
						move |_, s: &mut Self, t| {
							if let (Ok(Ok(t)), true) = (t, s.derezzables.0.contains_key(&id)) {
								s.positions.insert(id, t.translation.into());
							}
						},
					);
				}
			};
			let locate_moved = locate.clone();

			let target = Shape::Box {
				size: [0.05; 3].into(),
			};
			let target_at =
				|i: usize| {
					Entity::new(target.clone())
						.pose(self.targets[i])
						.component(Grabbable::new(move |s: &mut Self, pose| {
							s.targets[i] = pose
						}))
						.component(Derezzable::new(|_| {}))
						.build()
						.child(
							Lines::new(shape(target.clone()).into_iter().map(|l| {
								l.color(rgba_linear!(1.0, 0.1, 0.1, 1.0)).thickness(0.005)
							}))
							.build(),
						)
				};
			Spatial::default()
				.build()
				.child(
					BeamQuery::new(
						origin,
						Vec3::Y,
						move |s: &mut Self, id, field, spatial, interfaces, ray| {
							locate(id, spatial.clone());
							s.derezzables.0.insert(
								id,
								IntersectedQueryable {
									spatial,
									field,
									interfaces,
									ray,
								},
							);
						},
						|s: &mut Self, id, interfaces| {
							if let Some(q) = s.derezzables.0.get_mut(&id) {
								q.interfaces = interfaces;
							}
						},
						move |s: &mut Self, id, ray| {
							if let Some(q) = s.derezzables.0.get_mut(&id) {
								q.ray = ray;
								locate_moved(id, q.spatial.clone());
							}
						},
						|s: &mut Self, id| {
							s.derezzables.0.remove(&id);
							s.positions.remove(&id);
						},
					)
					.max_length(length)
					.margin(0.2)
					.build(),
				)
				.child(
					Lines::new(
						std::iter::once(
							line_from_points(vec![origin, origin + Vec3::Y * length])
								.color(rgba_linear!(1.0, 0.1, 0.1, 1.0))
								.thickness(0.005),
						)
						.chain(self.derezzables.0.iter().filter_map(|(id, q)| {
							Some(
								line_from_points(vec![
									origin + Vec3::Y * q.ray.deepest_point_distance,
									*self.positions.get(id)?,
								])
								.color(rgba_linear!(0.1, 1.0, 0.1, 1.0))
								.thickness(0.003),
							)
						})),
					)
					.build(),
				)
				.child(target_at(0))
				.child(target_at(1))
				.child(target_at(2))
		}
	}
	client::run::<TestState>(&[]).await.unwrap()
}
