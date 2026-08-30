use crate::{Component, ValidState};
use gluon::Node;

#[derive(Default, Debug)]
pub struct Container;
impl<State: ValidState> Component<State> for Container {
	type Inner = Node<stardust_xr_molecules::container::Container>;
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		_context: &crate::Context,
		info: crate::ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		stardust_xr_molecules::container::Container::new(info.queryable).await
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &crate::Context,
		_info: crate::ComponentCreateInfo<'_>,
		_inners: &mut crate::Inners<'_, State, Self>,
	) {
	}
}
