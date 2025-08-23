use std::f32::consts::FRAC_PI_2;

use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::{Mat4, Vec3};
use map_range::MapRange;
use mint::Vector3;
use stardust_xr_fusion::{
	drawable::{Line, Lines, LinesAspect},
	values::color::rgba_linear,
};
use stardust_xr_fusion::{
	fields::{Field, Shape},
	input::{InputData, InputDataType, InputHandler},
	node::NodeResult,
	spatial::{Spatial, SpatialAspect, SpatialRef, Transform},
};
use stardust_xr_molecules::{
	input_action::{InputQueue, InputQueueable, SingleAction},
	lines::{LineExt, circle},
};

const RADIUS: f32 = 0.01;
const LINE_THICKNESS: f32 = 0.001;

type OnGrab<State> = FnWrapper<dyn Fn(&mut State, Vector3<f32>) + Send + Sync>;
#[derive(Setters)]
#[derive_where(Debug)]
pub struct Handle<State: ValidState> {
	#[setters(skip)]
	pos: Vector3<f32>,
	#[setters(skip)]
	on_grab: OnGrab<State>,
}
impl<State: ValidState> Handle<State> {
	pub fn new<F: Fn(&mut State, Vector3<f32>) + Send + Sync + 'static>(
		pos: impl Into<Vector3<f32>>,
		on_grab: F,
	) -> Self {
		Handle {
			pos: pos.into(),
			on_grab: FnWrapper(Box::new(on_grab)),
		}
	}
}
impl<State: ValidState> CustomElement<State> for Handle<State> {
	type Inner = HandleInner;
	type Resource = ();
	type Error = stardust_xr_fusion::node::NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		HandleInner::new(info.parent_space, self.pos)
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.pos != old_self.pos {
			// Update the position of the handle
			let _ = inner
				.content_root
				.set_local_transform(Transform::from_translation(self.pos));
		}
	}

	fn frame(
		&self,
		_info: &stardust_xr_fusion::root::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if let Some(pos) = inner.handle_events(self.pos) {
			(self.on_grab.0)(state, pos);
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.content_root.clone().as_spatial_ref()
	}
}

pub struct HandleInner {
	_field: Field,
	input: InputQueue,
	grab_action: SingleAction,
	pointer_distance: f32,
	content_root: Spatial,
	octahedron: [Line; 3],
	lines: Lines,
}
impl HandleInner {
	pub fn new(parent_space: &SpatialRef, pos: Vector3<f32>) -> NodeResult<Self> {
		let field = Field::create(parent_space, Transform::identity(), Shape::Sphere(RADIUS))?;
		let input = InputHandler::create(parent_space, Transform::identity(), &field)?.queue()?;
		let content_root =
			Spatial::create(input.handler(), Transform::from_translation(pos), true)?;
		field.set_spatial_parent(&content_root)?;

		let diamond = circle(4, 0.0, RADIUS).thickness(LINE_THICKNESS);
		let octahedron = [
			diamond.clone().transform(Mat4::from_rotation_x(FRAC_PI_2)),
			diamond.clone().transform(Mat4::from_rotation_z(FRAC_PI_2)),
			diamond,
		];
		let lines = Lines::create(&content_root, Transform::identity(), &octahedron)?;

		Ok(HandleInner {
			_field: field,
			input,
			grab_action: SingleAction::default(),
			pointer_distance: 0.0,
			content_root,
			octahedron,
			lines,
		})
	}

	fn interact_point(&self, input: &InputData) -> Vec3 {
		match &input.input {
			InputDataType::Hand(h) => {
				// For hands, use midpoint between thumb and index finger (pinch position)
				Vec3::from(h.thumb.tip.position).lerp(Vec3::from(h.index.tip.position), 0.5)
			}
			InputDataType::Tip(t) => {
				// For tips, use the origin point
				Vec3::from(t.origin)
			}
			InputDataType::Pointer(p) => {
				// Calculate position at current distance along pointer ray
				let origin = Vec3::from(p.origin);
				let direction = Vec3::from(p.direction()).normalize();
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
			|i| i.distance < 0.05,
			|i| {
				i.datamap.with_data(|d| match &i.input {
					InputDataType::Hand(_) => d.idx("pinch_strength").as_f32() > 0.5,
					_ => d.idx("grab").as_f32() > 0.5,
				})
			},
		);

		// Initialize pointer distance when grab starts with a pointer
		if let Some(input) = self.grab_action.actor() {
			if let InputDataType::Pointer(p) = &input.input {
				if self.grab_action.actor_started() {
					// Set initial pointer distance based on deepest point
					self.pointer_distance =
						Vec3::from(p.origin).distance(Vec3::from(p.deepest_point));
				}
				// Adjust pointer_distance based on scroll input
				let scroll = input
					.datamap
					.with_data(|d| d.idx("scroll_continuous").as_vector().idx(1).as_f32());
				self.pointer_distance += scroll * 0.01;
			}
		}

		true
	}

	pub fn handle_events(&mut self, pos: Vector3<f32>) -> Option<Vector3<f32>> {
		if !self.update_input() {
			return None;
		}
		self.update_signifiers(pos.into());
		let input = self.grab_action.actor()?;
		Some(self.interact_point(input).into())
	}

	fn update_signifiers(&mut self, pos: Vec3) {
		for line in &mut self.octahedron {
			for point in &mut line.points {
				let lerp = Self::interact_proximity(&self.input, Vec3::from(point.point) + pos)
					.map_range(0.05..0.0, 0.0..1.0)
					.clamp(0.5, 1.0);
				point.color = rgba_linear!(lerp, lerp, lerp, 1.0);
			}
		}
		let _ = self.lines.set_lines(&self.octahedron);
	}

	fn interact_proximity(input: &InputQueue, point: Vec3) -> f32 {
		input
			.input()
			.keys()
			.map(|i| match &i.input {
				InputDataType::Hand(h) => vec![h.thumb.tip.position, h.index.tip.position]
					.into_iter()
					.map(|p| Vec3::from(p).distance(point))
					.reduce(|a, b| a.min(b))
					.unwrap_or(f32::INFINITY),
				InputDataType::Tip(t) => Vec3::from(t.origin).distance(point),
				InputDataType::Pointer(p) => {
					// Convert pointer origin to Vec3 for calculations
					let origin = Vec3::from(p.origin);
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
			.reduce(|a, b| a.min(b))
			.unwrap_or(f32::INFINITY)
	}
}

#[tokio::test]
async fn asteroids_grab_ring_element() {
	use crate::{
		client::{self, ClientState},
		elements::Handle,
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
		fn reify(&self) -> impl crate::Element<Self> {
			Handle::new(self.grab_pos, |state: &mut Self, pos| {
				state.grab_pos = pos;
			})
			.build()
		}
	}

	client::run::<TestState>(&[]).await
}
