use crate::{
	Component, ComponentCreateInfo, Context, ValidState,
	custom::{FnWrapper, derive_setters::Setters},
};
use derive_where::derive_where;
use glam::{Affine3A, Quat, Vec3, vec3};
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{Error, Result, client::FrameInfo, suis::InputDataType};
use stardust_xr_molecules::input_action::{
	InputQueue, InputSnapshot, SingleAction, grab_pinch_interact,
};
use std::f32::consts::PI;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerMode {
	Parent,
	Align,
	Move,
}

/// Called every frame the pose changes while grabbed, with the new pose.
type OnChangePose<State> =
	FnWrapper<dyn Fn(&mut State, Vector3<f32>, Quaternion<f32>) + Send + Sync>;
type GrabStart<State> = FnWrapper<dyn Fn(&mut State) + Send + Sync>;
type GrabStop<State> = FnWrapper<dyn Fn(&mut State) + Send + Sync>;

#[derive_where(Debug)]
#[derive(Setters)]
#[setters(into)]
pub struct Grabbable<State: ValidState> {
	#[setters(skip)]
	pos: Vector3<f32>,
	#[setters(skip)]
	rot: Quaternion<f32>,
	#[setters(skip)]
	on_change_pose: OnChangePose<State>,
	#[setters(skip)]
	grab_start: GrabStart<State>,
	#[setters(skip)]
	grab_stop: GrabStop<State>,
	/// Max distance that you can be to start grabbing.
	max_distance: f32,
	/// How should pointers be handled?
	pointer_mode: PointerMode,
}
impl<State: ValidState> Grabbable<State> {
	pub fn new<F: Fn(&mut State, Vector3<f32>, Quaternion<f32>) + Send + Sync + 'static>(
		pos: impl Into<Vector3<f32>>,
		rot: impl Into<Quaternion<f32>>,
		on_change: F,
	) -> Self {
		Grabbable {
			pos: pos.into(),
			rot: rot.into(),
			on_change_pose: FnWrapper(Box::new(on_change)),
			grab_start: FnWrapper(Box::new(|_| ())),
			grab_stop: FnWrapper(Box::new(|_| ())),
			max_distance: 0.05,
			pointer_mode: PointerMode::Parent,
		}
	}

	pub fn grab_start<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
		self.grab_start = FnWrapper(Box::new(f));
		self
	}
	pub fn grab_stop<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
		self.grab_stop = FnWrapper(Box::new(f));
		self
	}
}
impl<State: ValidState> Component<State> for Grabbable<State> {
	type Inner = GrabbableInner;

	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner> {
		// the entity owns the spatial/field/transform now — we just sense input on them.
		// input is reported relative to the *stationary* parent space, not the moving
		// entity spatial — otherwise dragging fights itself (jitter + half movement)
		let input = InputQueue::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.field.clone(),
			info.parent_space.clone(),
		)
		.await?;

		Ok(GrabbableInner {
			input,
			grab_action: SingleAction::default(),
			max_distance: self.max_distance,
			pointer_mode: self.pointer_mode,
			relative_transform: Affine3A::IDENTITY,
			prev_pose: Affine3A::IDENTITY,
		})
	}

	fn diff(&self, _old_self: &Self, _context: &Context, inner: &mut Self::Inner) {
		// the entity applies the state-owned pose onto the shared spatial; we only sync knobs.
		inner.max_distance = self.max_distance;
		inner.pointer_mode = self.pointer_mode;
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		let current =
			Affine3A::from_rotation_translation(Quat::from(self.rot), Vec3::from(self.pos));
		let update = inner.handle_events(current);

		if let Some((pos, rot)) = update.new_pose {
			(self.on_change_pose.0)(state, pos.into(), rot.into());
		}
		if update.started {
			(self.grab_start.0)(state);
		}
		if update.stopped {
			(self.grab_stop.0)(state);
		}
	}
}

/// What happened to the grab this frame, reported back so the element can edit `State`.
#[derive(Default)]
struct GrabUpdate {
	/// The new pose while actively grabbed, to write back into `State`.
	new_pose: Option<(Vec3, Quat)>,
	started: bool,
	stopped: bool,
}

pub struct GrabbableInner {
	input: InputQueue,
	grab_action: SingleAction,

	max_distance: f32,
	pointer_mode: PointerMode,

	// transient interaction state, only meaningful during an active grab
	relative_transform: Affine3A,
	prev_pose: Affine3A,
}
impl GrabbableInner {
	fn handle_events(&mut self, current: Affine3A) -> GrabUpdate {
		if !self.input.handle_events() {
			return GrabUpdate::default();
		}
		let max_distance = self.max_distance;
		self.grab_action.update(
			true,
			&self.input,
			|snap| match snap.input() {
				InputDataType::Hand { data } => {
					data.thumb.tip.distance < max_distance && data.index.tip.distance < max_distance
				}
				_ => snap.distance() < max_distance,
			},
			grab_pinch_interact,
		);

		let started = self.grab_action.actor_started();
		// (re)anchor the grab against the *current* (state-owned) pose
		if started || self.grab_action.actor_changed() {
			let actor = self.grab_action.actor().unwrap();
			let grab_pose = Affine3A::from_rotation_translation(
				snap_grab_rotation(actor),
				snap_grab_position(actor),
			);
			self.relative_transform = grab_pose.inverse() * current;
			self.prev_pose = current;
		}

		let mut update = GrabUpdate {
			started,
			..Default::default()
		};

		if let Some(actor) = self.grab_action.actor().cloned() {
			if matches!(actor.input(), InputDataType::Pointer { .. }) {
				let scroll_amount = actor.datamap_vec2("scroll_continuous").y * 0.01
					+ actor.datamap_vec2("scroll_discrete").y * 0.01;
				let offset = Affine3A::from_translation(vec3(0.0, 0.0, -scroll_amount));
				self.relative_transform = offset * self.relative_transform;
			}

			let current_grab_pose = Affine3A::from_rotation_translation(
				snap_grab_rotation(&actor),
				snap_grab_position(&actor),
			);

			let new_pose = match (actor.input(), self.pointer_mode) {
				(InputDataType::Pointer { data }, PointerMode::Align) => {
					let parent_pose = current_grab_pose * self.relative_transform;
					let (_, _, parent_translation) = parent_pose.to_scale_rotation_translation();
					let swing_rotation = swing_direction(Vec3::from(data.direction()));
					Affine3A::from_rotation_translation(swing_rotation, parent_translation)
				}
				(InputDataType::Pointer { .. }, PointerMode::Move) => {
					let parent_pose = current_grab_pose * self.relative_transform;
					let offset_rotation = parent_pose.to_scale_rotation_translation().1
						* self.prev_pose.to_scale_rotation_translation().1.inverse();
					parent_pose * Affine3A::from_quat(offset_rotation.inverse())
				}
				_ => current_grab_pose * self.relative_transform,
			};

			// kept across frames for PointerMode::Move's rotation offset
			self.prev_pose = new_pose;

			let (_, rot, pos) = new_pose.to_scale_rotation_translation();
			update.new_pose = Some((pos, rot));
		}

		if self.grab_action.actor_stopped() {
			update.stopped = true;
			self.relative_transform = Affine3A::IDENTITY;
		}

		update
	}
}

fn swing_direction(direction: Vec3) -> Quat {
	let pitch = direction.y.asin();
	let yaw = direction.z.atan2(direction.x);
	Quat::from_rotation_y(-yaw - PI / 2.0) * Quat::from_rotation_x(pitch)
}

fn snap_grab_position(snap: &InputSnapshot) -> Vec3 {
	match snap.input() {
		InputDataType::Pointer { data } => Vec3::from(data.pose.position),
		InputDataType::Hand { data } => Vec3::from(data.palm.pose.position),
		InputDataType::Tip { data } => Vec3::from(data.pose.position),
	}
}

fn snap_grab_rotation(snap: &InputSnapshot) -> Quat {
	match snap.input() {
		InputDataType::Pointer { data } => Quat::from(data.pose.orientation),
		InputDataType::Hand { data } => Quat::from(data.palm.pose.orientation),
		InputDataType::Tip { data } => Quat::from(data.pose.orientation),
	}
}

#[tokio::test]
async fn asteroids_grabbable_element() {
	use crate::{
		Context, Entity, Tasker, Transformable,
		client::{self, ClientState},
		custom::CustomElement,
	};
	use glam::Quat;
	use mint::Vector3;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{fields::Shape, types::rgba_linear};
	use stardust_xr_molecules::lines::LineExt as _;

	#[derive(Debug, Serialize, Deserialize)]
	struct TestState {
		pos: Vector3<f32>,
		rot: Quaternion<f32>,
		grabbed: bool,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				pos: [0.0, 0.5, 0.0].into(),
				rot: Quat::IDENTITY.into(),
				grabbed: false,
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.grabbable";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			let shape = Shape::Box {
				size: [0.1; 3].into(),
			};
			// one Entity owning the shared spatial+field; Grabbable is a component on it,
			// and the visual Lines hang off the same shared spatial as a child element.
			Entity::new(shape.clone())
				.pos(self.pos)
				.rot(self.rot)
				.component(
					Grabbable::new(self.pos, self.rot, |state: &mut Self, pos, rot| {
						state.pos = pos;
						state.rot = rot;
					})
					.grab_start(|state: &mut Self| {
						state.grabbed = true;
					})
					.grab_stop(|state: &mut Self| {
						state.grabbed = false;
					})
					.pointer_mode(PointerMode::Align),
				)
				.build()
				.child(
					crate::elements::Lines::new(
						stardust_xr_molecules::lines::shape(shape.clone())
							.into_iter()
							.map(|l| {
								l.color(if self.grabbed {
									rgba_linear!(0.0, 1.0, 0.5, 1.0)
								} else {
									rgba_linear!(1.0, 1.0, 1.0, 1.0)
								})
								.thickness(if self.grabbed { 0.01 } else { 0.005 })
							}),
					)
					.build(),
				)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
