use crate::{
	Component, ComponentCreateInfo, Context, FnWrapper, Inners, ValidState,
	elements::{QueryEvent, QueryableInterfaces},
};
use gluon_ipc::{Handler, Node, RefExt};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{
	Error,
	client::FrameInfo,
	fields::{FieldRef, FieldSample},
	query::{QueriedInterface, QueryableId},
	spatial::SpatialRef,
	spatial_query::{self, ZoneQueryHandle, ZoneQueryHandler, ZoneQueryHandlerHandler},
	types::Vec3F,
};
use tokio::sync::mpsc;

pub type OnEnteredZone<State, I> = FnWrapper<
	dyn Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, Vec3F, FieldSample) + Send + Sync,
>;
pub type OnInterfacesChangedZone<State, I> =
	FnWrapper<dyn Fn(&mut State, QueryableId, I) + Send + Sync>;
pub type OnMovedZone<State> =
	FnWrapper<dyn Fn(&mut State, QueryableId, Vec3F, FieldSample) + Send + Sync>;
pub type OnLeftZone<State> = FnWrapper<dyn Fn(&mut State, QueryableId) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct ZonedQueryable<I: QueryableInterfaces> {
	pub spatial: SpatialRef,
	pub field: FieldRef,
	pub interfaces: I,
	pub relative_position: Vec3F,
	pub sample: FieldSample,
}

#[derive(Debug, Clone)]
#[derive_where::derive_where(Default)]
pub struct ZoneQueryCache<I: QueryableInterfaces>(pub FxHashMap<QueryableId, ZonedQueryable<I>>);

/// finds everything with the interfaces I inside the entity's field
pub struct ZoneQuery<State: ValidState, I: QueryableInterfaces> {
	margin: f32,
	on_entered: OnEnteredZone<State, I>,
	on_interfaces_changed: OnInterfacesChangedZone<State, I>,
	on_moved: OnMovedZone<State>,
	on_left: OnLeftZone<State>,
}
impl<State: ValidState, I: QueryableInterfaces> ZoneQuery<State, I> {
	pub fn new(
		on_entered: impl Fn(&mut State, QueryableId, FieldRef, SpatialRef, I, Vec3F, FieldSample)
		+ Send
		+ Sync
		+ 'static,
		on_interfaces_changed: impl Fn(&mut State, QueryableId, I) + Send + Sync + 'static,
		on_moved: impl Fn(&mut State, QueryableId, Vec3F, FieldSample) + Send + Sync + 'static,
		on_left: impl Fn(&mut State, QueryableId) + Send + Sync + 'static,
	) -> Self {
		Self {
			margin: 0.0,
			on_entered: FnWrapper(Box::new(on_entered)),
			on_interfaces_changed: FnWrapper(Box::new(on_interfaces_changed)),
			on_moved: FnWrapper(Box::new(on_moved)),
			on_left: FnWrapper(Box::new(on_left)),
		}
	}
	pub fn new_cached(
		cache: impl Fn(&mut State) -> &mut ZoneQueryCache<I> + Clone + Send + Sync + 'static,
	) -> Self {
		let entered = cache.clone();
		let changed = cache.clone();
		let moved = cache.clone();
		Self::new(
			move |state, id, field, spatial, interfaces, relative_position, sample| {
				entered(state).0.insert(
					id,
					ZonedQueryable {
						spatial,
						field,
						interfaces,
						relative_position,
						sample,
					},
				);
			},
			move |state, id, interfaces| {
				if let Some(q) = changed(state).0.get_mut(&id) {
					q.interfaces = interfaces;
				}
			},
			move |state, id, relative_position, sample| {
				if let Some(q) = moved(state).0.get_mut(&id) {
					q.relative_position = relative_position;
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
}
impl<State: ValidState, I: QueryableInterfaces> std::fmt::Debug for ZoneQuery<State, I> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("ZoneQuery")
			.field("margin", &self.margin)
			.finish()
	}
}

pub struct ZoneQueryComponentInner {
	_node: Node<ZoneQueryInner>,
	handle: ZoneQueryHandle,
	events: mpsc::UnboundedReceiver<QueryEvent<(Vec3F, FieldSample)>>,
}
impl<State: ValidState, I: QueryableInterfaces> Component<State> for ZoneQuery<State, I> {
	type Inner = ZoneQueryComponentInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let (tx, events) = mpsc::unbounded_channel();
		let (node, handler) = ZoneQueryHandler::new_node(ZoneQueryInner { tx })?;
		let handle = context
			.stardust_client
			.spatial_query_interface()
			.zone_query(spatial_query::ZoneQuery {
				handler: handler.into(),
				interfaces: I::interface_dependencies(),
				zone_field: info.field.field_ref().await?,
				margin: self.margin,
			})
			.await??;
		Ok(ZoneQueryComponentInner {
			_node: node,
			handle,
			events,
		})
	}

	fn diff(
		&self,
		old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		if self.margin != old_self.margin {
			let _ = inners.self_inner().handle.update(self.margin);
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		while let Ok(event) = inner.events.try_recv() {
			match event {
				QueryEvent::Entered(id, field, spatial, interfaces, (position, sample)) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_entered.0)(
							state, id, field, spatial, interfaces, position, sample,
						);
					}
				}
				QueryEvent::InterfacesChanged(id, interfaces) => {
					if let Some(interfaces) = I::from_queried(&interfaces) {
						(self.on_interfaces_changed.0)(state, id, interfaces);
					}
				}
				QueryEvent::Moved(id, (position, sample)) => {
					(self.on_moved.0)(state, id, position, sample)
				}
				QueryEvent::Left(id) => (self.on_left.0)(state, id),
			}
		}
	}
}

#[derive(Handler)]
pub struct ZoneQueryInner {
	tx: mpsc::UnboundedSender<QueryEvent<(Vec3F, FieldSample)>>,
}
impl ZoneQueryHandlerHandler for ZoneQueryInner {
	async fn entered(
		&self,
		_ctx: gluon_ipc::Context,
		obj: QueryableId,
		field: FieldRef,
		spatial: SpatialRef,
		interfaces: Vec<QueriedInterface>,
		relative_position: Vec3F,
		spatial_info: FieldSample,
	) {
		let _ = self.tx.send(QueryEvent::Entered(
			obj,
			field,
			spatial,
			interfaces,
			(relative_position, spatial_info),
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
		relative_position: Vec3F,
		spatial_info: FieldSample,
	) {
		let _ = self
			.tx
			.send(QueryEvent::Moved(obj, (relative_position, spatial_info)));
	}

	async fn left(&self, _ctx: gluon_ipc::Context, obj: QueryableId) {
		let _ = self.tx.send(QueryEvent::Left(obj));
	}
}

#[tokio::test]
async fn asteroids_zone_query_component() {
	use crate::{
		Entity, Tasker, Transformable,
		client::{self, ClientState},
		components::{Derezzable, Grabbable, Lines},
		custom::CustomElement,
		elements::Spatial,
	};
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
		pose: Posef,
		targets: [Posef; 3],
		#[serde(skip)]
		derezzables: ZoneQueryCache<(DerezzableProxy,)>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				pose: Posef {
					position: [0.0, 0.0, -0.5].into(),
					..Default::default()
				},
				targets: [[0.3, 0.0, -0.5], [-0.3, 0.1, -0.6], [0.0, 0.3, -0.4]].map(|p| Posef {
					position: p.into(),
					..Default::default()
				}),
				derezzables: ZoneQueryCache::default(),
			}
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.zone_query";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
			_props: (),
		) -> impl crate::Element<Self> {
			let zone = Shape::Box {
				size: [0.4; 3].into(),
			};
			let target = Shape::Box {
				size: [0.05; 3].into(),
			};
			let target_at = |i: usize| {
				Entity::new(target.clone())
					.pose(self.targets[i])
					.component(Grabbable::new(move |s: &mut Self, pose| {
						s.targets[i] = pose
					}))
					.component(Derezzable::new(|_| {}))
					.component(Lines::new(shape(target.clone()).into_iter().map(|l| {
						l.color(rgba_linear!(1.0, 0.1, 0.1, 1.0)).thickness(0.005)
					})))
					.build()
			};
			Spatial::default()
				.build()
				.child(
					Entity::new(zone.clone())
						.pose(self.pose)
						.component(Grabbable::new(|s: &mut Self, pose| s.pose = pose))
						.component(ZoneQuery::new_cached(|s: &mut Self| &mut s.derezzables))
						.component(Lines::new(
							shape(zone).into_iter().map(|l| l.thickness(0.005)).chain(
								self.derezzables.0.values().map(|q| {
									line_from_points(vec![
										q.sample.closest_point,
										q.relative_position,
									])
									.color(rgba_linear!(0.1, 1.0, 0.1, 1.0))
									.thickness(0.005)
								}),
							),
						))
						.build(),
				)
				.child(target_at(0))
				.child(target_at(1))
				.child(target_at(2))
		}
	}
	client::run::<TestState>(&[]).await.unwrap()
}
