use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable},
};
use derive_setters::Setters;
use stardust_xr_fusion::{
	Error,
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, Transform},
};
use stardust_xr_molecules::keyboard::{Keyboard, KeypressInfo};
use tokio::sync::mpsc;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct KeyboardHandler<State: ValidState> {
	transform: Transform,
	field_shape: stardust_xr_fusion::fields::Shape,
	#[allow(clippy::type_complexity)]
	on_key: FnWrapper<dyn Fn(&mut State, KeypressInfo) + Send + Sync>,
}

impl<State: ValidState> Default for KeyboardHandler<State> {
	fn default() -> Self {
		KeyboardHandler {
			transform: Transform::IDENTITY,
			field_shape: stardust_xr_fusion::fields::Shape::Sphere { radius: 1.0 },
			on_key: FnWrapper(Box::new(|_, _| {})),
		}
	}
}
impl<State: ValidState> KeyboardHandler<State> {
	pub fn new(
		field_shape: Shape,
		on_key: impl Fn(&mut State, KeypressInfo) + Send + Sync + 'static,
	) -> KeyboardHandler<State> {
		KeyboardHandler {
			transform: Transform::IDENTITY,
			field_shape,
			on_key: FnWrapper(Box::new(on_key)),
		}
	}
}
pub struct KeyboardElementInner {
	spatial: Spatial,
	field: Field,
	key_rx: mpsc::UnboundedReceiver<KeypressInfo>,
	_keyboard: Keyboard,
}
impl<State: ValidState> CustomElement<State> for KeyboardHandler<State> {
	type Inner = KeyboardElementInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (field, _) = Field::create(
			&context.stardust_client,
			&info.child_space,
			self.field_shape.clone(),
		)
		.await?;
		let (key_tx, key_rx) = mpsc::unbounded_channel();
		let _keyboard = Keyboard::new(
			&context.stardust_client,
			info.child_space.clone(),
			field.clone(),
			move |key_info| {
				let _ = key_tx.send(key_info);
			},
		)
		.await?;
		Ok(KeyboardElementInner {
			spatial: info.child_space,
			field,
			_keyboard,
			key_rx,
		})
	}

	fn diff(&self, old: &Self, context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old, &inner.spatial);

		if self.field_shape != old.field_shape {
			let _ = inner.field.set_shape(self.field_shape.clone());
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		while let Ok(key_info) = inner.key_rx.try_recv() {
			(self.on_key.0)(state, key_info);
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
	use stardust_xr_molecules::keyboard::KeypressInfo;

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		latest_key: Option<KeypressInfo>,
	}
	impl TestState {
		pub fn key_press(&mut self, key_info: KeypressInfo) {
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
			KeyboardHandler::new(Shape::Sphere(0.5), Self::key_press)
				.build()
				.child(
					Text::new(
						self.latest_key
							.as_ref()
							.map(|key| format!("Latest key: {:?}", key.key))
							.unwrap_or_default(),
					)
					.character_height(0.05)
					.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await.unwrap();
}
