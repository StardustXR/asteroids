use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable},
};
use glam::Mat4;
use stardust_xr_fusion::{
	Error,
	drawable::{Line, Lines, LinesExt},
	spatial::{Spatial, Transform},
	types::rgba_linear,
};
use stardust_xr_molecules::lines::{LineExt, line_from_points};
use std::{f32::consts::FRAC_PI_2, fmt::Debug};

#[derive(Debug, Clone)]
pub struct Axes {
	transform: Transform,
	thickness: f32,
	length: f32,
}
impl Default for Axes {
	fn default() -> Self {
		Self {
			transform: Transform::IDENTITY,
			thickness: 0.001,
			length: 0.01,
		}
	}
}
impl<State: ValidState> CustomElement<State> for Axes {
	type Inner = (Lines, Spatial);
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		Ok((
			Lines::new(
				&context.stardust_client,
				&info.child_space,
				axes(self.length, self.thickness).to_vec(),
			)
			.await?,
			info.child_space,
		))
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.1);
		if self.length != old_self.length || self.thickness != old_self.thickness {
			let _ = inner.0.set_lines(axes(self.length, self.thickness));
		}
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
		Tasker,
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
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Axes::default().build()
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
