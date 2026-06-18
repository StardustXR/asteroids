use crate::{Component, ComponentCreateInfo, Context, ValidState, custom::FnWrapper};
use stardust_xr_fusion::{Error, client::FrameInfo};

#[derive_where::derive_where(Debug)]
pub struct Derezzable<State: ValidState> {
	on_derez: FnWrapper<dyn Fn(&mut State) + Send + Sync + 'static>,
}
impl<State: ValidState> Derezzable<State> {
	pub fn new(on_derez: impl Fn(&mut State) + Send + Sync + 'static) -> Self {
		Self {
			on_derez: FnWrapper(Box::new(on_derez)),
		}
	}
}
impl<State: ValidState> Component<State> for Derezzable<State> {
	// the entity owns the shared spatial/field; we just attach a molecules Derezzable to them
	type Inner = stardust_xr_molecules::Derezzable;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let derez = stardust_xr_molecules::Derezzable::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.field.clone(),
		)
		.await?;
		Ok(derez)
	}

	fn diff(&self, _old_self: &Self, _context: &Context, _inner: &mut Self::Inner) {}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if inner.receiver.try_recv().is_ok() {
			(self.on_derez.0)(state);
		}
	}
}

#[tokio::test]
async fn asteroids_derezzable_element() {
	use crate::{
		Context, Entity, Tasker,
		client::{self, ClientState},
		custom::CustomElement,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{fields::Shape, types::rgba_linear};
	use stardust_xr_molecules::lines::LineExt;

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState {
		derezzed: bool,
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.derezzable";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			let shape = Shape::Box {
				size: [0.1; 3].into(),
			};
			Entity::new(shape.clone())
				.component(crate::components::Derezzable::new(|_| std::process::exit(0)))
				.build()
				.child(
					crate::elements::Lines::new(
						stardust_xr_molecules::lines::shape(shape)
							.into_iter()
							.map(|l| l.color(rgba_linear!(1.0, 0.1, 0.1, 1.0)).thickness(0.005)),
					)
					.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await.unwrap()
}
