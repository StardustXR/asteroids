use crate::{Component, ComponentCreateInfo, Context, Inners, ValidState};
use gluon::Node;
use stardust_xr_fusion::Error;
use stardust_xr_molecules::container;

#[derive(Default, Debug)]
pub struct Container;
impl<State: ValidState> Component<State> for Container {
	type Inner = Node<container::Container>;
	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		container::Container::new(info.queryable).await
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}
}
