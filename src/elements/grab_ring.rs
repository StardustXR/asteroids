use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::Vec3;
use mint::Vector3;
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, Transform},
	suis::InputDataType,
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputSnapshot, SingleAction},
	lines::{LineExt, circle},
};

type OnGrab<State> = FnWrapper<dyn Fn(&mut State, Vector3<f32>) + Send + Sync>;
#[derive(Setters)]
#[derive_where(Debug)]
pub struct GrabRing<State: ValidState> {
	radius: f32,
	thickness: f32,

	#[setters(skip)]
	pos: Vector3<f32>,
	#[setters(skip)]
	on_grab: OnGrab<State>,
}
impl<State: ValidState> GrabRing<State> {
	pub fn new<F: Fn(&mut State, Vector3<f32>) + Send + Sync + 'static>(
		pos: impl Into<Vector3<f32>>,
		on_grab: F,
	) -> Self {
		GrabRing {
			pos: pos.into(),
			on_grab: FnWrapper(Box::new(on_grab)),

			radius: 0.05,
			thickness: 0.004,
		}
	}
}
impl<State: ValidState> CustomElement<State> for GrabRing<State> {
	type Inner = GrabRingInner;

	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let content_root = info.child_space;
		content_root.set_local_transform(Transform::from_translation(self.pos))?;

		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&content_root,
			Shape::Torus {
				major_radius: self.radius,
				minor_radius: self.thickness,
			},
		)
		.await?;
		let input = InputQueue::new(
			&context.stardust_client,
			content_root.clone(),
			field.clone(),
			// input data is reported relative to the *stationary* parent space, not the
			// moving content_root — otherwise dragging fights itself (jitter + half movement)
			info.parent_space.clone(),
		)
		.await?;

		let ring_line = circle(64, 0.0, self.radius).thickness(self.thickness);
		let ring_visual = Lines::new(
			&context.stardust_client,
			&content_root,
			vec![ring_line.clone()],
		)
		.await?;

		Ok(GrabRingInner {
			field,
			input,
			grab_action: SingleAction::default(),
			pointer_distance: 0.0,
			old_interact_point: Vec3::ZERO,
			content_root,
			ring_visual,
			ring_line,
		})
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.radius != old_self.radius || self.thickness != old_self.thickness {
			inner.resize(self.radius, self.thickness);
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if let Some(pos) = inner.handle_events(self.pos) {
			(self.on_grab.0)(state, pos);
		}
	}
}

pub struct GrabRingInner {
	field: Field,
	input: InputQueue,
	grab_action: SingleAction,
	old_interact_point: Vec3,
	pointer_distance: f32,
	content_root: Spatial,
	ring_visual: Lines,
	ring_line: Line,
}
impl GrabRingInner {
	fn interact_point(&self, input: &InputSnapshot) -> Vec3 {
		match input.input() {
			InputDataType::Hand { data: hand } => {
				// For hands, use midpoint between thumb and index finger (pinch position)
				Vec3::from(hand.thumb.tip.pose.position)
					.lerp(Vec3::from(hand.index.tip.pose.position), 0.5)
			}
			InputDataType::Tip { data: tip } => {
				// For tips, use the origin point
				Vec3::from(tip.pose.position)
			}
			InputDataType::Pointer { data: pointer } => {
				// Calculate position at current distance along pointer ray
				let origin = Vec3::from(pointer.pose.position);
				let direction = Vec3::from(pointer.direction()).normalize();
				origin + (direction * self.pointer_distance)
			}
		}
	}

	fn update_input(&mut self) -> bool {
		if !self.input.handle_events() {
			return false;
		}
		self.grab_action.update(
			true,
			&self.input,
			|i| i.distance() < 0.05,
			|i| match i.input() {
				InputDataType::Hand { .. } => i.datamap_f32("pinch_strength") > 0.8,
				_ => i.datamap_f32("grab") > 0.8,
			},
		);

		let start_grab = self.grab_action.actor_started();
		if let Some(input) = self.grab_action.actor() {
			if let InputDataType::Pointer { data: pointer } = input.input() {
				if start_grab {
					// deepest_point is now a distance along the ray
					self.pointer_distance = pointer.deepest_point;
				}
				// Adjust pointer_distance based on scroll input
				// TODO(api): verify the datamap vector accessor name in the new suis API
				let scroll_continuous = input.datamap_vec2("scroll_continuous").y;
				let scroll_discrete = input.datamap_vec2("scroll_discrete").y;
				self.pointer_distance += (scroll_continuous * 0.01) + // continuous +Y -> 1cm farther away
					(scroll_discrete * 0.1); // discrete +Y -> 10cm farther away
			}

			if start_grab {
				self.old_interact_point = self.interact_point(input);
			}
		}
		true
	}

	fn handle_grab(&mut self, pos: Vec3) -> Option<Vec3> {
		let input = self.grab_action.actor()?;
		let new_interact_point = self.interact_point(input);
		let delta = new_interact_point - self.old_interact_point;
		self.old_interact_point = new_interact_point;
		Some(pos + delta)
	}

	pub fn handle_events(&mut self, pos: Vector3<f32>) -> Option<Vector3<f32>> {
		if !self.update_input() {
			return None;
		}

		let new_pos = self.handle_grab(pos.into());
		if let Some(new_pos) = new_pos.as_ref() {
			let _ = self
				.content_root
				.set_local_transform(Transform::from_translation(*new_pos));
		}

		new_pos.map(Into::into)
	}

	// fn update_signifiers(&mut self, pos: Vec3) {
	//     for point in &mut self.ring_line.points {
	//         let lerp = Self::interact_proximity(&self.input, Vec3::from(point.point) + pos)
	//             .map_range(0.05..0.0, 0.0..1.0)
	//             .clamp(0.0, 1.0);
	//         point.color = rgba_linear!(lerp, lerp, lerp, 1.0);
	//     }
	//     let _ = self.ring_visual.set_lines(&[self.ring_line.clone()]);
	// }

	// fn interact_proximity(input: &InputQueue, point: Vec3) -> f32 {
	//     input
	//         .input()
	//         .keys()
	//         .map(|i| match &i.input {
	//             InputDataType::Hand(h) => vec![
	//                 h.thumb.tip.position,
	//                 h.index.tip.position,
	//                 h.ring.tip.position,
	//                 h.middle.tip.position,
	//                 h.little.tip.position,
	//             ]
	//             .into_iter()
	//             .map(|p| Vec3::from(p).distance(point))
	//             .reduce(|a, b| a.min(b))
	//             .unwrap_or(f32::INFINITY),
	//             InputDataType::Tip(t) => Vec3::from(t.origin).distance(point),
	//             InputDataType::Pointer(p) => {
	//                 // Convert pointer origin to Vec3 for calculations
	//                 let origin = Vec3::from(p.origin);
	//                 // Get normalized direction vector of pointer
	//                 let direction = Vec3::from(p.direction()).normalize();
	//                 // Vector from origin to point we're checking
	//                 let v = point - origin;
	//                 // Project v onto direction to get distance along ray
	//                 let t = v.dot(direction);
	//                 if t < 0.0 {
	//                     // Point is behind ray origin, use direct distance to origin
	//                     point.distance(origin)
	//                 } else {
	//                     // Point is in front of ray origin
	//                     // Get closest point on ray by moving t distance along direction
	//                     let projection = origin + direction * t;
	//                     // Return shortest distance from point to ray
	//                     point.distance(projection)
	//                 }
	//             }
	//         })
	//         .reduce(|a, b| a.min(b))
	//         .unwrap_or(f32::INFINITY)
	// }

	pub fn resize(&mut self, radius: f32, thickness: f32) {
		let _ = self.field.set_shape(Shape::Torus {
			major_radius: radius,
			minor_radius: thickness,
		});
		self.ring_line = circle(64, 0.0, radius).thickness(thickness);
		let _ = self.ring_visual.set_lines(vec![self.ring_line.clone()]);
	}
}

#[tokio::test]
async fn asteroids_grab_ring_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		elements::GrabRing,
	};
	use mint::Vector3;
	use serde::{Deserialize, Serialize};

	#[derive(Serialize, Deserialize)]
	struct TestState {
		grab_pos: Vector3<f32>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				grab_pos: [0.0; 3].into(),
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.grab_ring";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			GrabRing::new(self.grab_pos, |state: &mut Self, pos| {
				state.grab_pos = pos;
			})
			.radius(0.05)
			.thickness(0.004)
			.build()
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
