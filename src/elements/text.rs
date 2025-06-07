use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use derive_setters::Setters;
use stardust_xr_fusion::{
	core::values::{Color, ResourceID},
	drawable::{TextAspect, TextBounds, TextStyle, XAlign, YAlign},
	node::NodeError,
	spatial::{SpatialRef, Transform},
	values::color::rgba_linear,
};
use std::fmt::Debug;

#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Text {
	transform: Transform,
	text: String,
	character_height: f32,
	color: Color,
	font: Option<ResourceID>,
	text_align_x: XAlign,
	text_align_y: YAlign,
	bounds: Option<TextBounds>,
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
				text_align_x: self.text_align_x,
				text_align_y: self.text_align_y,
				bounds: self.bounds.clone(),
			},
		)
	}
	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, inner);
		if self.text != old_decl.text {
			let _ = inner.set_text(&self.text);
		}
		if self.character_height != old_decl.character_height {
			let _ = inner.set_character_height(self.character_height);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial().as_spatial_ref()
	}
}
impl Default for Text {
	fn default() -> Self {
		Text {
			transform: Transform::none(),
			text: "".to_string(),
			character_height: 1.0,
			color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
			font: None,
			text_align_x: XAlign::Left,
			text_align_y: YAlign::Top,
			bounds: None,
		}
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
