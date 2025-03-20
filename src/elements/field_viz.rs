use crate::{
	custom::{ElementTrait, Transformable},
	ValidState,
};
use derive_setters::Setters;
use glam::Vec3;
use mint::Vector3;
use stardust_xr_fusion::{
	core::values::Color,
	drawable::{Line, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, FieldRefAspect, Shape, TorusShape},
	node::NodeError,
	root::FrameInfo,
	spatial::{SpatialAspect, SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::lines::{line_from_points, LineExt};
use tokio::{sync::mpsc, task::JoinSet};
use zbus::Connection;

#[derive(Debug, Clone, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct FieldViz {
	transform: Transform,
	shape: Shape,
	grid_size: Vector3<usize>,
	sample_size: f32,
	normal_length: f32,
	line_thickness: f32,
	color: Color,
}

impl Default for FieldViz {
	fn default() -> Self {
		Self {
			transform: Transform::identity(),
			shape: Shape::Sphere(1.0),
			grid_size: [5, 5, 5].into(),
			sample_size: 0.5,
			normal_length: 0.1,
			line_thickness: 0.001,
			color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
		}
	}
}

pub struct FieldVizInner {
	field: Field,
	lines: stardust_xr_fusion::drawable::Lines,
	update_rx: mpsc::Receiver<Vec<Line>>,
	update_tx: mpsc::Sender<Vec<Line>>,
}

impl FieldVizInner {
	async fn update_normals(
		field: &Field,
		grid_size: Vector3<usize>,
		sample_size: f32,
		normal_length: f32,
		line_thickness: f32,
		color: Color,
	) -> Vec<Line> {
		let half_size = Vec3::new(
			grid_size.x as f32 - 1.0,
			grid_size.y as f32 - 1.0,
			grid_size.z as f32 - 1.0,
		) * sample_size
			* 0.5;

		let mut set = JoinSet::new();

		for x in 0..grid_size.x {
			for y in 0..grid_size.y {
				for z in 0..grid_size.z {
					let pos = Vec3::new(
						(x as f32 * sample_size) - half_size.x,
						(y as f32 * sample_size) - half_size.y,
						(z as f32 * sample_size) - half_size.z,
					);
					let field = field.clone();

					set.spawn(async move {
						const EPSILON: f32 = 0.0001;
						let (d, dx, dy, dz) = tokio::join!(
							field.distance(&field, pos),
							field.distance(&field, pos + Vec3::new(EPSILON, 0.0, 0.0)),
							field.distance(&field, pos + Vec3::new(0.0, EPSILON, 0.0)),
							field.distance(&field, pos + Vec3::new(0.0, 0.0, EPSILON)),
						);

						if let (Ok(d), Ok(dx), Ok(dy), Ok(dz)) = (d, dx, dy, dz) {
							let normal = Vec3::new(
								(dx - d) / EPSILON,
								(dy - d) / EPSILON,
								(dz - d) / EPSILON,
							)
							.normalize();

							let end = pos + (normal * normal_length);

							// Gradient color based on distance
							let t = (d * 20.0).clamp(-1.0, 1.0) * 0.5 + 0.5; // map [-0.05, 0.05] to [0, 1]
							let line_color = if t > 0.5 {
								// Outside: orange (at surface) to black
								let t = (t - 0.5) * 2.0; // remap [0.5, 1.0] to [0, 1]
								rgba_linear!(
									1.0 - t,         // red fades to black
									0.5 * (1.0 - t), // orange component fades to black
									0.0,
									1.0
								)
							} else {
								// Inside: orange (at surface) to white
								let t = t * 2.0; // remap [0, 0.5] to [0, 1]
								rgba_linear!(
									1.0,             // red stays at max
									0.5 + (0.5 * t), // green goes from orange to full
									t,               // blue comes up to full
									1.0
								)
							};

							Some(
								line_from_points(vec![
									[pos.x, pos.y, pos.z],
									[end.x, end.y, end.z],
								])
								.color(line_color)
								.thickness(line_thickness),
							)
						} else {
							None
						}
					});
				}
			}
		}

		let mut lines = Vec::new();
		while let Some(Ok(Some(line))) = set.join_next().await {
			lines.push(line);
		}

		lines
	}
}

impl<State: ValidState> ElementTrait<State> for FieldViz {
	type Inner = FieldVizInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_dbus_connection: &Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let field = Field::create(parent_space, Transform::identity(), self.shape.clone())?;
		let lines = stardust_xr_fusion::drawable::Lines::create(parent_space, self.transform, &[])?;
		field.set_spatial_parent(&lines)?;

		let (update_tx, update_rx) = mpsc::channel(1);

		// Initial update

		tokio::spawn({
			let field_clone = field.clone();
			let viz_config = self.clone();
			let update_tx = update_tx.clone();
			async move {
				let lines = FieldVizInner::update_normals(
					&field_clone,
					viz_config.grid_size,
					viz_config.sample_size,
					viz_config.normal_length,
					viz_config.line_thickness,
					viz_config.color,
				)
				.await;
				let _ = update_tx.send(lines).await;
			}
		});

		Ok(FieldVizInner {
			field,
			lines,
			update_rx,
			update_tx,
		})
	}

	fn update(
		&self,
		old: &Self,
		_state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if self.shape != old.shape {
			let _ = inner.field.set_shape(self.shape.clone());

			// Spawn new update task when shape changes
			let field = inner.field.clone();
			let update_tx = inner.update_tx.clone();
			let viz_config = self.clone();
			tokio::spawn(async move {
				let lines = FieldVizInner::update_normals(
					&field,
					viz_config.grid_size,
					viz_config.sample_size,
					viz_config.normal_length,
					viz_config.line_thickness,
					viz_config.color,
				)
				.await;
				let _ = update_tx.send(lines).await;
			});
		}

		// Handle any pending updates
		while let Ok(lines) = inner.update_rx.try_recv() {
			let _ = inner.lines.set_lines(&lines);
		}

		self.apply_transform(old, &inner.lines);
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.lines.clone().as_spatial().as_spatial_ref()
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
		client::{self, ClientState},
		elements::FieldViz,
		Element,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::fields::Shape;

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState(f32);
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const QUALIFIER: &'static str = "org";
		const ORGANIZATION: &'static str = "asteroids";
		const NAME: &'static str = "field_viz";

		fn on_frame(&mut self, info: &FrameInfo) {
			self.0 = info.elapsed;
		}

		fn reify(&self) -> Element<Self> {
			FieldViz::default()
				.shape(Shape::Torus(TorusShape {
					radius_a: 0.1,
					radius_b: 0.01,
				}))
				// .shape(Shape::Cylinder(CylinderShape {
				// 	length: 0.5,
				// 	radius: 0.025,
				// }))
				// .shape(Shape::Sphere((self.0 * 10.0).sin() * 0.5))
				.grid_size([11, 11, 11])
				.sample_size(0.025)
				// .normal_length(0.05)
				.normal_length(0.01)
				.build()
		}
	}

	client::run::<TestState>(&[]).await
}
