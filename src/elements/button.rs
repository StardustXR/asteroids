use crate::{
	custom::{ElementTrait, FnWrapper, Transformable},
	ValidState,
};
use derive_setters::Setters;
use derive_where::derive_where;
use mint::Vector2;
use stardust_xr_fusion::{
	core::values::Color,
	node::NodeError,
	spatial::{SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::{button::ButtonVisualSettings, DebugSettings, UIElement, VisualDebug};
use zbus::Connection;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Button<State: ValidState> {
	transform: Transform,
	on_press: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
	size: Vector2<f32>,
	max_hover_distance: f32,
	line_thickness: f32,
	accent_color: Color,
	debug: Option<DebugSettings>,
}
impl<State: ValidState> Default for Button<State> {
	fn default() -> Self {
		Button {
			transform: Transform::none(),
			on_press: FnWrapper(Box::new(|_| {})),
			size: [0.1; 2].into(),
			max_hover_distance: 0.025,
			line_thickness: 0.005,
			accent_color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
			debug: None,
		}
	}
}
impl<State: ValidState> Button<State> {
	pub fn new(on_press: impl Fn(&mut State) + Send + Sync + 'static) -> Button<State> {
		Button {
			on_press: FnWrapper(Box::new(on_press)),
			..Default::default()
		}
	}
}
impl<State: ValidState> ElementTrait<State> for Button<State> {
	type Inner = stardust_xr_molecules::button::Button;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_dbus_connection: &Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let mut button = stardust_xr_molecules::button::Button::create(
			parent_space,
			self.transform,
			self.size,
			stardust_xr_molecules::button::ButtonSettings {
				max_hover_distance: self.max_hover_distance,
				visuals: Some(ButtonVisualSettings {
					line_thickness: self.line_thickness,
					accent_color: self.accent_color,
				}),
			},
		)?;
		button.set_debug(self.debug);
		Ok(button)
	}

	fn update(
		&self,
		old: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		inner.handle_events();
		if inner.pressed() {
			(self.on_press.0)(state);
		}
		self.apply_transform(old, inner.touch_plane().root());
		// if self.size != old.size {
		//     inner.touch_plane().set_size(self.size);
		// }
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.touch_plane().root().clone().as_spatial_ref()
	}
}
impl<State: ValidState> Transformable for Button<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

#[tokio::test]
async fn asteroids_button_element() {
	use crate::{
		client::{self, ClientState},
		custom::ElementTrait,
		elements::Button,
		Element,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState;
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const QUALIFIER: &'static str = "org";
		const ORGANIZATION: &'static str = "asteroids";
		const NAME: &'static str = "button";

		fn reify(&self) -> Element<Self> {
			Button::new(|_| {
				std::process::exit(0);
			})
			.size([0.1, 0.1])
			.build()
		}
	}

	client::run::<TestState>(&[]).await
}
