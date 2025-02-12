use crate::{
	custom::{ElementTrait, FnWrapper, Transformable},
	ValidState,
};
use derive_setters::Setters;
use derive_where::derive_where;
use glam::{vec3, Mat4, Vec2, Vec3, Vec3Swizzles};
use stardust_xr_fusion::{
	core::values::Color,
	drawable::Lines,
	fields::{CylinderShape, Field, Shape},
	input::{InputDataType, InputHandler},
	node::{NodeError, NodeResult},
	spatial::{SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputQueueable, SingleAction},
	lines::{circle, LineExt},
};
use std::ops::Range;
use zbus::Connection;

#[derive_where::derive_where(Debug, PartialEq)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Knob<State: ValidState> {
	transform: Transform,
	on_change: FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>,
	current_value: f32,
	/// the knob's radius itself, going outside this will trigger a turn
	radius: f32,
	/// how thick should the knob be?
	thickness: f32,
	/// how much is 1 turn in units?
	turn_unit_amount: f32,
	/// the limits of the knob. what's its max and min?
	range: Range<f32>,
	/// what amount of divisions should the knob snap to? first one is innermost, all others go outward
	precisions: Vec<usize>,
	/// what range should a segment's arc length be? determines the radius for precisions
	segment_length_range: Range<f32>,
	accent_color: Color,
}
impl<State: ValidState> Knob<State> {
	pub fn create(
		current_value: f32,
		on_change: impl Fn(&mut State, f32) + Send + Sync + 'static,
	) -> Knob<State> {
		Knob {
			transform: Transform::none(),
			current_value,
			on_change: FnWrapper(Box::new(on_change)),
			range: 0.0..1.0,
			radius: 0.015,
			thickness: 0.075,
			turn_unit_amount: 1.0,
			precisions: Vec::new(),
			segment_length_range: 0.01..0.02,
			accent_color: rgba_linear!(0.0, 1.0, 0.75, 1.0),
		}
	}
}

impl<State: ValidState> ElementTrait<State> for Knob<State> {
	// You'll need to create this type in stardust_xr_molecules
	type Inner = KnobInner;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		parent_space: &SpatialRef,
		_dbus_connection: &Connection,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		KnobInner::create(
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
		let new_value = inner.update(
			self.current_value,
			self.turn_unit_amount,
			&self.range,
			&self.precisions,
			&self.segment_length_range,
			self.accent_color,
		);
		if new_value != self.current_value {
			(self.on_change.0)(state, new_value);
		}
		self.apply_transform(old, inner.input.handler());
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		inner.input.handler().clone().as_spatial().as_spatial_ref()
	}
}
impl<State: ValidState> Transformable for Knob<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
pub struct KnobInner {
	lines: Lines,
	input: InputQueue,
	single_action: SingleAction,
	field: Field,
	start_value: Option<f32>,
}
impl KnobInner {
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

		let lines = Lines::create(
			parent,
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
			start_value: None,
		})
	}

	pub fn update(
		&mut self,
		current_value: f32,
		turn_unit_amount: f32,
		range: &Range<f32>,
		precisions: &[usize],
		segment_length_range: &Range<f32>,
		accent_color: Color,
	) -> f32 {
		self.input.handle_events();
		self.single_action.update(
			false,
			&self.input,
			|data| data.distance >= 0.0,
			|data| match &data.input {
				InputDataType::Hand(hand) => data
					.datamap
					.with_data(|d| d.idx("pinch_strength").as_f32() > 0.5),
				_ => data.datamap.with_data(|d| d.idx("select").as_f32() > 0.5),
			},
		);
		let Some(actor) = self.single_action.actor() else {
			return current_value;
		};
		let interact_point: Vec2 = match &actor.input {
			InputDataType::Pointer(pointer) => {
				let origin: Vec3 = pointer.origin.into();
				let direction: Vec3 = pointer.direction().into();

				// Line-plane intersection with XZ plane (y=0)
				// ray-plane intersection: origin + t*direction = point where y=0
				// Solve for t: origin.y + t*direction.y = 0
				// t = -origin.y / direction.y
				// yes this is llm output i am lazy and didn't want other crate deps
				let t = -origin.y / direction.y;
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
		current_value
	}
}

#[tokio::test]
async fn asteroids_knob_element() {
	use crate::{
		client::{self, ClientState},
		elements::Knob,
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
			Knob::create(self.value, |state: &mut TestState, value| {
				state.value = value;
			})
			.radius(0.025)
			.current_value(self.value)
			.build()
		}
	}

	client::run(TestState::default, &[]).await
}
