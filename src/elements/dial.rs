use crate::{
	custom::{ElementTrait, FnWrapper, Transformable},
	ValidState,
};
use derive_setters::Setters;
use derive_where::derive_where;
use glam::{vec3, Mat4, Vec2, Vec3, Vec3Swizzles};
use stardust_xr_fusion::{
	core::values::Color,
	drawable::{Line, Lines, LinesAspect},
	fields::{CylinderShape, Field, FieldAspect, Shape},
	input::{InputDataType, InputHandler},
	node::{NodeError, NodeResult},
	spatial::{SpatialAspect, SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputQueueable, SingleAction},
	lines::{circle, line_from_points, LineExt},
};
use std::{f32::consts::TAU, ops::Range};
use zbus::Connection;

pub type OnChangeFn<State> = FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Dial<State: ValidState> {
	transform: Transform,
	/// When the knob changes its value
	on_change: OnChangeFn<State>,
	/// Current value, need to store the value from `on_change` then give it back next time here
	current_value: f32,
	/// the dial's radius itself, going outside this will trigger a turn
	radius: f32,
	/// how thick should the dial be?
	thickness: f32,
	/// how much is 1 turn in units?
	turn_unit_amount: f32,
	/// the limits of the dial. what's its max and min?
	range: Range<f32>,
	/// what amount of divisions should the dial snap to? first one is innermost, all others go outward
	precisions: Vec<usize>,
	/// what range should a segment's arc length be? determines the radius for precisions
	segment_length_range: Range<f32>,
	accent_color: Color,
}
impl<State: ValidState> Dial<State> {
	pub fn create(
		current_value: f32,
		on_change: impl Fn(&mut State, f32) + Send + Sync + 'static,
	) -> Dial<State> {
		Dial {
			transform: Transform::none(),
			current_value,
			on_change: FnWrapper(Box::new(on_change)),
			range: f32::NEG_INFINITY..f32::INFINITY,
			radius: 0.015,
			thickness: 0.075,
			turn_unit_amount: 1.0,
			precisions: Vec::new(),
			segment_length_range: 0.01..0.02,
			accent_color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
		}
	}
}

impl<State: ValidState> ElementTrait<State> for Dial<State> {
	// You'll need to create this type in stardust_xr_molecules
	type Inner = DialInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_dbus_connection: &Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		DialInner::create(
			parent_space,
			*self.transform(),
			self.radius,
			self.thickness,
			self.accent_color,
		)
	}

	fn update(
		&self,
		old: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if self.radius != old.radius || self.thickness != old.thickness {
			let _ = inner.field.set_shape(Shape::Cylinder(CylinderShape {
				radius: self.radius,
				length: self.thickness,
			}));
		}
		let new_value = inner.update(self);
		if new_value != self.current_value {
			(self.on_change.0)(state, new_value);
		}
		self.apply_transform(old, inner.input.handler());
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.input.handler().clone().as_spatial().as_spatial_ref()
	}
}
impl<State: ValidState> Transformable for Dial<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
pub struct DialInner {
	lines: Lines,
	input: InputQueue,
	single_action: SingleAction,
	field: Field,
	last_vector: Option<Vec2>,
}
impl DialInner {
	pub fn create(
		parent: &SpatialRef,
		transform: Transform,
		radius: f32,
		thickness: f32,
		accent_color: Color,
	) -> NodeResult<Self> {
		let field = Field::create(
			parent,
			transform,
			Shape::Cylinder(CylinderShape {
				radius,
				length: thickness,
			}),
		)?;
		let input = InputHandler::create(parent, transform, &field)?.queue()?;
		let _ = field.set_spatial_parent(input.handler());

		let lines = Lines::create(
			input.handler(),
			transform,
			&[
				// circles are z-facing
				circle(32, 0.0, radius).color(accent_color),
				circle(32, 0.0, radius)
					.color(accent_color)
					.transform(Mat4::from_translation(vec3(0.0, 0.0, thickness))),
			],
		)?;

		Ok(Self {
			lines,
			input,
			single_action: SingleAction::default(),
			field,
			last_vector: None,
		})
	}

	pub fn update<State: ValidState>(&mut self, decl: &Dial<State>) -> f32 {
		if !self.input.handle_events() {
			return decl.current_value;
		}
		self.single_action.update(
			false,
			&self.input,
			|data| data.distance < 0.0,
			|data| match &data.input {
				InputDataType::Hand(_) => data
					.datamap
					.with_data(|d| d.idx("pinch_strength").as_f32() > 0.5),
				_ => data.datamap.with_data(|d| d.idx("select").as_f32() > 0.5),
			},
		);
		// remove the start value when we stop pinching or such
		if self.single_action.actor_stopped() {
			self.last_vector.take();
		}
		let Some(actor) = self.single_action.actor() else {
			let _ = self
				.lines
				.set_lines(&self.signifier_lines::<State>(None, decl));
			return decl.current_value;
		};
		if actor.distance <= 0.0 {
			self.last_vector.take();
		}

		// We need the 2D projected/intersected interaction point
		let interact_point: Vec2 = match &actor.input {
			InputDataType::Pointer(pointer) => {
				let origin: Vec3 = pointer.origin.into();
				let direction: Vec3 = pointer.direction().into();

				// Line-plane intersection with XY plane (z=0)
				// ray-plane intersection: origin + t*direction = point where z=0
				// Solve for t: origin.z + t*direction.z = 0
				// t = -origin.z / direction.z
				// yes i used an llm i am lazy but it works so whatever
				let t = -origin.z / direction.z;
				let result = origin + direction * t;
				result.xy()
			}
			InputDataType::Hand(hand) => {
				let index_tip_pos: Vec3 = hand.index.tip.position.into();
				let thumb_tip_pos: Vec3 = hand.thumb.tip.position.into();
				let pinch_point = (index_tip_pos + thumb_tip_pos) / 2.0;
				pinch_point.xy()
			}
			InputDataType::Tip(tip) => Vec3::from(tip.origin).xy(),
		};

		let new_value = if let Some(last_vector) = &mut self.last_vector {
			// using delta vector since then as long as someone doesn't do more than half a turn in a frame it'll work
			let delta_rad = interact_point.angle_to(*last_vector);
			// technically not the most efficient to use turns but like we need good UX
			let delta_turns = delta_rad / TAU;
			let delta = delta_turns * decl.turn_unit_amount;
			let new_value = decl.current_value + delta;

			self.last_vector.replace(interact_point);
			new_value.clamp(decl.range.start, decl.range.end)
		} else {
			if actor.distance > 0.0 {
				self.last_vector.replace(interact_point);
			}
			decl.current_value
		};

		let _ = self
			.lines
			.set_lines(&self.signifier_lines::<State>(Some(interact_point), decl));
		new_value
	}

	fn signifier_lines<State: ValidState>(
		&self,
		interact_point: Option<Vec2>,
		decl: &Dial<State>,
	) -> Vec<Line> {
		let color = if interact_point.is_some() {
			decl.accent_color
		} else {
			rgba_linear!(1.0, 1.0, 1.0, 1.0)
		};
		let mut lines = vec![
			// circles are z-facing
			circle(32, 0.0, decl.radius).color(color).thickness(0.001),
			circle(32, 0.0, decl.radius)
				.color(color)
				.thickness(0.001)
				.transform(Mat4::from_translation(vec3(0.0, 0.0, decl.thickness))),
		];

		if let Some(interact_point) = interact_point {
			let normalized_start = interact_point.normalize() * decl.radius;
			lines.push(
				line_from_points(vec![
					[normalized_start.x, normalized_start.y, 0.0],
					[interact_point.x, interact_point.y, 0.0],
				])
				.thickness(0.001),
			);
		}

		lines
	}
}

#[tokio::test]
async fn asteroids_dial_element() {
	use crate::{
		client::{self, ClientState},
		elements::Dial,
		Element,
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::root::FrameInfo;

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState {
		value: f32,
	}
	impl ClientState for TestState {
		fn on_frame(&mut self, _info: &FrameInfo) {}
		fn reify(&self) -> Element<Self> {
			let hours = (self.value / 60.0).floor();
			let minutes = (self.value % 60.0).floor();
			let period = if hours >= 12.0 { "PM" } else { "AM" };
			let formatted_hours = if hours == 0.0 {
				12.0
			} else if hours > 12.0 {
				hours - 12.0
			} else {
				hours
			};
			crate::elements::Spatial::default().with_children([
				Dial::create(self.value, |state: &mut TestState, value| {
					state.value = value;
				})
				.radius(0.02)
				.thickness(0.01)
				.current_value(self.value)
				.turn_unit_amount(12.0 * 60.0)
				.range(0.0..(24.0 * 60.0))
				.build(),
				Dial::create(self.value, |state: &mut TestState, value| {
					state.value = value;
				})
				.radius(0.025)
				.thickness(0.005)
				.current_value(self.value)
				.turn_unit_amount(60.0)
				.range(0.0..(24.0 * 60.0))
				.build(),
				crate::elements::Text::default()
					.text(format!(
						"{:02.0}:{:02.0} {}",
						formatted_hours, minutes, period
					))
					.character_height(0.005)
					.pos([0.0, 0.0, 0.01])
					.rot(glam::Quat::from_rotation_y(std::f32::consts::PI))
					.build(),
			])
		}
	}

	client::run(TestState::default, &[]).await
}
