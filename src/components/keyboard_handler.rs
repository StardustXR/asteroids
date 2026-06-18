use std::sync::Arc;

use crate::{
	CloneFnWrapper, Component, ComponentCreateInfo, Context, ValidState, custom::FnWrapper,
};
use gluon::{Handler, Object};
use stardust_xr_fusion::{Error, query::QueryableInterfaceGuard, types::Timestamp};
use stardust_xr_molecules::keyboard_handler::protocol::{
	EXTERNAL_PROTOCOL, KeyEvent, KeyboardHandlerHandler,
};
use tokio::sync::{RwLock, mpsc};

#[derive_where::derive_where(Debug, PartialEq, Default)]
pub struct KeyboardHandler<State: ValidState> {
	#[allow(clippy::type_complexity)]
	on_key: Option<FnWrapper<dyn Fn(&mut State, KeyEvent, Option<Timestamp>) + Send + Sync>>,
	on_key_async: Option<OnKeyAsync>,
}
type OnKeyAsync = CloneFnWrapper<dyn Fn(KeyEvent, Option<Timestamp>) + Send + Sync>;

impl<State: ValidState> KeyboardHandler<State> {
	pub fn new() -> KeyboardHandler<State> {
		KeyboardHandler::default()
	}
	pub fn on_key(
		mut self,
		on_key: impl Fn(&mut State, KeyEvent, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.on_key = Some(FnWrapper(Box::new(on_key)));
		self
	}
	pub fn on_key_async(
		mut self,
		on_key_async: impl Fn(KeyEvent, Option<Timestamp>) + Send + Sync + 'static,
	) -> Self {
		self.on_key_async = Some(CloneFnWrapper(Arc::new(on_key_async)));
		self
	}
}
#[derive(Debug, Handler)]
struct KbHandler {
	key_tx: mpsc::UnboundedSender<(KeyEvent, Option<Timestamp>)>,
	on_key_asnyc: Arc<RwLock<Option<OnKeyAsync>>>,
}
impl KeyboardHandlerHandler for KbHandler {
	async fn key(
		&self,
		_ctx: gluon::Context,
		event: stardust_xr_molecules::keyboard_handler::protocol::KeyEvent,
		timestamp: Option<stardust_xr_fusion::types::Timestamp>,
	) {
		if let Some(on_key) = self.on_key_asnyc.read().await.as_ref() {
			(on_key.0)(event.clone(), timestamp);
		}
		_ = self.key_tx.send((event, timestamp));
	}
}
#[derive(Debug)]
pub struct KeyboardHandlerInner {
	key_rx: mpsc::UnboundedReceiver<(KeyEvent, Option<Timestamp>)>,
	kb_handler: Object<KbHandler>,
	// the entity owns the shared queryable; we just hold our interface guard on it
	_queryable_interface_guard: QueryableInterfaceGuard,
}

impl<State: ValidState> Component<State> for KeyboardHandler<State> {
	type Inner = KeyboardHandlerInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let (key_tx, key_rx) = mpsc::unbounded_channel();
		let kb_handler = context
			.stardust_client
			.pion_device()
			.register_object(KbHandler {
				key_tx,
				on_key_asnyc: Arc::new(RwLock::new(self.on_key_async.clone())),
			});
		let queryable_interface_guard = info
			.queryable
			.add_interface(&kb_handler, EXTERNAL_PROTOCOL.protocol_name)
			.await?;

		Ok(KeyboardHandlerInner {
			key_rx,
			kb_handler,
			_queryable_interface_guard: queryable_interface_guard,
		})
	}

	fn diff(&self, _old: &Self, _context: &Context, inner: &mut Self::Inner) {
		let on_key_async = self.on_key_async.clone();
		let rwlock = inner.kb_handler.on_key_asnyc.clone();
		// maybe theres a better way to do this?
		tokio::spawn(async move {
			*rwlock.write().await = on_key_async;
		});
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		while let Ok((key_event, timestamp)) = inner.key_rx.try_recv() {
			if let Some(on_key) = self.on_key.as_ref() {
				(on_key.0)(state, key_event, timestamp);
			}
		}
	}
}
#[tokio::test]
async fn asteroids_keyboard_element() {
	use crate::{
		Context, Entity, Tasker,
		client::{self, ClientState},
		components::KeyboardHandler,
		custom::CustomElement,
		elements::Text,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::fields::Shape;

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		latest_key: Option<KeyEvent>,
	}
	impl TestState {
		pub fn key_press(&mut self, key_info: KeyEvent, _ts: Option<Timestamp>) {
			if key_info.pressed {}
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.keyboard";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Entity::new(Shape::Sphere { radius: 0.5 })
				.component(KeyboardHandler::new().on_key(Self::key_press))
				.build()
				.child(
					Text::new(
						self.latest_key
							.as_ref()
							.map(|key| format!("Latest key: {:?}", key.keycode))
							.unwrap_or_default(),
					)
					.character_height(0.05)
					.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await.unwrap();
}
