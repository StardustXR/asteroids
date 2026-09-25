use crate::{Component, ComponentCreateInfo, Context, Inners, ValidState};
use stardust_xr_fusion::{
	Error,
	drawable::{Line, LinesExt},
};

/// lines drawn straight on the entity's spatial, so no extra spatial like the element needs
#[derive(Debug, Clone)]
pub struct Lines {
	lines: Vec<Line>,
}
impl Lines {
	pub fn new(lines: impl IntoIterator<Item = Line>) -> Self {
		Lines {
			lines: lines.into_iter().collect(),
		}
	}
}
impl<State: ValidState> Component<State> for Lines {
	type Inner = stardust_xr_fusion::drawable::Lines;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_fusion::drawable::Lines::new(
			&context.stardust_client,
			info.spatial,
			self.lines.clone(),
		)
		.await
	}

	fn diff(
		&self,
		old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		if self.lines != old_self.lines {
			let _ = inners.self_inner().set_lines(self.lines.clone());
		}
	}
}
