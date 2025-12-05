use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use derive_setters::Setters;
use stardust_xr_fusion::{
	drawable::{TextAspect, TextBounds, TextStyle, XAlign, YAlign},
	node::NodeError,
	spatial::{SpatialRef, Transform},
	values::color::rgba_linear,
	values::{Color, ResourceID},
};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Text {
	transform: Transform,
	#[setters(skip)]
	text: String,
	character_height: f32,
	color: Color,
	font: Option<ResourceID>,
	align_x: XAlign,
	align_y: YAlign,
	bounds: Option<TextBounds>,
}
impl Text {
	pub fn new(text: impl ToString) -> Self {
		Text {
			transform: Transform::none(),
			text: text.to_string(),
			character_height: 0.01,
			color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
			font: None,
			align_x: XAlign::Left,
			align_y: YAlign::Top,
			bounds: None,
		}
	}
}
impl<State: ValidState> CustomElement<State> for Text {
	type Inner = stardust_xr_fusion::drawable::Text;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::drawable::Text::create(
			info.parent_space,
			self.transform,
			&self.text,
			TextStyle {
				character_height: self.character_height,
				color: self.color,
				font: self.font.clone(),
				text_align_x: self.align_x,
				text_align_y: self.align_y,
				bounds: self.bounds.clone(),
			},
		)
	}
	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		self.apply_transform(old_self, inner);
		if self.text != old_self.text {
			let _ = inner.set_text(&self.text);
		}
		if self.character_height != old_self.character_height {
			let _ = inner.set_character_height(self.character_height);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial().as_spatial_ref()
	}
}
impl Transformable for Text {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

#[tokio::test]
async fn asteroids_text_test() {
	use crate::{
		client::{self, ClientState},
		custom::CustomElement,
		elements::{Axes, Lines},
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_molecules::lines::{LineExt, line_from_points};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState;
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.text";
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			let cell_size_x = 0.1;
			let cell_size_y = 0.01;
			Lines::new([
				// -x divider line
				line_from_points(vec![
					[cell_size_x / 2.0, cell_size_y * 1.5, 0.0],
					[cell_size_x / 2.0, -cell_size_y * 1.5, 0.0],
				])
				.thickness(0.00075),
				// +x divider line
				line_from_points(vec![
					[-cell_size_x / 2.0, cell_size_y * 1.5, 0.0],
					[-cell_size_x / 2.0, -cell_size_y * 1.5, 0.0],
				])
				.thickness(0.00075),
				// +y divider line
				line_from_points(vec![
					[-cell_size_x * 1.5, cell_size_y / 2.0, 0.0],
					[cell_size_x * 1.5, cell_size_y / 2.0, 0.0],
				])
				.thickness(0.00075),
				// -y divider line
				line_from_points(vec![
					[-cell_size_x * 1.5, -cell_size_y / 2.0, 0.0],
					[cell_size_x * 1.5, -cell_size_y / 2.0, 0.0],
				])
				.thickness(0.00075),
			])
			.build()
			.child(
				Text::new("Top left align")
					.align_y(YAlign::Top)
					.align_x(XAlign::Left)
					.pos([-cell_size_x * 1.5, -cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Top center align")
					.align_y(YAlign::Top)
					.align_x(XAlign::Center)
					.pos([0.0, -cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Top right align")
					.align_y(YAlign::Top)
					.align_x(XAlign::Right)
					.pos([cell_size_x * 1.5, -cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Middle left align")
					.align_y(YAlign::Center)
					.align_x(XAlign::Left)
					.pos([-cell_size_x * 1.5, 0.0, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Middle center align")
					.align_y(YAlign::Center)
					.align_x(XAlign::Center)
					.pos([0.0, 0.0, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Middle right align")
					.align_y(YAlign::Center)
					.align_x(XAlign::Right)
					.pos([cell_size_x * 1.5, 0.0, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Bottom left align")
					.align_y(YAlign::Bottom)
					.align_x(XAlign::Left)
					.pos([-cell_size_x * 1.5, cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Bottom center align")
					.align_y(YAlign::Bottom)
					.align_x(XAlign::Center)
					.pos([0.0, cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
			.child(
				Text::new("Bottom right align")
					.align_y(YAlign::Bottom)
					.align_x(XAlign::Right)
					.pos([cell_size_x * 1.5, cell_size_y * 1.5, 0.0])
					.build()
					.child(Axes::default().build()),
			)
		}
	}

	client::run::<TestState>(&[]).await;
}
