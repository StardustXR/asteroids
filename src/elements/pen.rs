use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper},
};
use derive_setters::Setters;
use glam::{Quat, Vec3};
use map_range::MapRange as _;
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, LinePoint, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, SpatialExt, Transform},
	suis::InputDataType,
	types::color::{AlphaColor, Rgb, color_space::LinearRgb, rgba_linear},
};
use stardust_xr_molecules::input_action::{InputQueue, SimpleAction, SingleAction};
use std::f32::consts::{FRAC_PI_2, FRAC_PI_4};

#[derive(Debug, Clone, Copy, Default)]
pub enum PenState {
	#[default]
	Floating,
	Grabbed,
	StartedDrawing(f32),
	Drawing(f32),
	StoppedDrawing,
}

pub struct PenInner {
	content_root: Spatial,
	pen_visuals: Lines,
	// the field straddles the shaft, so it lives on a spatial offset to the shaft's midpoint
	field_root: Spatial,
	field: Field,
	pointer_distance: f32,
	input: InputQueue,
	grab_action: SingleAction,
	draw_action: SimpleAction,
	drawing: bool,
}

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Pen<State: ValidState> {
	pub length: f32,
	pub thickness: f32,
	pub grab_distance: f32,
	pub hand_draw_threshold: f32,
	pub tip_draw_threshold: f32,
	pub color: AlphaColor<f32, Rgb<f32, LinearRgb>>,
	pub pos: Vector3<f32>,
	pub rot: Quaternion<f32>,
	#[expect(clippy::type_complexity)]
	#[setters(skip)]
	pub update:
		FnWrapper<dyn Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync>,
}
impl<State: ValidState> Pen<State> {
	pub fn new(
		pos: impl Into<Vector3<f32>>,
		rot: impl Into<Quaternion<f32>>,
		update: impl Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync + 'static,
	) -> Self {
		Pen {
			length: 0.075,
			thickness: 0.0025,
			grab_distance: 0.05,
			hand_draw_threshold: 0.75,
			tip_draw_threshold: 0.1,
			color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
			pos: pos.into(),
			rot: rot.into(),
			update: FnWrapper(Box::new(update)),
		}
	}

	fn get_lines(&self) -> Line {
		Line {
			points: vec![
				LinePoint {
					point: [0.0; 3].into(),
					thickness: 0.0,
					color: self.color,
				},
				LinePoint {
					point: [0.0, self.thickness, 0.0].into(),
					thickness: self.thickness,
					color: self.color,
				},
				LinePoint {
					point: [0.0, self.length, 0.0].into(),
					thickness: self.thickness,
					color: self.color,
				},
			],
			cyclic: false,
		}
	}
}
impl<State: ValidState> CustomElement<State> for Pen<State> {
	type Inner = PenInner;

	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let content_root = info.child_space;
		content_root
			.set_local_transform(Transform::from_translation_rotation(self.pos, self.rot)).await?;

		let pen_visuals = Lines::new(
			&context.stardust_client,
			&content_root,
			vec![self.get_lines()],
		)
		.await?;

		// the cylinder field is centered on its spatial, so offset it up to the shaft midpoint
		let content_root_ref = content_root.spatial_ref().await?;
		let (field_root, _field_root_ref) = Spatial::new(
			&context.stardust_client,
			&content_root_ref,
			Transform::from_translation([0.0, self.length * 0.5, 0.0]),
		)
		.await?;
		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&field_root,
			Shape::Cylinder {
				length: self.length,
				radius: self.thickness,
			},
		)
		.await?;

		let input = InputQueue::new(
			&context.stardust_client,
			field_root.clone(),
			field.clone(),
			// input is reported relative to the *stationary* parent space, not the moving
			// content_root, so the grab pose doesn't fight itself as the pen follows
			info.parent_space.clone(),
		)
		.await?;

		Ok(PenInner {
			content_root,
			pen_visuals,
			field_root,
			field,
			input,
			pointer_distance: 0.0,
			grab_action: Default::default(),
			draw_action: Default::default(),
			drawing: false,
		})
	}

	fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.pos != old.pos || self.rot != old.rot {
			let _ = inner
				.content_root
				.set_local_transform(Transform::from_translation_rotation(self.pos, self.rot));
		}

		if self.thickness != old.thickness || self.length != old.length || self.color != old.color {
			let _ = inner.pen_visuals.set_lines(vec![self.get_lines()]);
			let _ = inner.field.set_shape(Shape::Cylinder {
				length: self.length,
				radius: self.thickness,
			});
			let _ = inner
				.field_root
				.set_local_transform(Transform::from_translation([0.0, self.length * 0.5, 0.0]));
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if !inner.input.handle_events() {
			return;
		}

		inner.grab_action.update(
			false,
			&inner.input,
			|i| i.distance() < self.grab_distance,
			|i| match i.input() {
				InputDataType::Hand { data: h } => {
					(h.finger_curl(&h.ring) + h.finger_curl(&h.little)) / 2.0 > 0.75
				}
				_ => i.datamap_f32("grab") > 0.90,
			},
		);

		inner
			.draw_action
			.update(&inner.input, &|i| match i.input() {
				InputDataType::Hand { data: h } => h.pinch_strength() > self.hand_draw_threshold,
				_ => i.datamap_f32("select") > self.tip_draw_threshold,
			});

		let Some(actor) = inner.grab_action.actor().cloned() else {
			if inner.grab_action.actor_stopped() {
				(self.update.0)(state, PenState::Floating, self.pos, self.rot);
			}
			return;
		};

		if let InputDataType::Pointer { data: p } = actor.input() {
			if inner.grab_action.actor_started() {
				// deepest_point is now a distance along the ray
				inner.pointer_distance = p.deepest_point;
			} else {
				inner.pointer_distance += (actor.datamap_vec2("scroll_continuous").y * 0.01) + // continuous +Y -> 1cm farther away
					(actor.datamap_vec2("scroll_discrete").y * 0.1); // discrete +Y -> 10cm farther away
			}
		}

		let (pos, rot) = match actor.input() {
			InputDataType::Hand { data: h } => (
				Vec3::from(h.predicted_pinch_position()),
				Quat::from(h.palm.pose.orientation),
			),
			InputDataType::Tip { data: t } => (
				Vec3::from(t.pose.position),
				Quat::from(t.pose.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			InputDataType::Pointer { data: p } => {
				// Calculate position at current distance along pointer ray
				let origin = Vec3::from(p.pose.position);
				let orientation = Quat::from(p.pose.orientation);
				let direction = Vec3::from(p.direction()).normalize();
				(
					origin + (direction * inner.pointer_distance),
					orientation * Quat::from_rotation_z(-FRAC_PI_4),
				)
			}
		};

		let pen_state = if !inner.grab_action.actor_acting() {
			PenState::Floating
		} else if !inner.draw_action.currently_acting().is_empty() {
			let pressure = match actor.input() {
				InputDataType::Hand { data: h } => h
					.pinch_strength()
					.map_range(self.hand_draw_threshold..1.0, 0.0..1.0),
				_ => actor
					.datamap_f32("select")
					.map_range(self.tip_draw_threshold..1.0, 0.0..1.0),
			};
			if !inner.drawing {
				inner.drawing = true;
				PenState::StartedDrawing(pressure)
			} else {
				PenState::Drawing(pressure)
			}
		} else if inner.drawing {
			inner.drawing = false;
			PenState::StoppedDrawing
		} else {
			PenState::Grabbed
		};

		(self.update.0)(state, pen_state, pos.into(), rot.into());
	}
}

#[tokio::test]
async fn asteroids_pen_test() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		custom::CustomElement,
		elements::{Axes, Lines, Pen, line_from_points},
	};
	use mint::{Quaternion, Vector3};
	use serde::{Deserialize, Serialize};
	use stardust_xr_molecules::lines::LineExt;

	#[derive(Serialize, Deserialize)]
	struct TestState {
		#[serde(skip)]
		pen_state: PenState,
		pos: Vector3<f32>,
		rot: Quaternion<f32>,
	}
	impl Default for TestState {
		fn default() -> Self {
			Self {
				pen_state: PenState::Floating,
				pos: [0.1; 3].into(),
				rot: Quat::from_rotation_z(FRAC_PI_4).into(),
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.pen";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			Axes::default()
				.build()
				.child(
					Lines::new([
						line_from_points(vec![[0.0; 3].into(), self.pos]).thickness(0.0025)
					])
					.build(),
				)
				.child(
					Pen::new(
						self.pos,
						self.rot,
						|state: &mut TestState, pen_state, pos, rot| {
							state.pen_state = dbg!(pen_state);
							state.pos = pos;
							state.rot = rot;
						},
					)
					.color(match &self.pen_state {
						PenState::Floating => {
							rgba_linear!(1.0, 1.0, 1.0, 1.0)
						}
						PenState::Grabbed => {
							rgba_linear!(0.1, 0.1, 1.0, 1.0)
						}
						PenState::StartedDrawing(p) => {
							rgba_linear!(0.1 * p, 1.0 * p, 0.1 * p, 1.0)
						}
						PenState::Drawing(p) => {
							rgba_linear!(1.0 * p, 1.0 * p, 0.1 * p, 1.0)
						}
						PenState::StoppedDrawing => {
							rgba_linear!(1.0, 0.1, 0.1, 1.0)
						}
					})
					.length(0.1)
					.thickness(0.01)
					.build(),
				)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
