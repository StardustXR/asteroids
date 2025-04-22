use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{ElementTrait, Transformable},
};
use derive_setters::Setters;
use rustc_hash::{FxHashMap, FxHashSet};
use stardust_xr_fusion::{
	core::values::ResourceID,
	drawable::{MaterialParameter, ModelPartAspect},
	items::panel::{PanelItem, PanelItemAspect, SurfaceId},
	node::{NodeError, NodeResult},
	spatial::{SpatialRef, Transform},
};
use std::{fmt::Debug, hash::Hash, path::Path};
use zbus::Connection;

pub struct ModelInner {
	dbus_connection: Connection,
	parent: SpatialRef,
	model: stardust_xr_fusion::drawable::Model,
	model_parts: FxHashMap<String, stardust_xr_fusion::drawable::ModelPart>,
}
impl ModelInner {
	pub fn create(
		parent_space: &SpatialRef,
		dbus_connection: &Connection,
		decl: &Model,
	) -> NodeResult<Self> {
		let model = stardust_xr_fusion::drawable::Model::create(
			parent_space,
			decl.transform,
			&decl.resource,
		)?;
		let model_parts = decl
			.model_parts
			.iter()
			.filter_map(|p| {
				let part = model.part(&p.path).ok()?;
				p.apply_material_parameters(&part).ok()?;
				Some((p.path.clone(), part))
			})
			.collect();
		let inner = ModelInner {
			dbus_connection: dbus_connection.clone(),
			parent: parent_space.clone(),
			model,
			model_parts,
		};
		Ok(inner)
	}
}
#[derive(Debug, Clone, PartialEq)]
pub struct ModelPart {
	path: String,
	material_parameter_overrides: FxHashMap<String, MaterialParameter>,
	panel_item_override: Option<(PanelItem, SurfaceId)>,
	panel_item_cursor_override: Option<PanelItem>,
}
impl ModelPart {
	pub fn new(path: &str) -> Self {
		ModelPart {
			path: path.to_string(),
			material_parameter_overrides: FxHashMap::default(),
			panel_item_override: None,
			panel_item_cursor_override: None,
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
	pub fn apply_panel_item_cursor(mut self, panel_item: PanelItem) -> Self {
		self.panel_item_cursor_override.replace(panel_item);
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
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		ModelInner::create(info.parent_space, &context.dbus_connection, self)
	}
	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, &inner.model);
		if self.resource != old_decl.resource {
			if let Ok(new_inner) = ModelInner::create(&inner.parent, &inner.dbus_connection, self) {
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
			let Some(model_part) = inner.model_parts.get(&part_info.path) else {
				return;
			};
			if let Some((panel_override, surface_id)) = &part_info.panel_item_override {
				let _ = panel_override.apply_surface_material(surface_id.clone(), model_part);
			}
			if let Some(panel_item_cursor) = &part_info.panel_item_cursor_override {
				let _ = panel_item_cursor.apply_cursor_material(model_part);
			}
			if let Some(old_part_info) = old_decl.model_parts.get(part_info) {
				if part_info.material_parameter_overrides
					!= old_part_info.material_parameter_overrides
				{
					let _ = part_info.apply_material_parameters(model_part);
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
	pub fn direct(path: impl AsRef<Path>) -> std::io::Result<Self> {
		Ok(Model {
			transform: Transform::none(),
			resource: ResourceID::new_direct(path)?,
			model_parts: Default::default(),
		})
	}
	pub fn part(mut self, info: ModelPart) -> Self {
		self.model_parts.insert(info);
		self
	}
}
