use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable},
};
use derive_setters::Setters;
use glam::{Mat4, Vec2, Vec3, Vec3Swizzles, vec3};
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, Transform},
	suis::InputDataType,
	types::{Color, Vec3F, rgba_linear},
};
use stardust_xr_molecules::{
	input_action::{InputQueue, SingleAction},
	lines::{LineExt, circle, line_from_points},
};
use std::{
	f32::consts::{FRAC_PI_2, TAU},
	ops::Range,
};

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
}
impl<State: ValidState> Dial<State> {
	pub fn create(
		current_value: f32,
		on_change: impl Fn(&mut State, f32) + Send + Sync + 'static,
	) -> Dial<State> {
		Dial {
			transform: Transform::IDENTITY,
			current_value,
			on_change: FnWrapper(Box::new(on_change)),
			range: f32::NEG_INFINITY..f32::INFINITY,
			radius: 0.015,
			thickness: 0.075,
			turn_unit_amount: 1.0,
			precisions: Vec::new(),
			segment_length_range: 0.01..0.02,
		}
	}
}

impl<State: ValidState> CustomElement<State> for Dial<State> {
	// You'll need to create this type in stardust_xr_molecules
	type Inner = DialInner;

	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		if self.transform != Transform::IDENTITY {
			info.child_space.set_local_transform(self.transform).await?;
		}
		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&info.child_space,
			Shape::Transform {
				shape: Box::new(Shape::Cylinder {
					radius: self.radius,
					length: self.thickness,
				}),
				transform: Mat4::from_rotation_x(FRAC_PI_2).into(),
			},
		)
		.await?;
		let input = InputQueue::new(
			&context.stardust_client,
			info.child_space.clone(),
			field.clone(),
			info.child_space.spatial_ref().await?,
		)
		.await?;

		let accent_color = context.accent_color.color();
		let lines = Lines::new(
			&context.stardust_client,
			&info.child_space,
			[
				// circles are z-facing
				circle(32, 0.0, self.radius)
					.color(accent_color)
					.transform(Mat4::from_rotation_x(FRAC_PI_2)),
				circle(32, 0.0, self.radius)
					.color(accent_color)
					.transform(Mat4::from_rotation_x(FRAC_PI_2))
					.transform(Mat4::from_translation(vec3(0.0, 0.0, self.thickness))),
			]
			.to_vec(),
		)
		.await?;

		Ok(DialInner {
			spatial: info.child_space,
			lines,
			input,
			single_action: SingleAction::default(),
			field,
			last_vector: None,
		})
	}

	fn diff(&self, old: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old, &inner.spatial);
		if self.radius != old.radius || self.thickness != old.thickness {
			let _ = inner.field.set_shape(Shape::Transform {
				shape: Box::new(Shape::Cylinder {
					radius: self.radius,
					length: self.thickness,
				}),
				transform: Mat4::from_rotation_x(FRAC_PI_2).into(),
			});
		}
	}

	fn frame(
		&self,
		context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		let new_value = inner.update(self, context.accent_color.color());
		if new_value != self.current_value {
			(self.on_change.0)(state, new_value);
		}
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
	spatial: Spatial,
	lines: Lines,
	input: InputQueue,
	single_action: SingleAction,
	field: Field,
	last_vector: Option<Vec2>,
}
impl DialInner {
	pub fn update<State: ValidState>(&mut self, decl: &Dial<State>, accent_color: Color) -> f32 {
		if !self.input.handle_events() {
			return decl.current_value;
		}
		self.single_action.update(
			false,
			&self.input,
			|data| data.distance() < 0.0,
			|data| match &data.input() {
				InputDataType::Hand { data: hand } => hand.pinch_strength() > 0.5,
				_ => data.datamap_f32("select") > 0.5,
			},
		);
		// remove the start value when we stop pinching or such
		if self.single_action.actor_stopped() {
			self.last_vector.take();
		}
		let Some(actor) = self.single_action.actor() else {
			let _ = self
				.lines
				.set_lines(self.signifier_lines::<State>(None, decl, accent_color));
			return decl.current_value;
		};
		if actor.distance() <= 0.0 {
			self.last_vector.take();
		}

		// We need the 2D projected/intersected interaction point
		let interact_point: Vec2 = match actor.input() {
			InputDataType::Pointer { data: pointer } => {
				let origin: Vec3 = pointer.pose.position.into();
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
			InputDataType::Hand { data: hand } => Vec3::from(hand.predicted_pinch_position()).xy(),
			InputDataType::Tip { data: tip } => Vec3::from(tip.pose.position).xy(),
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
			if actor.distance() > 0.0 {
				self.last_vector.replace(interact_point);
			}
			decl.current_value
		};

		let _ = self.lines.set_lines(self.signifier_lines::<State>(
			Some(interact_point),
			decl,
			accent_color,
		));
		new_value
	}

	fn signifier_lines<State: ValidState>(
		&self,
		interact_point: Option<Vec2>,
		decl: &Dial<State>,
		accent_color: Color,
	) -> Vec<Line> {
		let color = if interact_point.is_some() {
			accent_color
		} else {
			rgba_linear!(1.0, 1.0, 1.0, 0.5)
		};
		let mut lines = vec![
			// circles are z-facing
			circle(32, 0.0, decl.radius)
				.color(color)
				.thickness(0.001)
				.transform(Mat4::from_rotation_x(FRAC_PI_2))
				.shimmer(
					&self
						.single_action
						.hovering()
						.current()
						.iter()
						.flat_map(|i| -> Vec<Vec3F> {
							match i.input() {
								InputDataType::Pointer { data: pointer } => {
									vec![
										(Vec3::from(pointer.direction()) * pointer.deepest_point)
											.into(),
									]
								}
								InputDataType::Hand { data: hand } => {
									vec![
										hand.index.tip.pose.position,
										hand.thumb.tip.pose.position,
										hand.stable_pinch_position(),
									]
								}
								InputDataType::Tip { data: tip } => {
									vec![tip.pose.position]
								}
							}
						})
						.collect::<Vec<_>>(),
					0.025,
					0.0,
					accent_color,
					1.25,
				),
			circle(32, 0.0, decl.radius)
				.color(color)
				.thickness(0.001)
				.transform(Mat4::from_rotation_x(FRAC_PI_2))
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
		Tasker,
		client::{self, ClientState},
		elements::Dial,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
	struct TestState {
		value: f32,
	}
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.dial";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
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
			crate::elements::Spatial::default()
				.build()
				.child(
					Dial::create(self.value, |state: &mut TestState, value| {
						state.value = value;
					})
					.radius(0.02)
					.thickness(0.01)
					.current_value(self.value)
					.turn_unit_amount(12.0 * 60.0)
					.range(0.0..(24.0 * 60.0))
					.build(),
				)
				.child(
					Dial::create(self.value, |state: &mut TestState, value| {
						state.value = value;
					})
					.radius(0.025)
					.thickness(0.005)
					.current_value(self.value)
					.turn_unit_amount(60.0)
					.range(0.0..(24.0 * 60.0))
					.build(),
				)
				.child(
					crate::elements::Text::new(format!(
						"{formatted_hours:02.0}:{minutes:02.0} {period}",
					))
					.character_height(0.005)
					.pos([0.0, 0.0, 0.01])
					.build(),
				)
		}
	}
	client::run::<TestState>(&[]).await.unwrap();
}
