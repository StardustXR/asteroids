use std::f32::consts::FRAC_PI_2;

use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::{Mat4, Vec3, vec3};
use map_range::MapRange;
use mint::Vector3;
use stardust_xr_fusion::{
	Error, Result,
	drawable::{Line, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{Spatial, Transform},
	suis::InputDataType,
	types::rgba_linear,
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputSnapshot, SingleAction},
	lines::{LineExt, circle, line_from_points},
};

const RADIUS: f32 = 0.01;
const LINE_THICKNESS: f32 = 0.001;

type OnGrab<State> = FnWrapper<dyn Fn(&mut State, Vector3<f32>) + Send + Sync>;
type OnRelease<State> = FnWrapper<dyn Fn(&mut State, Vector3<f32>) + Send + Sync>;
#[derive(Setters)]
#[derive_where(Debug)]
#[setters(into)]
pub struct Handle<State: ValidState> {
	#[setters(skip)]
	root_pos: Vector3<f32>,
	head_offset: Vector3<f32>,
	#[setters(skip)]
	on_grab: OnGrab<State>,
	#[setters(skip)]
	on_release: OnRelease<State>,
}
impl<State: ValidState> Handle<State> {
	pub fn new<F: Fn(&mut State, Vector3<f32>) + Send + Sync + 'static>(
		root_pos: impl Into<Vector3<f32>>,
		on_grab: F,
	) -> Self {
		Handle {
			root_pos: root_pos.into(),
			head_offset: [0.0; 3].into(),
			on_grab: FnWrapper(Box::new(on_grab)),
			on_release: FnWrapper(Box::new(|_, _| ())),
		}
	}
	pub fn on_release<F: Fn(&mut State, Vector3<f32>) + Send + Sync + 'static>(
		mut self,
		f: F,
	) -> Self {
		self.on_release = FnWrapper(Box::new(f));
		self
	}
}
impl<State: ValidState> CustomElement<State> for Handle<State> {
	type Inner = HandleInner;
	type Error = Error;

	async fn create_inner(&self, context: &Context, info: CreateInnerInfo) -> Result<Self::Inner> {
		let content_root = info.child_space;
		content_root.set_local_transform(Transform::from_translation(self.root_pos))?;

		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&content_root,
			Shape::Transform {
				shape: Box::new(Shape::Sphere { radius: RADIUS }),
				transform: Mat4::from_translation(self.head_offset.into()).into(),
			},
		)
		.await?;
		let input = InputQueue::new(
			&context.stardust_client,
			content_root.clone(),
			field.clone(),
			// input data is reported relative to the *stationary* parent space so the
			// interact point doesn't jitter as `content_root` follows the drag
			info.parent_space.clone(),
		)
		.await?;

		let diamond = circle(4, 0.0, RADIUS).thickness(LINE_THICKNESS);
		let octahedron = [
			diamond.clone().transform(Mat4::from_rotation_x(FRAC_PI_2)),
			diamond.clone().transform(Mat4::from_rotation_z(FRAC_PI_2)),
			diamond,
		];
		let lines =
			Lines::new(&context.stardust_client, &content_root, octahedron.to_vec()).await?;
		let mut inner = HandleInner {
			field,
			input,
			grab_action: SingleAction::default(),
			pointer_distance: 0.0,
			last_grab_pos: self.root_pos,
			content_root,
			octahedron,
			lines,
		};
		if self.head_offset != [0.0; 3].into() {
			inner.update_signifiers(self.root_pos.into(), self.head_offset.into());
		}
		Ok(inner)
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		if self.root_pos != old_self.root_pos {
			// Update the position of the handle
			let _ = inner
				.content_root
				.set_local_transform(Transform::from_translation(self.root_pos));
		}

		if self.head_offset != old_self.head_offset {
			inner.update_signifiers(self.root_pos.into(), self.head_offset.into());
			_ = inner.field.set_shape(Shape::Transform {
				shape: Box::new(Shape::Sphere { radius: RADIUS }),
				transform: Mat4::from_translation(self.head_offset.into()).into(),
			});
		}
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if let Some(update) = inner.handle_events(self.root_pos, self.head_offset) {
			(self.on_grab.0)(state, update.pos);
			if update.released {
				(self.on_release.0)(state, update.pos);
			}
		}
	}
}

/// A grab interaction this frame, reported back so the element can edit `State`.
pub struct HandleUpdate {
	/// The interact point while grabbed, or the last grabbed point on release.
	pub pos: Vector3<f32>,
	/// The grab was let go this frame.
	pub released: bool,
}

pub struct HandleInner {
	content_root: Spatial,
	field: Field,
	input: InputQueue,
	grab_action: SingleAction,
	pointer_distance: f32,
	last_grab_pos: Vector3<f32>,
	octahedron: [Line; 3],
	lines: Lines,
}
impl HandleInner {
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
			false,
			&self.input,
			|i| i.distance() < 0.05,
			|i| match i.input() {
				InputDataType::Hand { .. } => i.datamap_f32("pinch_strength") > 0.5,
				_ => i.datamap_f32("grab") > 0.5,
			},
		);

		// Initialize pointer distance when grab starts with a pointer
		let start_grab = self.grab_action.actor_started();
		if let Some(input) = self.grab_action.actor() {
			if let InputDataType::Pointer { data: pointer } = input.input() {
				if start_grab {
					// deepest_point is a distance along the ray in the new API
					self.pointer_distance = pointer.deepest_point;
				}
				// Adjust pointer_distance based on scroll input
				let scroll_continuous = input.datamap_vec2("scroll_continuous").y;
				let scroll_discrete = input.datamap_vec2("scroll_discrete").y;
				self.pointer_distance += (scroll_continuous * 0.01) + (scroll_discrete * 0.1);
			}
		}

		true
	}

	pub fn handle_events(
		&mut self,
		root_pos: Vector3<f32>,
		head_offset: Vector3<f32>,
	) -> Option<HandleUpdate> {
		if !self.update_input() {
			return None;
		}
		self.update_signifiers(root_pos.into(), head_offset.into());
		if let Some(input) = self.grab_action.actor() {
			self.last_grab_pos = self.interact_point(input).into();
			Some(HandleUpdate {
				pos: self.last_grab_pos,
				released: false,
			})
		} else if self.grab_action.actor_stopped() {
			Some(HandleUpdate {
				pos: self.last_grab_pos,
				released: true,
			})
		} else {
			None
		}
	}

	fn update_signifiers(&mut self, root_pos: Vec3, head_offset: Vec3) {
		// proximity coloring is keyed off each vertex's *resting* world position (point + pos)
		for line in &mut self.octahedron {
			for point in &mut line.points {
				let lerp =
					Self::interact_proximity(&self.input, Vec3::from(point.point) + root_pos)
						.map_range(0.05..0.0, 0.0..1.0)
						.clamp(0.5, 1.0);
				point.color = rgba_linear!(lerp, lerp, lerp, 1.0);
			}
		}

		// The lines are parented to `content_root` (at `pos`), so everything here is in
		// content_root-local space. While grabbed we bake the interact-point offset into
		// the geometry rather than moving a spatial — that keeps `content_root` purely the
		// logical position `diff` owns, so the two don't fight (which caused the snapping).
		let offset = self
			.grab_action
			.actor()
			.map(|a| self.interact_point(a) - root_pos)
			.unwrap_or(head_offset);
		let octahedron = self.octahedron.iter().cloned().map(|mut line| {
			for point in &mut line.points {
				point.point = (Vec3::from(point.point) + offset).into();
			}
			line
		});
		// a tether from the resting position (local origin) to the interact point
		let handle_line = line_from_points(vec![vec3(0.0, 0.0, 0.0), offset]).thickness(0.001);
		let lines = octahedron
			.chain(std::iter::once(handle_line))
			.collect::<Vec<_>>();
		let _ = self.lines.set_lines(lines.as_slice());
	}

	fn interact_proximity(input: &InputQueue, point: Vec3) -> f32 {
		input
			.input()
			.values()
			.map(|i| match i.input() {
				InputDataType::Hand { data: h } => {
					[h.thumb.tip.pose.position, h.index.tip.pose.position]
						.into_iter()
						.map(|p| Vec3::from(p).distance(point))
						.reduce(f32::min)
						.unwrap_or(f32::INFINITY)
				}
				InputDataType::Tip { data: t } => Vec3::from(t.pose.position).distance(point),
				InputDataType::Pointer { data: p } => {
					// Convert pointer origin to Vec3 for calculations
					let origin = Vec3::from(p.pose.position);
					// Get normalized direction vector of pointer
					let direction = Vec3::from(p.direction()).normalize();
					// Vector from origin to point we're checking
					let v = point - origin;
					// Project v onto direction to get distance along ray
					let t = v.dot(direction);
					if t < 0.0 {
						// Point is behind ray origin, use direct distance to origin
						point.distance(origin)
					} else {
						// Point is in front of ray origin
						// Get closest point on ray by moving t distance along direction
						let projection = origin + direction * t;
						// Return shortest distance from point to ray
						point.distance(projection)
					}
				}
			})
			.reduce(f32::min)
			.unwrap_or(f32::INFINITY)
	}
}

#[tokio::test]
async fn asteroids_handle_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		elements::Handle,
	};
	use glam::FloatExt;
	use serde::{Deserialize, Serialize};

	#[derive(Serialize, Deserialize)]
	struct TestState {
		slider_value: f32,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState { slider_value: 0.5 }
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.handle";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			let width = 0.1;
			let start_x = width * -0.5;
			let end_x = width * 0.5;
			let slide_point = (start_x).lerp(end_x, self.slider_value);
			crate::elements::Lines::new({
				[
					line_from_points(vec![vec3(start_x, 0.0, 0.0), vec3(slide_point, 0.0, 0.0)])
						.thickness(0.001),
					line_from_points(vec![vec3(slide_point, 0.0, 0.0), vec3(end_x, 0.0, 0.0)])
						.thickness(0.001)
						.color(rgba_linear!(0.1, 0.1, 0.75, 1.0)),
				]
			})
			.build()
			.child(
				Handle::new([slide_point, 0.0, 0.0], move |state: &mut Self, pos| {
					state.slider_value = pos.x.map_range(start_x..end_x, 0.0..1.0).clamp(0.0, 1.0);
				})
				.head_offset([0.0, 0.01, 0.0])
				.build(),
			)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
