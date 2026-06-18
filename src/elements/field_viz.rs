use std::sync::Arc;

use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, Transformable, derive_setters::Setters},
};
use glam::{Vec3, Vec3A};
use mint::Vector3;
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, Lines, LinesExt},
	fields::Shape,
	spatial::{Spatial, Transform},
	types::{Color, rgba_linear},
};
use stardust_xr_molecules::lines::{LineExt, line_from_points};

#[derive(Clone, Setters)]
#[setters(into, strip_option)]
pub struct FieldViz {
	transform: Transform,
	shape: Shape,
	grid_size: Vector3<usize>,
	sample_size: f32,
	normal_length: f32,
	line_thickness: f32,
	color: Color,
	#[setters(skip)]
	color_fn: Arc<dyn Fn(f32) -> Color + Send + Sync>,
}

impl std::fmt::Debug for FieldViz {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("FieldViz")
			.field("transform", &self.transform)
			.field("shape", &self.shape)
			.field("grid_size", &self.grid_size)
			.field("sample_size", &self.sample_size)
			.field("normal_length", &self.normal_length)
			.field("line_thickness", &self.line_thickness)
			.field("color", &self.color)
			.field("color_fn", &"<function>")
			.finish()
	}
}
impl Default for FieldViz {
	fn default() -> Self {
		Self {
			transform: Transform::IDENTITY,
			shape: Shape::Sphere { radius: 1.0 },
			grid_size: [5, 5, 5].into(),
			sample_size: 0.5,
			normal_length: 0.1,
			line_thickness: 0.001,
			color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
			color_fn: Arc::new(|d: f32| {
				let t = (d * 20.0).clamp(-1.0, 1.0) * 0.5 + 0.5;
				if t > 0.5 {
					let t = (t - 0.5) * 2.0;
					rgba_linear!(1.0 - t, 0.5 * (1.0 - t), 0.0, 1.0)
				} else {
					let t = t * 2.0;
					rgba_linear!(1.0, 0.5 + (0.5 * t), t, 1.0)
				}
			}),
		}
	}
}

impl FieldViz {
	pub fn color_fn<F>(mut self, f: F) -> Self
	where
		F: Fn(f32) -> Color + Send + Sync + 'static,
	{
		self.color_fn = Arc::new(f);
		self
	}

	/// Sample the shape's signed-distance field across the grid and build a
	/// normal line at every grid point.
	///
	/// This runs entirely locally via [`Shape::sample`] — no server round-trip.
	/// The sample's `gradient` is the exact outward unit normal, so there is no
	/// finite-difference estimation, and its `distance` drives the per-line color.
	fn compute_lines(&self) -> Vec<Line> {
		let half_size = Vec3::new(
			self.grid_size.x as f32 - 1.0,
			self.grid_size.y as f32 - 1.0,
			self.grid_size.z as f32 - 1.0,
		) * self.sample_size
			* 0.5;

		let mut lines = Vec::new();
		for x in 0..self.grid_size.x {
			for y in 0..self.grid_size.y {
				for z in 0..self.grid_size.z {
					let pos = Vec3::new(
						(x as f32 * self.sample_size) - half_size.x,
						(y as f32 * self.sample_size) - half_size.y,
						(z as f32 * self.sample_size) - half_size.z,
					);

					let sample = self.shape.sample(Vec3A::from(pos));
					let normal = Vec3::from(sample.gradient).normalize_or_zero();
					let end = pos + (normal * self.normal_length);

					let line_color = (self.color_fn)(sample.distance);
					if line_color.a > 0.0 {
						lines.push(
							line_from_points(vec![[pos.x, pos.y, pos.z], [end.x, end.y, end.z]])
								.color(line_color)
								.thickness(self.line_thickness),
						);
					}
				}
			}
		}

		lines
	}
}

pub struct FieldVizInner {
	content_root: Spatial,
	lines: Lines,
}

impl<State: ValidState> CustomElement<State> for FieldViz {
	type Inner = FieldVizInner;
	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let content_root = info.child_space;
		content_root.set_local_transform(self.transform)?;

		let lines = Lines::new(
			&context.stardust_client,
			&content_root,
			self.compute_lines(),
		)
		.await?;

		Ok(FieldVizInner {
			content_root,
			lines,
		})
	}

	fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.shape != old.shape
			|| self.grid_size != old.grid_size
			|| self.sample_size != old.sample_size
			|| self.normal_length != old.normal_length
			|| self.line_thickness != old.line_thickness
		{
			let _ = inner.lines.set_lines(self.compute_lines());
		}

		self.apply_transform(old, &inner.content_root);
	}
}

impl Transformable for FieldViz {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

#[tokio::test]
async fn asteroids_field_viz_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::FieldViz,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{client::FrameInfo, fields::Shape};

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState(f32);
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.field_viz";

		fn on_frame(&mut self, info: &FrameInfo) {
			self.0 += info.delta;
		}
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			FieldViz::default()
				.shape(Shape::Transform {
					shape: Box::new(Shape::Torus {
						major_radius: 0.1,
						minor_radius: 0.01,
					}),
					transform: glam::Mat4::from_translation([0.0, self.0.sin() * 0.1, 0.0].into())
						.into(),
				})
				.grid_size([11, 11, 11])
				.sample_size(0.025)
				.normal_length(0.01)
				.build()
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
