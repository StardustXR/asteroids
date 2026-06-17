use crate::{Context, CustomElement, FnWrapper, Transformable, ValidState};
use derive_setters::Setters;
use stardust_xr_fusion::{
	Error,
	client::FrameInfo,
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, Transform},
};

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Derezzable<State: ValidState> {
	#[setters(skip)]
	shape: Shape,
	transform: Transform,
	#[setters(skip)]
	on_derez: FnWrapper<dyn Fn(&mut State) + Send + Sync + 'static>,
}
impl<State: ValidState> Derezzable<State> {
	pub fn new(on_derez: impl Fn(&mut State) + Send + Sync + 'static, shape: Shape) -> Self {
		Self {
			transform: Transform::IDENTITY,
			on_derez: FnWrapper(Box::new(on_derez)),
			shape,
		}
	}
}
impl<State: ValidState> Transformable for Derezzable<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
impl<State: ValidState> CustomElement<State> for Derezzable<State> {
	type Inner = (Spatial, Field, stardust_xr_molecules::Derezzable);
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: crate::CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (field, _) = Field::new(
			&context.stardust_client,
			&info.child_space,
			self.shape.clone(),
		)
		.await?;
		let derez = stardust_xr_molecules::Derezzable::new(
			&context.stardust_client,
			info.child_space.clone(),
			field.clone(),
		)
		.await?;
		Ok((info.child_space, field, derez))
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.0);
		if self.shape != old_self.shape {
			_ = inner.1.set_shape(self.shape.clone());
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if inner.2.receiver.try_recv().is_ok() {
			(self.on_derez.0)(state);
		}
	}
}

#[tokio::test]
async fn asteroids_derezzable_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
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
			crate::elements::Derezzable::new(|_| std::process::exit(0), shape.clone())
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
