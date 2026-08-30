use crate::{
	Context, CreateInnerInfo, ValidState,
	custom::{CustomElement, FnWrapper, Transformable, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::{Mat4, Quat, Vec3};
use map_range::MapRange;
use stardust_xr_fusion::{
	Error,
	client::FrameInfo,
	drawable::{Line, LinePoint, Lines, LinesExt},
	fields::{Field, FieldExt, Shape},
	spatial::{PartialTransform, Spatial, SpatialExt, Transform},
	suis::InputDataType,
	types::rgba_linear,
};
use stardust_xr_molecules::input_action::{InputQueue, InputSnapshot, SimpleAction, SingleAction};
use std::f32::consts::TAU;

type OnRotate<State> = FnWrapper<dyn Fn(&mut State, f32) + Send + Sync>;
#[derive(Setters)]
#[derive_where(Debug)]
pub struct Turntable<State: ValidState> {
	#[setters(skip)]
	transform: Transform,
	#[setters(skip)]
	rotation: f32,
	line_count: u32,
	line_thickness: f32,
	height: f32,
	inner_radius: f32,
	scroll_multiplier: f32,
	#[setters(skip)]
	on_rotate: OnRotate<State>,
}
impl<State: ValidState> Transformable for Turntable<State> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
impl<State: ValidState> CustomElement<State> for Turntable<State> {
	type Inner = TurntableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		// the root is the center of the turntable's *top* surface: it carries the
		// element transform directly, and everything else (field, grip lines) is
		// offset down into -Y from it. that keeps the input handler, the field and
		// the lines all in one space, so no height fudging is needed anywhere else.
		let (root_spatial, root_spatial_ref) =
			Spatial::new(&context.stardust_client, &info.parent_space, self.transform).await?;
		// the content parent only ever spins, so children ride on the platter
		let content_parent = info.child_space;
		content_parent.set_parent(root_spatial_ref.clone())?;
		content_parent.set_local_transform(PartialTransform::from_rotation(
			Quat::from_rotation_y(self.rotation),
		))?;

		let (field, _field_ref) = Field::new(
			&context.stardust_client,
			&root_spatial,
			field_shape(self.inner_radius, self.height),
		)
		.await?;
		let input = InputQueue::new(
			&context.stardust_client,
			root_spatial.clone(),
			field.clone(),
			root_spatial_ref,
		)
		.await?;

		let grip = Lines::new(&context.stardust_client, &content_parent, self.grip_lines()).await?;

		Ok(TurntableInner {
			root: root_spatial,
			content_parent,
			grip,
			// start "lit" so the first frame sends the proximity-colored lines,
			// settling them to their resting (black) state right away
			grip_lit: true,
			field,
			input,
			pointer_hover_action: Default::default(),
			touch_action: Default::default(),
			prev_angle: None,
			angular_momentum: 0.0,
		})
	}

	fn diff(&self, old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.root);
		let size_changed =
			self.inner_radius != old_self.inner_radius || self.height != old_self.height;
		if size_changed {
			inner.set_size(self.inner_radius, self.height);
		}
		if size_changed
			|| self.line_count != old_self.line_count
			|| self.line_thickness != old_self.line_thickness
		{
			// the grip lines are only resent while something is lighting them up, so
			// mark them dirty to guarantee the new geometry goes out next frame
			inner.grip_lit = true;
		}
	}

	fn frame(
		&self,
		_context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		inner.update(*info, self, state);
	}
}

impl<State: ValidState> Turntable<State> {
	pub fn new<F: Fn(&mut State, f32) + Send + Sync + 'static>(
		rotation: f32,
		on_rotate: F,
	) -> Self {
		Turntable {
			transform: Transform::IDENTITY,
			rotation,
			line_count: 106,
			line_thickness: 0.002,
			height: 0.03,
			inner_radius: 0.5,
			scroll_multiplier: 10.0_f32.to_radians(),
			on_rotate: FnWrapper(Box::new(on_rotate)),
		}
	}
	fn grip_lines(&self) -> Vec<Line> {
		(0..self.line_count)
			.map(|c| (c as f32) / (self.line_count as f32) * TAU) // get angle from count
			.map(|a| a.sin_cos()) // get x+y from angle (unit circle)
			.map(|(x, y)| {
				let outer_radius = self.inner_radius + self.height;
				Line {
					points: vec![
						LinePoint {
							point: [x * self.inner_radius, 0.0, y * self.inner_radius].into(),
							thickness: self.line_thickness,
							color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
						},
						LinePoint {
							point: [x * outer_radius, -self.height, y * outer_radius].into(),
							thickness: self.line_thickness,
							color: rgba_linear!(1.0, 1.0, 1.0, 1.0),
						},
					],
					cyclic: false,
				}
			})
			.collect()
	}
}

/// The turntable body hangs below its root: a plain cylinder spanning
/// `y = -height..0`, wide enough to contain the flare of the grip lines
/// (`inner_radius + height`).
fn field_shape(inner_radius: f32, height: f32) -> Shape {
	Shape::Transform {
		shape: Box::new(Shape::Cylinder {
			length: height,
			radius: inner_radius + height,
		}),
		transform: Mat4::from_translation([0.0, height * -0.5, 0.0].into()).into(),
	}
}

fn interact_point(input: &InputSnapshot) -> Option<Vec3> {
	match input.input() {
		InputDataType::Hand { data: h } => Some(
			Vec3::from(h.thumb.tip.pose.position).lerp(Vec3::from(h.index.tip.pose.position), 0.5),
		),
		InputDataType::Tip { data: t } => Some(t.pose.position.into()),
		_ => None,
	}
}
fn interact_points(input: &InputSnapshot) -> Vec<Vec3> {
	match input.input() {
		InputDataType::Hand { data: h } => {
			vec![
				h.thumb.tip.pose.position.into(),
				h.index.tip.pose.position.into(),
				h.ring.tip.pose.position.into(),
				h.middle.tip.pose.position.into(),
				h.little.tip.pose.position.into(),
			]
		}
		InputDataType::Tip { data: t } => vec![t.pose.position.into()],
		_ => vec![],
	}
}
fn interact_proximity(input: &InputQueue, point: Vec3) -> f32 {
	input
		.input()
		.values()
		.map(|i| match i.input() {
			InputDataType::Hand { data: h } => [
				h.thumb.tip.pose.position,
				h.index.tip.pose.position,
				h.ring.tip.pose.position,
				h.middle.tip.pose.position,
				h.little.tip.pose.position,
			]
			.into_iter()
			.map(|p| Vec3::from(p).distance(point))
			.reduce(f32::min)
			.unwrap_or(f32::INFINITY),
			InputDataType::Tip { data: t } => Vec3::from(t.pose.position).distance(point),
			// a pointer is a ray, not a point, so use the distance from the grip
			// point to the ray itself (clamped to in front of the pointer)
			InputDataType::Pointer { data: p } => {
				let origin = Vec3::from(p.pose.position);
				let direction = Quat::from(p.pose.orientation) * Vec3::NEG_Z;
				let t = (point - origin).dot(direction).max(0.0);
				(origin + direction * t).distance(point)
			}
		})
		.reduce(f32::min)
		.unwrap_or(f32::INFINITY)
}
fn interact_angle(input: &InputSnapshot) -> Option<f32> {
	let p = interact_point(input)?;
	Some(p.z.atan2(p.x))
}

pub struct TurntableInner {
	root: Spatial,
	content_parent: Spatial,
	grip: Lines,
	grip_lit: bool,
	field: Field,

	input: InputQueue,
	pointer_hover_action: SimpleAction,
	touch_action: SingleAction,
	angular_momentum: f32,
	prev_angle: Option<f32>,
}
impl TurntableInner {
	pub fn root(&self) -> &Spatial {
		&self.root
	}
	pub fn content_parent(&self) -> &Spatial {
		&self.content_parent
	}

	pub fn set_size(&self, inner_radius: f32, height: f32) {
		let _ = self.field.set_shape(field_shape(inner_radius, height));
	}

	#[inline]
	fn scroll(&self) -> f32 {
		self.pointer_hover_action
			.currently_acting()
			.iter()
			.map(|i| {
				let scroll_continuous = i.datamap_vec2("scroll_continuous");
				let scroll_discrete = i.datamap_vec2("scroll_discrete");

				scroll_continuous.x
					+ scroll_continuous.y
					+ (scroll_discrete.x * 5.0)
					+ (scroll_discrete.y * 5.0)
			})
			.reduce(|a, b| a + b)
			.unwrap_or_default()
	}
	pub fn rotate<State: ValidState>(
		&mut self,
		mut rotation: f32,
		angle: f32,
		state: &mut State,
		on_rotate: &OnRotate<State>,
	) {
		rotation += angle;
		let _ = self
			.content_parent
			.set_local_transform(PartialTransform::from_rotation(Quat::from_rotation_y(
				rotation,
			)));
		(on_rotate.0)(state, rotation);
	}
	pub fn update<State: ValidState>(
		&mut self,
		info: FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		self.input.handle_events();
		self.update_pointer_hover(settings);
		self.update_touch(settings);
		self.update_scroll_rotation(settings, state);
		self.update_touch_rotation(&info, settings, state);
		self.update_momentum_rotation(&info, settings, state);
		self.update_grip_visuals(settings);
	}

	fn update_pointer_hover<State: ValidState>(&mut self, _settings: &Turntable<State>) {
		self.pointer_hover_action
			.update(&self.input, &|input| match input.input() {
				InputDataType::Pointer { data: _ } => input.distance() < 0.0,
				_ => false,
			});
	}

	fn update_touch<State: ValidState>(&mut self, settings: &Turntable<State>) {
		self.touch_action.update(
			false,
			&self.input,
			|_| true,
			|input| {
				let slope_condition = interact_points(input).into_iter().any(|p| {
					// the input handler sits at the center of the top surface, so p.y
					// is negative inside the body and its magnitude is the depth
					let interact_point_height = p.y;
					// distance on XZ plane from center
					let interact_point_radius = p.x.hypot(p.z);
					// treat it as a cone so we can compare height to width for slope
					let interact_point_radius_slope =
						(interact_point_radius - settings.inner_radius).max(0.0);
					interact_point_height.abs() > interact_point_radius_slope
				});
				let distance_condition = input.distance() < 0.0;
				slope_condition && distance_condition
			},
		);
	}

	fn update_scroll_rotation<State: ValidState>(
		&mut self,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		let scroll_rotation = -self.scroll() * settings.scroll_multiplier;
		self.rotate(
			scroll_rotation,
			settings.rotation,
			state,
			&settings.on_rotate,
		);
	}

	fn update_touch_rotation<State: ValidState>(
		&mut self,
		info: &FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		if let Some(angle) = self
			.touch_action
			.actor()
			.cloned()
			.as_deref()
			.and_then(interact_angle)
		{
			if let Some(prev_angle) = self.prev_angle {
				let delta = prev_angle - angle;
				self.angular_momentum = delta * info.delta;
				self.rotate(delta, settings.rotation, state, &settings.on_rotate);
			}
			self.prev_angle.replace(angle);
		}
		if self.touch_action.actor_stopped() {
			self.prev_angle.take();
		}
	}

	fn update_momentum_rotation<State: ValidState>(
		&mut self,
		info: &FrameInfo,
		settings: &Turntable<State>,
		state: &mut State,
	) {
		self.angular_momentum *= 0.98;
		if !self.touch_action.actor_acting() && self.angular_momentum.abs() > 0.0 {
			self.rotate(
				self.angular_momentum / info.delta,
				settings.rotation,
				state,
				&settings.on_rotate,
			);
		}
	}

	fn update_grip_visuals<State: ValidState>(&mut self, settings: &Turntable<State>) {
		let mut lines = settings.grip_lines();
		let mut any_lit = false;
		for line in &mut lines {
			for point in &mut line.points {
				let lerp = interact_proximity(
					&self.input,
					Quat::from_rotation_y(settings.rotation) * Vec3::from(point.point),
				)
				.map_range(0.05..0.0, 1.0..0.0)
				.clamp(0.0, 1.0);
				any_lit |= lerp > 0.0;
				point.color = rgba_linear!(lerp, lerp, lerp, 1.0);
			}
		}
		// Only resend the lines while an input method is close enough to light them
		// up. `grip_lit` keeps us sending for one trailing frame after the last
		// input leaves, so the lines settle back to black instead of getting stuck.
		if !any_lit && !self.grip_lit {
			return;
		}
		self.grip_lit = any_lit;
		self.grip.set_lines(lines).unwrap();
	}
}

#[tokio::test]
async fn asteroids_turntable_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		components::Derezzable,
		custom::CustomElement,
		elements::{Lines, Turntable},
	};
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::spatial::BoundingBox;
	use stardust_xr_molecules::lines::{LineExt, bounding_box};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState {
		elapsed: f32,
		#[serde(skip)]
		rotation: f32,
	}

	impl TestState {
		pub fn handle_rotation(&mut self, rotation: f32) {
			self.rotation = rotation;
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.turntable";
		fn on_frame(&mut self, info: &FrameInfo) {
			self.elapsed += info.delta;
		}
	}
	impl crate::Reify for TestState {
		fn reify(&self, context: &Context, _tasks: impl Tasker<Self>) -> impl crate::Element<Self> {
			crate::Entity::new(stardust_xr_fusion::fields::Shape::Sphere { radius: 0.05 })
				.component(Derezzable::program_stopper(context))
				.build()
				.child(
					Turntable::new(self.rotation, Self::handle_rotation)
						.line_count(64)
						.line_thickness(0.002)
						.height(0.03)
						// .inner_radius(0.1)
						.inner_radius((self.elapsed.sin() * 0.01) + 0.1)
						.scroll_multiplier(1.0_f32.to_radians())
						.build()
						.child(
							Lines::new(
								bounding_box(BoundingBox {
									center: [0.0; 3].into(),
									extents: [0.05; 3].into(),
								})
								.into_iter()
								.map(|l| l.thickness(0.002)),
							)
							.pos([0.0, 0.025, 0.0])
							.build(),
						),
				)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
