use crate::{Context, CustomElement, FnWrapper, Transformable, ValidState};
use derive_setters::Setters;
use stardust_xr_fusion::{
	node::NodeError,
	root::FrameInfo,
	spatial::{Spatial, SpatialRef, Transform},
};

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Derezzable<State: ValidState> {
	transform: Transform,
	#[setters(skip)]
	on_derez: FnWrapper<dyn Fn(&mut State) + Send + Sync + 'static>,
}
impl<State: ValidState> Derezzable<State> {
	pub fn new(on_derez: impl Fn(&mut State) + Send + Sync + 'static) -> Self {
		Self {
			transform: Transform::identity(),
			on_derez: FnWrapper(Box::new(on_derez)),
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
	type Inner = (stardust_xr_molecules::Derezzable, Spatial);
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		asteroids_context: &Context,
		info: crate::CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let spatial = Spatial::create(info.parent_space, Transform::identity(), false)?;
		let derez = stardust_xr_molecules::Derezzable::create(
			asteroids_context.dbus_connection.clone(),
			info.element_path,
			spatial.clone(),
			None,
		)?;
		Ok((derez, spatial))
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		self.apply_transform(old_self, &inner.1);
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if inner.0.receiver.try_recv().is_ok() {
			(self.on_derez.0)(state);
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.1.clone().as_spatial_ref()
	}
}

#[tokio::test]
async fn asteroids_derezzable_element() {
	use crate::client::{self, ClientState};
	use color::rgba_linear;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::fields::Shape;
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
		fn reify(&self) -> impl crate::Element<Self> {
			crate::elements::Derezzable::new(|_| std::process::exit(0))
				.build()
				.child(
					crate::elements::Lines::new(
						stardust_xr_molecules::lines::shape(Shape::Box([0.1; 3].into()))
							.into_iter()
							.map(|l| l.color(rgba_linear!(1.0, 0.1, 0.1, 1.0)).thickness(0.005)),
					)
					.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await
}
