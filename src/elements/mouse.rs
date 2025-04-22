use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{ElementTrait, FnWrapper, Transformable},
};
use derive_setters::Setters;
use derive_where::derive_where;
use mint::Vector2;
use stardust_xr_fusion::{
	fields::{Field, FieldAspect, Shape},
	node::NodeError,
	spatial::{SpatialRef, Transform},
};
use stardust_xr_molecules::{
	dbus::DbusObjectHandles, mouse::MouseHandler as MoleculesMouseHandler,
};
use tokio::sync::mpsc;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct MouseHandler<State: ValidState> {
	transform: Transform,
	field_shape: stardust_xr_fusion::fields::Shape,
	#[setters(skip)]
	#[allow(clippy::type_complexity)]
	on_button: FnWrapper<dyn Fn(&mut State, u32, bool) + Send + Sync + 'static>,
	#[setters(skip)]
	#[allow(clippy::type_complexity)]
	on_motion: FnWrapper<dyn Fn(&mut State, Vector2<f32>) + Send + Sync + 'static>,
	#[setters(skip)]
	#[allow(clippy::type_complexity)]
	on_scroll_discrete: FnWrapper<dyn Fn(&mut State, Vector2<f32>) + Send + Sync + 'static>,
	#[setters(skip)]
	#[allow(clippy::type_complexity)]
	on_scroll_continuous: FnWrapper<dyn Fn(&mut State, Vector2<f32>) + Send + Sync + 'static>,
}

impl<State: ValidState> MouseHandler<State> {
	pub fn new(
		field_shape: Shape,
		on_button: impl Fn(&mut State, u32, bool) + Send + Sync + 'static,
		on_motion: impl Fn(&mut State, Vector2<f32>) + Send + Sync + 'static,
		on_scroll_discrete: impl Fn(&mut State, Vector2<f32>) + Send + Sync + 'static,
		on_scroll_continuous: impl Fn(&mut State, Vector2<f32>) + Send + Sync + 'static,
	) -> MouseHandler<State> {
		MouseHandler {
			transform: Transform::none(),
			field_shape,
			on_button: FnWrapper(Box::new(on_button)),
			on_motion: FnWrapper(Box::new(on_motion)),
			on_scroll_discrete: FnWrapper(Box::new(on_scroll_discrete)),
			on_scroll_continuous: FnWrapper(Box::new(on_scroll_continuous)),
		}
	}
}
pub struct MouseElementInner {
	field: Field,
	_dbus_object_handles: DbusObjectHandles,
	button_rx: mpsc::UnboundedReceiver<(u32, bool)>,
	motion_rx: mpsc::UnboundedReceiver<Vector2<f32>>,
	scroll_discrete_rx: mpsc::UnboundedReceiver<Vector2<f32>>,
	scroll_continuous_rx: mpsc::UnboundedReceiver<Vector2<f32>>,
}
impl<State: ValidState> ElementTrait<State> for MouseHandler<State> {
	type Inner = MouseElementInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let field = Field::create(info.parent_space, self.transform, self.field_shape.clone())?;
		let (button_tx, button_rx) = mpsc::unbounded_channel();
		let (motion_tx, motion_rx) = mpsc::unbounded_channel();
		let (scroll_discrete_tx, scroll_discrete_rx) = mpsc::unbounded_channel();
		let (scroll_continuous_tx, scroll_continuous_rx) = mpsc::unbounded_channel();
		let _dbus_object_handles = MoleculesMouseHandler::create(
			context.dbus_connection.clone(),
			info.element_path,
			None,
			&field,
			move |button, pressed| {
				let _ = button_tx.send((button, pressed));
			},
			move |motion| {
				let _ = motion_tx.send(motion);
			},
			move |scroll| {
				let _ = scroll_discrete_tx.send(scroll);
			},
			move |scroll| {
				let _ = scroll_continuous_tx.send(scroll);
			},
		);
		Ok(MouseElementInner {
			field,
			_dbus_object_handles,
			button_rx,
			motion_rx,
			scroll_discrete_rx,
			scroll_continuous_rx,
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

		while let Ok((button, pressed)) = inner.button_rx.try_recv() {
			(self.on_button.0)(state, button, pressed);
		}
		while let Ok(key_info) = inner.motion_rx.try_recv() {
			(self.on_motion.0)(state, key_info);
		}
		while let Ok(key_info) = inner.scroll_discrete_rx.try_recv() {
			(self.on_scroll_discrete.0)(state, key_info);
		}
		while let Ok(key_info) = inner.scroll_continuous_rx.try_recv() {
			(self.on_scroll_continuous.0)(state, key_info);
		}
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.field.clone().as_spatial().as_spatial_ref()
	}
}
impl<State: ValidState> Transformable for MouseHandler<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
#[tokio::test]
async fn asteroids_mouse_element() {
	use crate::{
		Element,
		client::{self, ClientState},
		custom::ElementTrait,
		elements::{MouseHandler, Spatial, Text},
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
		pub fn handle_button(&mut self, button: u32, pressed: bool) {
			self.latest_button = Some((button, pressed));
		}

		pub fn handle_motion(&mut self, motion: Vector2<f32>) {
			self.latest_motion = Some(motion);
		}

		pub fn handle_scroll_discrete(&mut self, scroll: Vector2<f32>) {
			self.latest_scroll_discrete = Some(scroll);
		}

		pub fn handle_scroll_continuous(&mut self, scroll: Vector2<f32>) {
			self.latest_scroll_continuous = Some(scroll);
		}
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const QUALIFIER: &'static str = "org";
		const ORGANIZATION: &'static str = "asteroids";
		const NAME: &'static str = "mouse";

		fn reify(&self) -> Element<Self> {
			// Create a container spatial
			Spatial::default().with_children([
				Text::default()
					.text(format!(
						"Latest button: {:?}\nLatest motion: {:?}\nLatest discrete scroll: {:?}\nLatest continuous scroll: {:?}",
						self.latest_button,
						self.latest_motion,
						self.latest_scroll_discrete,
						self.latest_scroll_continuous
					))
					.character_height(0.05)
					.build(),
				MouseHandler::new(
					Shape::Sphere(0.5),
					Self::handle_button,
					Self::handle_motion,
					Self::handle_scroll_discrete,
					Self::handle_scroll_continuous,
				)
				.build(),
			])
		}
	}
	client::run::<TestState>(&[]).await
}
