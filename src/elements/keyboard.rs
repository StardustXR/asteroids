use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{ElementTrait, FnWrapper, Transformable},
};
use derive_setters::Setters;
use derive_where::derive_where;
use stardust_xr_fusion::{
	fields::{Field, FieldAspect, Shape},
	node::NodeError,
	spatial::{SpatialRef, Transform},
};
use stardust_xr_molecules::{
	dbus::DbusObjectHandles,
	keyboard::{KeyboardHandler as MoleculesKeyboardHandler, KeypressInfo},
};
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
			transform: Transform::none(),
			field_shape: stardust_xr_fusion::fields::Shape::Sphere(1.0),
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
			transform: Transform::none(),
			field_shape,
			on_key: FnWrapper(Box::new(on_key)),
		}
	}
}
pub struct KeyboardElementInner {
	field: Field,
	_dbus_object_handles: DbusObjectHandles,
	key_rx: mpsc::UnboundedReceiver<KeypressInfo>,
}
impl<State: ValidState> ElementTrait<State> for KeyboardHandler<State> {
	type Inner = KeyboardElementInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let field = Field::create(info.parent_space, self.transform, self.field_shape.clone())?;
		let (key_tx, key_rx) = mpsc::unbounded_channel();
		let _dbus_object_handles = MoleculesKeyboardHandler::create(
			context.dbus_connection.clone(),
			info.element_path,
			None,
			&field,
			move |key_info| {
				let _ = key_tx.send(key_info);
			},
		);
		Ok(KeyboardElementInner {
			field,
			_dbus_object_handles,
			key_rx,
		})
	}

	fn update(
		&self,
		old: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old, &inner.field);

		if self.field_shape != old.field_shape {
			let _ = inner.field.set_shape(self.field_shape.clone());
		}

		while let Ok(key_info) = inner.key_rx.try_recv() {
			(self.on_key.0)(state, key_info);
		}
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.field.clone().as_spatial().as_spatial_ref()
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
		Element,
		client::{self, ClientState},
		custom::ElementTrait,
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

		fn reify(&self) -> Element<Self> {
			KeyboardHandler::new(Shape::Sphere(0.5), Self::key_press)
				.build()
				.child(
					Text::default()
						.text(
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
	client::run::<TestState>(&[]).await
}
