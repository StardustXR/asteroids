use std::f32::consts::{FRAC_PI_2, PI};

use crate::{
	custom::{ElementTrait, FnWrapper},
	ValidState,
};
use glam::{Quat, Vec3};
use stardust_xr_fusion::{
	drawable::{Line, LinePoint, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputData, InputDataType, InputHandler},
	node::NodeError,
	spatial::{Spatial, SpatialAspect, Transform},
	values::color::{color_space::LinearRgb, AlphaColor, Rgb},
};
use stardust_xr_molecules::input_action::{
	InputQueue, InputQueueable as _, SimpleAction, SingleAction,
};

#[derive_where::derive_where(Debug)]
pub struct Pen<State: ValidState> {
	/// only send move updates after the pen has moved a distance greater than this since the last
	/// move update
	pub move_resolution: f32,
	pub length: f32,
	pub thickness: f32,
	pub grab_distance: f32,
	pub color: AlphaColor<f32, Rgb<f32, LinearRgb>>,
	pub child_anchor: PenChildAnchor,
	pub should_interact: FnWrapper<dyn Fn(&InputData) -> bool + Send + Sync>,
	pub on_interact_start: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
	#[expect(clippy::type_complexity)]
	pub on_interact: FnWrapper<dyn Fn(&mut State, Vec3, &InputData) + Send + Sync>,
	pub on_interact_stop: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PenChildAnchor {
	Tip,
	Top,
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
			match self.child_anchor {
				PenChildAnchor::Tip => Transform::from_rotation(Quat::from_rotation_x(PI)),
				PenChildAnchor::Top => Transform::from_translation(Vec3::new(0., self.length, 0.)),
			},
			false,
		)?;

		Ok(PenInner {
			field,
			pen_root,
			queue,
			grab_action: Default::default(),
			interact_action: Default::default(),
			visuals,
			child_root,
			last_update_position: Vec3::ZERO,
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
		let last_actor = inner.grab_action.actor().cloned();
		inner.grab_action.update(
			false,
			&inner.queue,
			// should this even be exposed?
			|data| data.distance < self.grab_distance,
			|data| {
				data.datamap.with_data(|datamap| match &data.input {
					// should we also expose these values?
					InputDataType::Hand(_) => datamap.idx("grab_strength").as_f32() > 0.70,
					InputDataType::Tip(_) => datamap.idx("grab").as_f32() > 0.90,
					_ => false,
				})
			},
		);
		inner
			.interact_action
			.update(&inner.queue, &self.should_interact.0);

		if inner.grab_action.actor_started() {
			let _ = inner.pen_root.set_zoneable(false);
		}
		if inner.grab_action.actor_stopped() {
			let _ = inner.pen_root.set_zoneable(true);
		}
		if let Some(actor) = inner.grab_action.actor().or(last_actor.as_ref()) {
			if inner.interact_action.stopped_acting().contains(actor) {
				self.on_interact_stop.0(state)
			}
		}
		let Some(actor) = inner.grab_action.actor() else {
			return;
		};
		if inner.interact_action.started_acting().contains(actor) {
			self.on_interact_start.0(state)
		}
		let transform = match &actor.input {
			InputDataType::Hand(h) => Transform::from_translation_rotation(
				(Vec3::from(h.thumb.tip.position) + Vec3::from(h.index.tip.position)) * 0.5,
				Quat::from(h.palm.rotation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			InputDataType::Tip(t) => Transform::from_translation_rotation(
				t.origin,
				Quat::from(t.orientation) * Quat::from_rotation_x(FRAC_PI_2),
			),
			_ => Transform::none(),
		};
		let _ = inner
			.pen_root
			.set_relative_transform(inner.queue.handler(), transform);
		if inner.interact_action.currently_acting().contains(actor) {
			let point = match &actor.input {
				InputDataType::Hand(h) => {
					(Vec3::from(h.thumb.tip.position) + Vec3::from(h.index.tip.position)) * 0.5
				}

				InputDataType::Tip(t) => Vec3::from(t.origin),
				_ => unreachable!(),
			};
			if point.distance(inner.last_update_position) >= self.move_resolution {
				inner.last_update_position = point;
				self.on_interact.0(state, point, actor);
			}
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> stardust_xr_fusion::spatial::SpatialRef {
		inner.child_root.clone().as_spatial_ref()
	}
}

impl<State: ValidState> Pen<State> {
	fn react_to_changes(&self, old_decl: &Self, inner: &mut PenInner) {
		if self.child_anchor != old_decl.child_anchor || self.length != old_decl.length {
			_ = inner
				.child_root
				.set_local_transform(match self.child_anchor {
					PenChildAnchor::Tip => Transform::from_rotation(Quat::from_rotation_x(PI)),
					PenChildAnchor::Top => {
						Transform::from_translation(Vec3::new(0., self.length, 0.))
					}
				});
		}
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
	interact_action: SimpleAction,
	visuals: Lines,
	last_update_position: Vec3,
}
