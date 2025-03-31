use std::f32::consts::FRAC_PI_2;

use crate::{
	custom::{ElementTrait, FnWrapper},
	ValidState,
};
use derive_setters::Setters;
use glam::{Quat, Vec3};
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputData, InputDataType, InputHandler},
	node::NodeError,
	spatial::{Spatial, SpatialAspect, Transform},
	values::color::{color_space::LinearRgb, rgba_linear, AlphaColor, Rgb},
};
use stardust_xr_molecules::input_action::{InputQueue, InputQueueable as _, SingleAction};

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Pen<State: ValidState> {
	pub length: f32,
	pub thickness: f32,
	pub grab_distance: f32,
	pub color: AlphaColor<f32, Rgb<f32, LinearRgb>>,
	pub pos: Vector3<f32>,
	pub rot: Quaternion<f32>,
	#[expect(clippy::type_complexity)]
	pub update:
		FnWrapper<dyn Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync>,
	pub drawing_value: FnWrapper<dyn Fn(&InputData) -> f32 + Send + Sync>,
	pub should_draw: FnWrapper<dyn Fn(&InputData) -> bool + Send + Sync>,
}

#[derive(Debug)]
pub enum PenState {
	Grabbed,
	StartedDrawing(f32),
	Drawing(f32),
	StoppedDrawing,
}

impl<State: ValidState> ElementTrait<State> for Pen<State> {
	type Inner = PenInner;

	type Resource = ();

	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &stardust_xr_fusion::spatial::SpatialRef,
		_dbus_connection: &stardust_xr_fusion::core::schemas::zbus::Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let pen_root = Spatial::create(parent_space, Transform::none(), true)?;
		let field = Field::create(
			&pen_root,
			Transform::from_translation([0.0, 0.0, self.length * 0.5]),
			Shape::Cylinder(CylinderShape {
				length: self.length,
				radius: self.thickness * 0.5,
			}),
		)?;
		let queue = InputHandler::create(parent_space, Transform::none(), &field)?.queue()?;
		let visuals = Lines::create(&pen_root, Transform::none(), &[self.get_lines()])?;

		let child_root = Spatial::create(
			&pen_root,
			Transform::from_translation(Vec3::new(0., self.length, 0.)),
			false,
		)?;

		Ok(PenInner {
			field,
			pen_root,
			queue,
			grab_action: Default::default(),
			visuals,
			child_root,
			drawing: false,
		})
	}

	fn update(
		&self,
		old_decl: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		self.react_to_changes(old_decl, inner);
		if !inner.queue.handle_events() {
			return;
		}
		inner.grab_action.update(
			false,
			&inner.queue,
			// should this even be exposed?
			|data| {
				data.distance < self.grab_distance
			},
			|data| {
				data.datamap.with_data(|datamap| match &data.input {
					// should we also expose these values?
					InputDataType::Hand(_) => datamap.idx("grab_strength").as_f32() > 0.70,
					InputDataType::Tip(_) => datamap.idx("grab").as_f32() > 0.90,
					_ => false,
				})
			},
		);

		if inner.grab_action.actor_started() {
			let _ = inner.pen_root.set_zoneable(false);
		}
		if inner.grab_action.actor_stopped() {
			let _ = inner.pen_root.set_zoneable(true);
		}
		let Some(actor) = inner.grab_action.actor() else {
			if inner.drawing {
				inner.drawing = false;
				self.update.0(state, PenState::StoppedDrawing, self.pos, self.rot);
			}
			return;
		};
		let (pos, rot) = match &actor.input {
			InputDataType::Hand(h) => (
				(Vec3::from(h.thumb.tip.position) + Vec3::from(h.index.tip.position)) * 0.5,
				Quat::from(h.palm.rotation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			InputDataType::Tip(t) => (
				t.origin.into(),
				Quat::from(t.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			_ => (Vec3::ZERO, Quat::IDENTITY),
		};
		let transform = Transform::from_translation_rotation(pos, rot);
		let _ = inner
			.pen_root
			.set_relative_transform(inner.queue.handler(), transform);
		let mut pen_state = PenState::Grabbed;
		let drawing = self.should_draw.0(actor);
		let started_drawing = drawing && !inner.drawing;
		let stopped_drawing = inner.drawing && !drawing;
		inner.drawing = drawing;

		if inner.drawing && !started_drawing {
			pen_state = PenState::Drawing(self.drawing_value.0(actor));
		}
		if started_drawing {
			pen_state = PenState::StartedDrawing(self.drawing_value.0(actor));
		}
		if stopped_drawing {
			pen_state = PenState::StoppedDrawing;
		}

		self.update.0(state, pen_state, pos.into(), rot.into());
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> stardust_xr_fusion::spatial::SpatialRef {
		inner.child_root.clone().as_spatial_ref()
	}
}

impl<State: ValidState> Pen<State> {
	pub fn new(
		pos: impl Into<Vector3<f32>>,
		rot: impl Into<Quaternion<f32>>,
		update: impl Fn(&mut State, PenState, Vector3<f32>, Quaternion<f32>) + Send + Sync + 'static,
	) -> Self {
		Pen {
			length: 0.075,
			thickness: 0.005,
			grab_distance: 0.05,
			color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
			pos: pos.into(),
			rot: rot.into(),
			update: FnWrapper(Box::new(update)),
			drawing_value: FnWrapper(Box::new(|data| {
				data.datamap.with_data(|datamap| match &data.input {
					InputDataType::Hand(_) => datamap.idx("pinch_strength").as_f32(),
					InputDataType::Tip(_) => datamap.idx("select").as_f32(),
					_ => unimplemented!(),
				})
			})),
			should_draw: FnWrapper(Box::new(|data| {
				data.datamap.with_data(|datamap| match &data.input {
					InputDataType::Hand(_) => datamap.idx("pinch_strength").as_f32() > 0.3,
					InputDataType::Tip(_) => datamap.idx("select").as_f32() > 0.01,
					_ => false,
				})
			})),
		}
	}
}

impl<State: ValidState> Pen<State> {
	fn react_to_changes(&self, old_decl: &Self, inner: &mut PenInner) {
		if self.thickness != old_decl.thickness || self.length != old_decl.length {
			_ = inner.visuals.set_lines(&[self.get_lines()]);
			_ = inner.field.set_shape(Shape::Cylinder(CylinderShape {
				length: self.length,
				radius: self.thickness * 0.5,
			}));
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

pub struct PenInner {
	child_root: Spatial,
	field: Field,
	pen_root: Spatial,
	queue: InputQueue,
	grab_action: SingleAction,
	visuals: Lines,
	drawing: bool,
}
