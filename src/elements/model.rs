use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use derive_setters::Setters;
use rustc_hash::{FxHashMap, FxHashSet};
use stardust_xr_fusion::{
	Error, Result,
	drawable::{MaterialParameter, ModelExt},
	spatial::{Spatial, Transform},
	types::Resource,
};
use std::{fmt::Debug, hash::Hash, path::Path};

pub struct ModelInner {
	spatial: Spatial,
	model: stardust_xr_fusion::drawable::Model,
	model_parts: FxHashMap<String, stardust_xr_fusion::drawable::ModelPart>,
}
impl ModelInner {
	pub async fn create(
		context: &Context,
		spatial: Spatial,
		decl: &Model,
	) -> Result<Self> {
		let model = stardust_xr_fusion::drawable::Model::create(
			&context.stardust_client,
			&spatial,
			decl.resource,
		).await?;
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
			spatial,
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
}
impl ModelPart {
	pub fn new(path: &str) -> Self {
		ModelPart {
			path: path.to_string(),
			material_parameter_overrides: FxHashMap::default(),
		}
	}
	pub fn mat_param(mut self, name: &str, value: MaterialParameter) -> Self {
		self.material_parameter_overrides
			.insert(name.to_string(), value);
		self
	}
	fn apply_material_parameters(
		&self,
		part: &stardust_xr_fusion::drawable::ModelPart,
	) -> Result<()> {
		// TODO: use joinset or something nicer than this
		for (param_name, param_value) in self.material_parameter_overrides.clone() {
			let part = part.clone();
			tokio::spawn(async move {
				part.set_material_parameter(param_name, param_value.clone())
					.await
			});
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
	pub resource: Resource,
	pub model_parts: FxHashSet<ModelPart>,
}
impl<State: ValidState> CustomElement<State> for Model {
	type Inner = ModelInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner> {
		ModelInner::create(context,  info.child_space, self).await
	}
	fn diff(&self, old_self: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.spatial);
		if self.resource != old_self.resource {
			if let Ok(new_inner) = ModelInner::create(&inner.spatial, , self)
			{
				*inner = new_inner;
			}
		}
		// just added
		for part_info in self.model_parts.difference(&old_self.model_parts) {
			let Ok(part) = inner.model.part(&part_info.path) else {
				continue;
			};
			if part_info.apply_material_parameters(&part).is_err() {
				continue;
			}
			inner.model_parts.insert(part_info.path.clone(), part);
		}
		//still here
		for part_info in self.model_parts.union(&old_self.model_parts) {
			let Some(model_part) = inner.model_parts.get(&part_info.path) else {
				return;
			};
			if let Some(old_part_info) = old_self.model_parts.get(part_info) {
				if part_info.material_parameter_overrides
					!= old_part_info.material_parameter_overrides
				{
					let _ = part_info.apply_material_parameters(model_part);
				}
			}
		}

		// just removed
		for part_info in old_self.model_parts.difference(&self.model_parts) {
			inner.model_parts.remove(&part_info.path);
		}
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
			transform: Transform::IDENTITY,
			resource: Resource::Namespaced {
				namespace: namespace.into(),
				path: path.into(),
			},
			model_parts: Default::default(),
		}
	}
	pub fn direct(path: impl AsRef<Path>) -> std::io::Result<Self> {
		Ok(Model {
			transform: Transform::IDENTITY,
			resource: Resource::Direct {
				path: path
					.as_ref()
					.to_str()
					.ok_or(std::io::ErrorKind::Other)?
					.to_string(),
			},
			model_parts: Default::default(),
		})
	}
	pub fn part(mut self, info: ModelPart) -> Self {
		self.model_parts.insert(info);
		self
	}
}
