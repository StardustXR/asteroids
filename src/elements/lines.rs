use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use stardust_xr_fusion::{
	Error,
	drawable::{Line, LinesExt},
	spatial::{Spatial, Transform},
};
use std::fmt::Debug;

pub use stardust_xr_molecules::lines::*;

#[derive(Debug, Clone)]
pub struct Lines {
	transform: Transform,
	lines: Vec<Line>,
}
impl Lines {
	pub fn new(lines: impl IntoIterator<Item = Line>) -> Self {
		Lines {
			transform: Transform::IDENTITY,
			lines: lines.into_iter().collect(),
		}
	}
}
impl<State: ValidState> CustomElement<State> for Lines {
	type Inner = (Spatial, stardust_xr_fusion::drawable::Lines);
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		info.child_space.set_local_transform(self.transform)?;
		let lines = stardust_xr_fusion::drawable::Lines::create(
			&context.stardust_client,
			&info.child_space,
			self.lines.clone(),
		)
		.await?;
		Ok((info.child_space, lines))
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.0);
		if self.lines != old_self.lines {
			let _ = inner.1.set_lines(self.lines.clone());
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
