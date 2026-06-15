use std::sync::Arc;

use crate::{
	CloneFnWrapper, Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable},
};
use derive_setters::Setters;
use gluon::{Handler, Object};
use stardust_xr_fusion::{
	Error,
	fields::{Field, FieldExt, Shape},
	query::{QueryableInterfaceGuard, QueryableObject},
	spatial::{Spatial, Transform},
	types::Timestamp,
};
use stardust_xr_molecules::keyboard_handler::protocol::{
	EXTERNAL_PROTOCOL, KeyEvent, KeyboardHandlerHandler,
};
use tokio::sync::{RwLock, mpsc};

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct KeyboardHandler<State: ValidState> {
	transform: Transform,
	field_shape: stardust_xr_fusion::fields::Shape,
	#[setters(ignore)]
	#[allow(clippy::type_complexity)]
	on_key: Option<FnWrapper<dyn Fn(&mut State, KeyEvent, Option<Timestamp>) + Send + Sync>>,
	#[setters(ignore)]
	on_key_async: Option<OnKeyAsync>,
}
type OnKeyAsync = CloneFnWrapper<dyn Fn(KeyEvent, Option<Timestamp>) + Send + Sync>;

impl<State: ValidState> Default for KeyboardHandler<State> {
	fn default() -> Self {
		KeyboardHandler {
			transform: Transform::IDENTITY,
			field_shape: stardust_xr_fusion::fields::Shape::Sphere { radius: 1.0 },
			on_key: None,
			on_key_async: None,
		}
	}
}
impl<State: ValidState> KeyboardHandler<State> {
	pub fn new(field_shape: Shape) -> KeyboardHandler<State> {
		KeyboardHandler {
			transform: Transform::IDENTITY,
			field_shape,
			on_key: None,
			on_key_async: None,
		}
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
	spatial: Spatial,
	field: Field,
	key_rx: mpsc::UnboundedReceiver<(KeyEvent, Option<Timestamp>)>,
	kb_handler: Object<KbHandler>,
	_queryable: QueryableObject,
	_queryable_interface_guard: QueryableInterfaceGuard,
}

impl<State: ValidState> CustomElement<State> for KeyboardHandler<State> {
	type Inner = KeyboardHandlerInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (field, _) = Field::new(
			&context.stardust_client,
			&info.child_space,
			self.field_shape.clone(),
		)
		.await?;
		let (key_tx, key_rx) = mpsc::unbounded_channel();
		let kb_handler = context
			.stardust_client
			.pion_device()
			.register_object(KbHandler {
				key_tx,
				on_key_asnyc: Arc::new(RwLock::new(self.on_key_async.clone())),
			});
		let queryable = context
			.stardust_client
			.query_interface()
			.register_queryable(info.child_space.clone(), field.clone())
			.await??;
		let queryable_interface_guard = queryable
			.add_interface(&kb_handler, EXTERNAL_PROTOCOL.protocol_name)
			.await?;

		Ok(KeyboardHandlerInner {
			spatial: info.child_space,
			field,
			key_rx,
			kb_handler,
			_queryable_interface_guard: queryable_interface_guard,
			_queryable: queryable,
		})
	}

	fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old, &inner.spatial);

		if self.field_shape != old.field_shape {
			let _ = inner.field.set_shape(self.field_shape.clone());
		}

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
impl<State: ValidState> Transformable for KeyboardHandler<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
#[tokio::test]
async fn asteroids_keyboard_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::{KeyboardHandler, Text},
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
			KeyboardHandler::new(Shape::Sphere { radius: 0.5 })
				.on_key(Self::key_press)
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
