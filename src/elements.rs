use crate::{
	custom::{ElementTrait, FnWrapper, Transformable},
	ValidState,
};
use derive_setters::Setters;
use derive_where::derive_where;
use mint::Vector2;
use rustc_hash::{FxHashMap, FxHashSet};
use stardust_xr_fusion::{
	core::values::{Color, ResourceID},
	drawable::{
		Line, LinesAspect, MaterialParameter, ModelPartAspect, TextAspect, TextBounds, TextStyle,
		XAlign, YAlign,
	},
	items::panel::{PanelItem, PanelItemAspect, SurfaceId},
	node::{NodeError, NodeResult},
	spatial::{SpatialAspect, SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::{button::ButtonVisualSettings, DebugSettings, UIElement, VisualDebug};
use std::{fmt::Debug, hash::Hash};

#[derive(Debug, Clone, Copy, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Spatial {
	transform: Transform,
	zoneable: bool,
}
impl<State: ValidState> ElementTrait<State> for Spatial {
	type Inner = stardust_xr_fusion::spatial::Spatial;
	type Error = NodeError;

	fn create_inner(&self, spatial_parent: &SpatialRef) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::spatial::Spatial::create(spatial_parent, self.transform, self.zoneable)
	}
	fn update(&self, old_decl: &Self, _state: &mut State, inner: &mut Self::Inner) {
		self.apply_transform(old_decl, inner);
		if self.zoneable != old_decl.zoneable {
			let _ = inner.set_zoneable(self.zoneable);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial_ref()
	}
}
impl Default for Spatial {
	fn default() -> Self {
		Spatial {
			transform: Transform::none(),
			zoneable: false,
		}
	}
}
impl Transformable for Spatial {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

pub struct ModelInner {
	parent: SpatialRef,
	model: stardust_xr_fusion::drawable::Model,
	model_parts: FxHashMap<String, stardust_xr_fusion::drawable::ModelPart>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPart {
	path: String,
	material_parameter_overrides: FxHashMap<String, MaterialParameter>,
	panel_item_override: Option<(PanelItem, SurfaceId)>,
}
impl ModelPart {
	pub fn new(path: &str) -> Self {
		ModelPart {
			path: path.to_string(),
			material_parameter_overrides: FxHashMap::default(),
			panel_item_override: None,
		}
	}
	pub fn mat_param(mut self, name: &str, value: MaterialParameter) -> Self {
		self.material_parameter_overrides
			.insert(name.to_string(), value);
		self
	}
	pub fn apply_panel_item(mut self, panel_item: PanelItem, surface_id: SurfaceId) -> Self {
		self.panel_item_override.replace((panel_item, surface_id));
		self
	}
	fn apply_material_parameters(
		&self,
		part: &stardust_xr_fusion::drawable::ModelPart,
	) -> NodeResult<()> {
		for (param_name, param_value) in &self.material_parameter_overrides {
			part.set_material_parameter(param_name, param_value.clone())?;
		}
		Ok(())
	}
}
impl Hash for ModelPart {
	fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
		self.path.hash(state)
	}
}
impl Eq for ModelPart {}
#[derive(Debug, Clone, Setters)]
#[setters(into, strip_option)]
pub struct Model {
	transform: Transform,
	pub resource: ResourceID,
	pub model_parts: FxHashSet<ModelPart>,
}
impl<State: ValidState> ElementTrait<State> for Model {
	type Inner = ModelInner;
	type Error = NodeError;

	fn create_inner(&self, spatial_parent: &SpatialRef) -> Result<Self::Inner, Self::Error> {
		let model = stardust_xr_fusion::drawable::Model::create(
			spatial_parent,
			self.transform,
			&self.resource,
		)?;
		let model_parts = self
			.model_parts
			.iter()
			.filter_map(|p| {
				let part = model.part(&p.path).ok()?;
				p.apply_material_parameters(&part).ok()?;
				Some((p.path.clone(), part))
			})
			.collect();
		let inner = ModelInner {
			parent: spatial_parent.clone(),
			model,
			model_parts,
		};
		Ok(inner)
	}
	fn update(&self, old_decl: &Self, _state: &mut State, inner: &mut Self::Inner) {
		self.apply_transform(old_decl, &inner.model);
		if self.resource != old_decl.resource {
			if let Ok(new_inner) = <Self as ElementTrait<State>>::create_inner(self, &inner.parent)
			{
				*inner = new_inner;
			}
		}
		// just added
		for part_info in self.model_parts.difference(&old_decl.model_parts) {
			let Ok(part) = inner.model.part(&part_info.path) else {
				continue;
			};
			if part_info.apply_material_parameters(&part).is_err() {
				continue;
			}
			inner.model_parts.insert(part_info.path.clone(), part);
		}
		//still here
		for part_info in self.model_parts.union(&old_decl.model_parts) {
			if let Some(model_part) = inner.model_parts.get(&part_info.path) {
				if let Some(old_part_info) = old_decl.model_parts.get(part_info) {
					if part_info.material_parameter_overrides
						!= old_part_info.material_parameter_overrides
					{
						let _ = part_info.apply_material_parameters(model_part);
					}
					if let Some((panel_override, surface_id)) = &part_info.panel_item_override {
						if let Some((old_panel_override, old_surface_id)) =
							&old_part_info.panel_item_override
						{
							if panel_override != old_panel_override && surface_id != old_surface_id
							{
								let _ = panel_override
									.apply_surface_material(surface_id.clone(), model_part);
							}
						} else {
							let _ = panel_override
								.apply_surface_material(surface_id.clone(), model_part);
						}
					}
				} else {
					let _ = part_info.apply_material_parameters(model_part);
					if let Some((panel_override, surface_id)) = &part_info.panel_item_override {
						let _ =
							panel_override.apply_surface_material(surface_id.clone(), model_part);
					}
				}
			}
		}
		// just removed
		for part_info in old_decl.model_parts.difference(&self.model_parts) {
			inner.model_parts.remove(&part_info.path);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.model.clone().as_spatial().as_spatial_ref()
	}
}
impl Transformable for Model {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
impl Model {
	pub fn namespaced(namespace: &str, path: &str) -> Self {
		Model {
			transform: Transform::none(),
			resource: ResourceID::new_namespaced(namespace, path),
			model_parts: Default::default(),
		}
	}
	pub fn part(mut self, info: ModelPart) -> Self {
		self.model_parts.insert(info);
		self
	}
}

#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Lines {
	transform: Transform,
	lines: Vec<Line>,
}
impl<State: ValidState> ElementTrait<State> for Lines {
	type Inner = stardust_xr_fusion::drawable::Lines;
	type Error = NodeError;

	fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::drawable::Lines::create(parent_space, self.transform, &self.lines)
	}

	fn update(&self, old_decl: &Self, _state: &mut State, inner: &mut Self::Inner) {
		self.apply_transform(old_decl, inner);
		if self.lines != old_decl.lines {
			let _ = inner.set_lines(&self.lines);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial().as_spatial_ref()
	}
}
impl Default for Lines {
	fn default() -> Self {
		Lines {
			transform: Transform::none(),
			lines: vec![],
		}
	}
}
impl Transformable for Lines {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

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
impl<State: ValidState> ElementTrait<State> for Text {
	type Inner = stardust_xr_fusion::drawable::Text;
	type Error = NodeError;

	fn create_inner(&self, spatial_parent: &SpatialRef) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::drawable::Text::create(
			spatial_parent,
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
	fn update(&self, old_decl: &Self, _state: &mut State, inner: &mut Self::Inner) {
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
	type Error = NodeError;

	fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error> {
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

	fn update(&self, old: &Self, state: &mut State, inner: &mut Self::Inner) {
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

// #[derive_where(Debug, Clone, PartialEq, Setters)]
// #[setters(into, strip_option)]
// pub struct Grabbable {
//     transform: Transform,
// }
// impl<State: ValidState> ElementTrait<State> for Grabbable {
//     type Inner = stardust_xr_molecules::Grabbable;
//     type Error = NodeError;

//     fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error> {
//         stardust_xr_molecules::Grabbable::create(
//             parent_space,
//             self.transform.clone(),
//             field,
//             GrabbableSettings::default(),
//         )
//     }

//     fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
//         self.apply_transform(old_decl, inner.content_parent())
//     }

//     fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
//         Some(inner.content_parent())
//     }
// }
// impl Transformable for Grabbable {
//     fn transform(&self) -> &Transform {
//         &self.transform
//     }
//     fn transform_mut(&mut self) -> &mut Transform {
//         &mut self.transform
//     }
// }
