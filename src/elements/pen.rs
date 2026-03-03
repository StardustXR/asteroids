use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper},
};
use derive_setters::Setters;
use glam::{Quat, Vec3};
use map_range::MapRange as _;
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputDataType, InputHandler},
	node::NodeError,
	spatial::{Spatial, SpatialAspect, SpatialRef, Transform},
	values::color::{AlphaColor, Rgb, color_space::LinearRgb, rgba_linear},
};
use stardust_xr_molecules::input_action::{
	InputQueue, InputQueueable as _, SimpleAction, SingleAction,
};
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
	child_root: Spatial,
	pen_visuals_root: Lines,
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
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let pen_visuals_root = Lines::create(
			info.parent_space,
			Transform::from_translation_rotation(self.pos, self.rot),
			&[self.get_lines()],
		)?;
		let field = Field::create(
			&pen_visuals_root,
			Transform::from_translation([0.0, self.length * 0.5, 0.0]),
			Shape::Cylinder(CylinderShape {
				length: self.length,
				radius: self.thickness,
			}),
		)?;
		let queue = InputHandler::create(info.parent_space, Transform::none(), &field)?.queue()?;

		let child_root = Spatial::create(
			&pen_visuals_root,
			Transform::from_translation(Vec3::new(0., self.length, 0.)),
		)?;

		pen_visuals_root.set_spatial_parent(queue.handler())?;

		Ok(PenInner {
			field,
			pen_visuals_root,
			input: queue,
			pointer_distance: 0.0,
			grab_action: Default::default(),
			draw_action: Default::default(),
			child_root,
			drawing: false,
		})
	}

	fn diff(&self, old: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.pos != old.pos || self.rot != old.rot {
			let transform = Transform::from_translation_rotation(self.pos, self.rot);
			let _ = inner
				.pen_visuals_root
				.set_relative_transform(inner.input.handler(), transform);
		}

		if self.thickness != old.thickness || self.length != old.length || self.color != old.color {
			_ = inner.pen_visuals_root.set_lines(&[self.get_lines()]);
			_ = inner.field.set_shape(Shape::Cylinder(CylinderShape {
				length: self.length,
				radius: self.thickness,
			}));
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::root::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if !inner.input.handle_events() {
			return;
		}

		inner.grab_action.update(
			false,
			&inner.input,
			|data| data.distance < self.grab_distance,
			|data| match &data.input {
				InputDataType::Hand(h) => {
					(h.finger_curl(&h.ring) + h.finger_curl(&h.little)) / 2.0 > 0.75
				}
				_ => data
					.datamap
					.with_data(|datamap| datamap.idx("grab").as_f32() > 0.90),
			},
		);

		inner
			.draw_action
			.update(&inner.input, &|data| match &data.input {
				InputDataType::Hand(h) => h.pinch_strength() > self.hand_draw_threshold,
				_ => data
					.datamap
					.with_data(|datamap| datamap.idx("select").as_f32() > self.tip_draw_threshold),
			});

		let Some(actor) = inner.grab_action.actor() else {
			if inner.grab_action.actor_stopped() {
				(self.update.0)(state, PenState::Floating, self.pos, self.rot);
			}
			return;
		};

		if let InputDataType::Pointer(p) = &actor.input {
			if inner.grab_action.actor_started() {
				inner.pointer_distance = Vec3::from(p.origin).distance(p.deepest_point.into());
			} else {
				inner.pointer_distance += actor.datamap.with_data(|d| {
					(-d.idx("scroll_continuous").as_vector().idx(1).as_f32() * 0.01) + // continuous +Y -> 1cm farther away
					(-d.idx("scroll_discrete").as_vector().idx(1).as_f32() * 0.1) // discrete +Y -> 10cm farther away
				});
			}
		}

		let (pos, rot) = match &actor.input {
			InputDataType::Hand(h) => (
				h.predicted_pinch_position().into(),
				Quat::from(h.palm.rotation),
			),
			InputDataType::Tip(t) => (
				t.origin.into(),
				Quat::from(t.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			InputDataType::Pointer(p) => {
				// Calculate position at current distance along pointer ray
				let origin = Vec3::from(p.origin);
				let orientation = Quat::from(p.orientation);
				let direction = Vec3::from(p.direction()).normalize();
				(
					(origin + (direction * inner.pointer_distance)),
					orientation * Quat::from_rotation_z(-FRAC_PI_4),
				)
			}
		};

		let pen_state = if !inner.grab_action.actor_acting() {
			PenState::Floating
		} else if !inner.draw_action.currently_acting().is_empty() {
			let pressure = actor.datamap.with_data(|datamap| match &actor.input {
				InputDataType::Hand(h) => h
					.pinch_strength()
					.map_range(self.hand_draw_threshold..1.0, 0.0..1.0),
				_ => datamap
					.idx("select")
					.as_f32()
					.map_range(self.tip_draw_threshold..1.0, 0.0..1.0),
			});
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

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.child_root.clone().as_spatial_ref()
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

	client::run::<TestState>(&[]).await;
}
