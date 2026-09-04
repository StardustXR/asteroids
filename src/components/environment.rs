use crate::{Component, ValidState};
use gluon::Interface;
use stardust_xr_fusion::query::QueryableInterface;
use stardust_xr_molecules::environment::EnvironmentObject;

#[derive(Debug)]
pub struct Environment;
impl<State: ValidState> Component<State> for Environment {
	type Inner = QueryableInterface;
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		_context: &crate::Context,
		info: crate::ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		Ok(info
			.queryable
			.add_interface(
				&EnvironmentObject::new()?,
				stardust_xr_molecules::environment::Environment::ID,
			)
			.await??)
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
