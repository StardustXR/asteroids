use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{ElementTrait, Transformable},
};
use stardust_xr_fusion::{
	drawable::{Line, LinesAspect},
	node::NodeError,
	spatial::{SpatialRef, Transform},
};
use std::fmt::Debug;

pub use stardust_xr_molecules::lines::*;

#[derive(Debug, Clone, PartialEq)]
pub struct Lines {
	transform: Transform,
	lines: Vec<Line>,
}
impl Lines {
	pub fn new(lines: impl IntoIterator<Item = Line>) -> Self {
		Lines {
			transform: Transform::identity(),
			lines: lines.into_iter().collect(),
		}
	}
}
impl<State: ValidState> ElementTrait<State> for Lines {
	type Inner = stardust_xr_fusion::drawable::Lines;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::drawable::Lines::create(info.parent_space, self.transform, &self.lines)
	}

	fn update(
		&self,
		old_decl: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.apply_transform(old_decl, inner);
		if self.lines != old_decl.lines {
			let _ = inner.set_lines(&self.lines);
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial().as_spatial_ref()
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
