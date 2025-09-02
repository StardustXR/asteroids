use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use glam::Mat4;
use stardust_xr_fusion::{
	drawable::{Line, Lines, LinesAspect},
	node::NodeError,
	spatial::{SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::lines::{LineExt, line_from_points};
use std::{f32::consts::FRAC_PI_2, fmt::Debug};

#[derive(Debug, Clone, PartialEq)]
pub struct Axes {
	transform: Transform,
	thickness: f32,
	length: f32,
}
impl Default for Axes {
	fn default() -> Self {
		Self {
			transform: Transform::identity(),
			thickness: 0.001,
			length: 0.01,
		}
	}
}
impl<State: ValidState> CustomElement<State> for Axes {
	type Inner = Lines;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		Lines::create(
			info.parent_space,
			self.transform,
			&axes(self.length, self.thickness),
		)
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		self.apply_transform(old_self, inner);
		if self.length != old_self.length || self.thickness != old_self.thickness {
			let _ = inner.set_lines(&axes(self.length, self.thickness));
		}
	}
	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.clone().as_spatial().as_spatial_ref()
	}
}
impl Transformable for Axes {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

fn axes(length: f32, thickness: f32) -> [Line; 3] {
	let axis_line = line_from_points(vec![[0.0; 3], [0.0, length, 0.0]]).thickness(thickness);
	[
		axis_line
			.clone()
			.transform(Mat4::from_rotation_z(-FRAC_PI_2))
			.color(rgba_linear!(1.0, 0.0, 0.0, 1.0)),
		axis_line.clone().color(rgba_linear!(0.0, 1.0, 0.0, 1.0)),
		axis_line
			.clone()
			.transform(Mat4::from_rotation_x(FRAC_PI_2))
			.color(rgba_linear!(0.0, 0.0, 1.0, 1.0)),
	]
}

#[tokio::test]
async fn asteroids_axes_test() {
	use crate::{
		client::{self, ClientState},
		custom::CustomElement,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState;
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.axes";
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			Axes::default().build()
		}
	}

	client::run::<TestState>(&[]).await;
}
