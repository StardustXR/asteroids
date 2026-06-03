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
use tokio::sync::mpsc;

pub struct ModelInner {
	spatial: Spatial,
	model: stardust_xr_fusion::drawable::Model,
	model_parts: FxHashMap<String, stardust_xr_fusion::drawable::ModelPart>,
	pending_parts_tx: mpsc::UnboundedSender<(String, stardust_xr_fusion::drawable::ModelPart)>,
	pending_parts_rx: mpsc::UnboundedReceiver<(String, stardust_xr_fusion::drawable::ModelPart)>,
}
impl ModelInner {
	pub async fn create(context: &Context, spatial: Spatial, decl: &Model) -> Result<Self> {
		let model = stardust_xr_fusion::drawable::Model::create(
			&context.stardust_client,
			&spatial,
			decl.resource.clone(),
		)
		.await?;
		let mut model_parts = FxHashMap::default();
		for p in &decl.model_parts {
			let Some(part) = model.get_part(p.path.clone()).await.ok().flatten() else {
				continue;
			};
			p.apply_material_parameters(&part);
			model_parts.insert(p.path.clone(), part);
		}
		let (pending_parts_tx, pending_parts_rx) = mpsc::unbounded_channel();
		Ok(ModelInner {
			spatial,
			model,
			model_parts,
			pending_parts_tx,
			pending_parts_rx,
		})
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
	fn apply_material_parameters(&self, part: &stardust_xr_fusion::drawable::ModelPart) {
		// TODO: use joinset or something nicer than this
		for (param_name, param_value) in self.material_parameter_overrides.clone() {
			let part = part.clone();
			tokio::spawn(async move {
				let _ = part.set_material_parameter(param_name, param_value).await;
			});
		}
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

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		ModelInner::create(context, info.child_space, self).await
	}
	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		_state: &mut State,
		inner: &mut Self::Inner,
	) {
		while let Ok((path, part)) = inner.pending_parts_rx.try_recv() {
			inner.model_parts.insert(path, part);
		}
	}
	fn diff(&self, old_self: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.spatial);
		if self.resource != old_self.resource {
			// TODO: resource changes require full element recreation
		}
		// just added
		for part_info in self.model_parts.difference(&old_self.model_parts) {
			let model = inner.model.clone();
			let part_info = part_info.clone();
			let tx = inner.pending_parts_tx.clone();
			tokio::spawn(async move {
				let Some(part) = model.get_part(part_info.path.clone()).await.ok().flatten() else {
					return;
				};
				part_info.apply_material_parameters(&part);
				let _ = tx.send((part_info.path.clone(), part));
			});
		}
		// still here
		for part_info in self.model_parts.intersection(&old_self.model_parts) {
			let Some(model_part) = inner.model_parts.get(&part_info.path) else {
				continue;
			};
			if let Some(old_part_info) = old_self.model_parts.get(part_info) {
				if part_info.material_parameter_overrides
					!= old_part_info.material_parameter_overrides
				{
					part_info.apply_material_parameters(model_part);
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
