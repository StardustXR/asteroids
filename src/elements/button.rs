use crate::{
	CreateInnerInfo, ValidState,
	context::Context,
	custom::{CustomElement, FnWrapper, Transformable},
};
use derive_setters::Setters;
use mint::Vector2;
use stardust_xr_fusion::{
	Error,
	spatial::{Spatial, Transform},
};
use stardust_xr_molecules::{DebugSettings, UIElement, VisualDebug, button::ButtonVisualSettings};

#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Button<State: ValidState> {
	transform: Transform,
	on_press: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
	size: Vector2<f32>,
	max_hover_distance: f32,
	line_thickness: f32,
	debug: Option<DebugSettings>,
}

impl<State: ValidState> std::fmt::Debug for Button<State> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Button")
			.field("transform", &self.transform)
			.field("on_press", &self.on_press)
			.field("size", &self.size)
			.field("max_hover_distance", &self.max_hover_distance)
			.field("line_thickness", &self.line_thickness)
			.field("debug", &self.debug)
			.finish()
	}
}
impl<State: ValidState> Default for Button<State> {
	fn default() -> Self {
		Button {
			transform: Transform::IDENTITY,
			on_press: FnWrapper(Box::new(|_| {})),
			size: [0.1; 2].into(),
			max_hover_distance: 0.025,
			line_thickness: 0.005,
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
impl<State: ValidState> CustomElement<State> for Button<State> {
	type Inner = (Spatial, stardust_xr_molecules::button::Button);
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let mut button = stardust_xr_molecules::button::Button::new(
			&context.stardust_client,
			&info.parent_space,
			self.transform,
			self.size,
			stardust_xr_molecules::button::ButtonSettings {
				max_hover_distance: self.max_hover_distance,
				visuals: Some(ButtonVisualSettings {
					line_thickness: self.line_thickness,
					accent_color: context.accent_color.color(),
				}),
			},
		)
		.await?;
		button.set_debug(self.debug);
		info.child_space
			.set_parent(button.touch_plane().root().spatial_ref().await?)?;
		Ok((info.child_space, button))
	}

	fn diff(&self, old: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old, inner.1.touch_plane().root());
		if self.size != old.size {
			inner.1.set_size(self.size);
		}
	}

	fn frame(
		&self,
		context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		inner.1.set_visual_settings(Some(ButtonVisualSettings {
			line_thickness: self.line_thickness,
			accent_color: context.accent_color.color(),
		}));
		inner.1.handle_events();
		if inner.1.pressed() {
			(self.on_press.0)(state);
		}
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
		Reify, Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::Button,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState;
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.button";
	}
	impl Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Button::new(|_| {
				// std::process::exit(0);
			})
			.debug(DebugSettings::default())
			.size([0.1, 0.1])
			.build()
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
