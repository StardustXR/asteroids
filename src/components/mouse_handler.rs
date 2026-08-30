use crate::{CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState};
use gluon::{Handler, Interface, Node, RefExt};
use mint::Vector2;
use stardust_xr_fusion::{
	Error,
	query::QueryableInterface,
	types::{Timestamp, proxies::Vec2F},
};
use stardust_xr_molecules::mouse_handler::{
	ScrollSource,
	protocol::{MouseHandler as MouseHandlerProxy, MouseHandlerHandler},
};
use std::sync::Arc;
use tokio::sync::{RwLock, mpsc};

#[derive_where::derive_where(Debug, PartialEq, Clone)]
pub struct MouseHandler<State: ValidState> {
	#[allow(clippy::type_complexity)]
	on_button: Option<
		CloneFnWrapper<dyn Fn(&mut State, u32, bool, Option<Timestamp>) + Send + Sync + 'static>,
	>,
	#[allow(clippy::type_complexity)]
	on_motion: Option<
		CloneFnWrapper<dyn Fn(&mut State, Vector2<f32>, Option<Timestamp>) + Send + Sync + 'static>,
	>,
	#[allow(clippy::type_complexity)]
	on_scroll_discrete: Option<
		CloneFnWrapper<
			dyn Fn(&mut State, Vector2<f32>, ScrollSource, Option<Timestamp>)
				+ Send
				+ Sync
				+ 'static,
		>,
	>,
	#[allow(clippy::type_complexity)]
	on_scroll_continuous: Option<
		CloneFnWrapper<
			dyn Fn(&mut State, Vector2<f32>, ScrollSource, Option<Timestamp>)
				+ Send
				+ Sync
				+ 'static,
		>,
	>,
	async_callbacks: AsyncMouseCallbacks,
}

#[derive(Clone, Debug, PartialEq)]
struct AsyncMouseCallbacks {
	#[allow(clippy::type_complexity)]
	on_button: Option<CloneFnWrapper<dyn Fn(u32, bool, Option<Timestamp>) + Send + Sync + 'static>>,
	#[allow(clippy::type_complexity)]
	on_motion:
		Option<CloneFnWrapper<dyn Fn(Vector2<f32>, Option<Timestamp>) + Send + Sync + 'static>>,
	#[allow(clippy::type_complexity)]
	on_scroll_discrete: Option<
		CloneFnWrapper<
			dyn Fn(Vector2<f32>, ScrollSource, Option<Timestamp>) + Send + Sync + 'static,
		>,
	>,
	#[allow(clippy::type_complexity)]
	on_scroll_continuous: Option<
		CloneFnWrapper<
			dyn Fn(Vector2<f32>, ScrollSource, Option<Timestamp>) + Send + Sync + 'static,
		>,
	>,
}

impl<State: ValidState> MouseHandler<State> {
	pub fn new() -> MouseHandler<State> {
		MouseHandler {
			on_button: None,
			on_motion: None,
			on_scroll_discrete: None,
			on_scroll_continuous: None,
			async_callbacks: AsyncMouseCallbacks {
				on_button: None,
				on_motion: None,
				on_scroll_discrete: None,
				on_scroll_continuous: None,
			},
		}
	}
	pub fn on_button(
		mut self,
		on_button: impl Fn(&mut State, u32, bool, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.on_button = Some(CloneFnWrapper(Arc::new(on_button)));
		self
	}
	pub fn on_motion(
		mut self,
		on_motion: impl Fn(&mut State, Vector2<f32>, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.on_motion = Some(CloneFnWrapper(Arc::new(on_motion)));
		self
	}
	pub fn on_scroll_discrete(
		mut self,
		on_scroll_discrete: impl Fn(&mut State, Vector2<f32>, ScrollSource, Option<Timestamp>)
		+ Send
		+ Sync
		+ 'static,
	) -> Self {
		self.on_scroll_discrete = Some(CloneFnWrapper(Arc::new(on_scroll_discrete)));
		self
	}
	pub fn on_scroll_continuous(
		mut self,
		on_scroll_continuous: impl Fn(&mut State, Vector2<f32>, ScrollSource, Option<Timestamp>)
		+ Send
		+ Sync
		+ 'static,
	) -> Self {
		self.on_scroll_continuous = Some(CloneFnWrapper(Arc::new(on_scroll_continuous)));
		self
	}

	pub fn on_button_async(
		mut self,
		on_button: impl Fn(u32, bool, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.async_callbacks.on_button = Some(CloneFnWrapper(Arc::new(on_button)));
		self
	}
	pub fn on_motion_async(
		mut self,
		on_motion: impl Fn(Vector2<f32>, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.async_callbacks.on_motion = Some(CloneFnWrapper(Arc::new(on_motion)));
		self
	}
	pub fn on_scroll_discrete_async(
		mut self,
		on_scroll_discrete: impl Fn(Vector2<f32>, ScrollSource, Option<Timestamp>)
		+ Send
		+ Sync
		+ 'static,
	) -> Self {
		self.async_callbacks.on_scroll_discrete =
			Some(CloneFnWrapper(Arc::new(on_scroll_discrete)));
		self
	}
	pub fn on_scroll_continuous_async(
		mut self,
		on_scroll_continuous: impl Fn(Vector2<f32>, ScrollSource, Option<Timestamp>)
		+ Send
		+ Sync
		+ 'static,
	) -> Self {
		self.async_callbacks.on_scroll_continuous =
			Some(CloneFnWrapper(Arc::new(on_scroll_continuous)));
		self
	}
}
impl<State: ValidState> Default for MouseHandler<State> {
	fn default() -> Self {
		Self::new()
	}
}
#[derive(Debug, Handler)]
struct MouseHandlerQueryable {
	button_tx: mpsc::UnboundedSender<(u32, bool, Option<Timestamp>)>,
	motion_tx: mpsc::UnboundedSender<(Vector2<f32>, Option<Timestamp>)>,
	scroll_discrete_tx: mpsc::UnboundedSender<(Vector2<f32>, ScrollSource, Option<Timestamp>)>,
	scroll_continuous_tx: mpsc::UnboundedSender<(Vector2<f32>, ScrollSource, Option<Timestamp>)>,
	callbacks: Arc<RwLock<AsyncMouseCallbacks>>,
}
impl MouseHandlerHandler for MouseHandlerQueryable {
	async fn motion(&self, _ctx: gluon::Context, delta: Vec2F, timestamp: Option<Timestamp>) {
		let callbacks = self.callbacks.read().await;
		if let Some(on_motion) = callbacks.on_motion.as_ref() {
			(on_motion.0)(delta, timestamp);
		}
		_ = self.motion_tx.send((delta, timestamp));
	}

	async fn button(
		&self,
		_ctx: gluon::Context,
		button: u32,
		pressed: bool,
		timestamp: Option<Timestamp>,
	) {
		let callbacks = self.callbacks.read().await;
		if let Some(on_button) = callbacks.on_button.as_ref() {
			(on_button.0)(button, pressed, timestamp);
		}
		_ = self.button_tx.send((button, pressed, timestamp));
	}

	async fn scroll_smooth(
		&self,
		_ctx: gluon::Context,
		delta: Vec2F,
		source: ScrollSource,
		timestamp: Option<Timestamp>,
	) {
		let callbacks = self.callbacks.read().await;
		if let Some(on_scroll) = callbacks.on_scroll_continuous.as_ref() {
			(on_scroll.0)(delta, source, timestamp);
		}
		_ = self.scroll_continuous_tx.send((delta, source, timestamp));
	}

	async fn scroll_discrete(
		&self,
		_ctx: gluon::Context,
		delta: Vec2F,
		source: ScrollSource,
		timestamp: Option<Timestamp>,
	) {
		let callbacks = self.callbacks.read().await;
		if let Some(on_scroll) = callbacks.on_scroll_discrete.as_ref() {
			(on_scroll.0)(delta, source, timestamp);
		}
		_ = self.scroll_discrete_tx.send((delta, source, timestamp));
	}
}
pub struct MouseElementInner {
	button_rx: mpsc::UnboundedReceiver<(u32, bool, Option<Timestamp>)>,
	motion_rx: mpsc::UnboundedReceiver<(Vector2<f32>, Option<Timestamp>)>,
	scroll_discrete_rx: mpsc::UnboundedReceiver<(Vector2<f32>, ScrollSource, Option<Timestamp>)>,
	scroll_continuous_rx: mpsc::UnboundedReceiver<(Vector2<f32>, ScrollSource, Option<Timestamp>)>,
	mouse_handler: Node<MouseHandlerQueryable>,
	// the entity owns the shared queryable; we just hold our interface guard on it
	_queryable_interface: QueryableInterface,
}
impl<State: ValidState> Component<State> for MouseHandler<State> {
	type Inner = MouseElementInner;

	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let (button_tx, button_rx) = mpsc::unbounded_channel();
		let (motion_tx, motion_rx) = mpsc::unbounded_channel();
		let (scroll_discrete_tx, scroll_discrete_rx) = mpsc::unbounded_channel();
		let (scroll_continuous_tx, scroll_continuous_rx) = mpsc::unbounded_channel();
		let (mouse_handler, mouse_ref) = MouseHandlerProxy::new_node(MouseHandlerQueryable {
			button_tx,
			motion_tx,
			scroll_discrete_tx,
			scroll_continuous_tx,
			callbacks: Arc::new(RwLock::new(self.async_callbacks.clone())),
		})?;
		let queryable_interface = info
			.queryable
			.add_interface(&mouse_ref, MouseHandlerProxy::ID)
			.await??;
		Ok(MouseElementInner {
			button_rx,
			motion_rx,
			scroll_discrete_rx,
			scroll_continuous_rx,
			mouse_handler,
			_queryable_interface: queryable_interface,
		})
	}

	fn diff(
		&self,
		_old: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		let callbacks = self.async_callbacks.clone();
		let rwlock = inner.mouse_handler.callbacks.clone();
		// maybe theres a better way to do this?
		tokio::spawn(async move {
			*rwlock.write().await = callbacks;
		});
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		while let Ok((button, pressed, timestamp)) = inner.button_rx.try_recv() {
			if let Some(on_button) = self.on_button.as_ref() {
				(on_button.0)(state, button, pressed, timestamp);
			}
		}
		while let Ok((delta, timestamp)) = inner.motion_rx.try_recv() {
			if let Some(on_motion) = self.on_motion.as_ref() {
				(on_motion.0)(state, delta, timestamp);
			}
		}
		while let Ok((delta, source, timestamp)) = inner.scroll_discrete_rx.try_recv() {
			if let Some(on_scroll_discrete) = self.on_scroll_discrete.as_ref() {
				(on_scroll_discrete.0)(state, delta, source, timestamp);
			}
		}
		while let Ok((delta, source, timestamp)) = inner.scroll_continuous_rx.try_recv() {
			if let Some(on_scroll_continuous) = self.on_scroll_continuous.as_ref() {
				(on_scroll_continuous.0)(state, delta, source, timestamp);
			}
		}
	}
}
#[tokio::test]
async fn asteroids_mouse_element() {
	use crate::{
		Context, Entity, Tasker,
		client::{self, ClientState},
		components::MouseHandler,
		custom::CustomElement,
		elements::Text,
	};
	use mint::Vector2;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::fields::Shape;

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		latest_button: Option<(u32, bool)>,
		#[serde(skip)]
		latest_motion: Option<Vector2<f32>>,
		#[serde(skip)]
		latest_scroll_discrete: Option<Vector2<f32>>,
		#[serde(skip)]
		latest_scroll_continuous: Option<Vector2<f32>>,
	}
	impl TestState {
		pub fn handle_button(&mut self, button: u32, pressed: bool, _ts: Option<Timestamp>) {
			self.latest_button = Some((button, pressed));
		}

		pub fn handle_motion(&mut self, motion: Vector2<f32>, _ts: Option<Timestamp>) {
			self.latest_motion = Some(motion);
		}

		pub fn handle_scroll_discrete(
			&mut self,
			scroll: Vector2<f32>,
			_source: ScrollSource,
			_ts: Option<Timestamp>,
		) {
			self.latest_scroll_discrete = Some(scroll);
		}

		pub fn handle_scroll_continuous(
			&mut self,
			scroll: Vector2<f32>,
			_source: ScrollSource,
			_ts: Option<Timestamp>,
		) {
			self.latest_scroll_continuous = Some(scroll);
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.mouse";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Entity::new(Shape::Sphere { radius: 0.5 })
				.component(
					MouseHandler::new()
						.on_button(Self::handle_button)
						.on_motion(Self::handle_motion)
						.on_scroll_discrete(Self::handle_scroll_discrete)
						.on_scroll_continuous(Self::handle_scroll_continuous),
				)
				.build().child(Text::new(
					format!(
						"Latest button: {:?}\nLatest motion: {:?}\nLatest discrete scroll: {:?}\nLatest continuous scroll: {:?}",
						self.latest_button,
						self.latest_motion,
						self.latest_scroll_discrete,
						self.latest_scroll_continuous
					))
					.character_height(0.05)
					.build())
		}
	}
	client::run::<TestState>(&[]).await.unwrap();
}
