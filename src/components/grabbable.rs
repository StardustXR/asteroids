use crate::{
	CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState,
	custom::derive_setters::Setters,
};
use derive_where::derive_where;
use glam::{Affine3A, Quat, Vec3, vec3};
use stardust_xr_fusion::{
	Error, Result,
	client::FrameInfo,
	spatial::Transform,
	suis::InputDataType,
	types::Posef,
};
use stardust_xr_molecules::input_action::{
	InputQueue, InputSnapshot, SingleAction, grab_pinch_interact,
};
use std::f32::consts::PI;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerMode {
	Parent,
	Align,
	Move,
}

/// Called every frame the pose changes while grabbed, with the new pose.
type OnChangePose<State> = CloneFnWrapper<dyn Fn(&mut State, Posef) + Send + Sync>;
type GrabStart<State> = CloneFnWrapper<dyn Fn(&mut State) + Send + Sync>;
type GrabStop<State> = CloneFnWrapper<dyn Fn(&mut State) + Send + Sync>;

#[derive_where(Debug, Clone)]
#[derive(Setters)]
#[setters(into)]
pub struct Grabbable<State: ValidState> {
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
	pub fn new<F: Fn(&mut State, Posef) + Send + Sync + 'static>(on_change: F) -> Self {
		Grabbable {
			on_change_pose: CloneFnWrapper(Arc::new(on_change)),
			grab_start: CloneFnWrapper(Arc::new(|_| ())),
			grab_stop: CloneFnWrapper(Arc::new(|_| ())),
			max_distance: 0.05,
			pointer_mode: PointerMode::Parent,
		}
	}

	pub fn grab_start<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
		self.grab_start = CloneFnWrapper(Arc::new(f));
		self
	}
	pub fn grab_stop<F: Fn(&mut State) + Send + Sync + 'static>(mut self, f: F) -> Self {
		self.grab_stop = CloneFnWrapper(Arc::new(f));
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
		// input is reported relative to the anchor, not the moving entity spatial, otherwise
		// dragging fights itself. the anchor rather than parent_space so a containment
		// doesn't split the two apart
		let input = InputQueue::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.field.clone(),
			info.anchor_space.clone(),
		)
		.await?;

		Ok(GrabbableInner {
			input,
			grab_action: SingleAction::default(),
			max_distance: self.max_distance,
			pointer_mode: self.pointer_mode,
			pose: pose(info.transform),
			relative_transform: Affine3A::IDENTITY,
			prev_pose: Affine3A::IDENTITY,
		})
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		inner.max_distance = self.max_distance;
		inner.pointer_mode = self.pointer_mode;
		// the entity transform is where the state-owned pose came back to us, so a grab starts
		// from there rather than from wherever the last one ended
		inner.pose = pose(info.transform);
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		let update = inner.handle_events();

		if let Some(pose) = update.new_pose {
			(self.on_change_pose.0)(state, pose);
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
	new_pose: Option<Posef>,
	started: bool,
	stopped: bool,
}

pub struct GrabbableInner {
	input: InputQueue,
	grab_action: SingleAction,

	max_distance: f32,
	pointer_mode: PointerMode,

	pose: Posef,

	// transient interaction state, only meaningful during an active grab
	relative_transform: Affine3A,
	prev_pose: Affine3A,
}
impl GrabbableInner {
	pub fn grabbing(&self) -> bool {
		self.grab_action.actor_acting()
	}

	fn handle_events(&mut self) -> GrabUpdate {
		if !self.input.handle_events() {
			return GrabUpdate::default();
		}
		let current = Affine3A::from_rotation_translation(
			Quat::from(self.pose.orientation),
			Vec3::from(self.pose.position),
		);
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
			self.pose = Posef {
				position: pos.into(),
				orientation: rot.into(),
			};
			update.new_pose = Some(self.pose);
		}

		if self.grab_action.actor_stopped() {
			update.stopped = true;
			self.relative_transform = Affine3A::IDENTITY;
		}

		update
	}
}

/// the entity's scale is its own business, a grab only ever moves it around
fn pose(transform: &Transform) -> Posef {
	Posef {
		position: transform.translation,
		orientation: transform.rotation,
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
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::{fields::Shape, types::rgba_linear};
	use stardust_xr_molecules::lines::LineExt as _;

	#[derive(Debug, Serialize, Deserialize)]
	struct TestState {
		pose: Posef,
		grabbed: bool,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				pose: Posef {
					position: [0.0, 0.5, 0.0].into(),
					..Default::default()
				},
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
				.pose(self.pose)
				.component(
					Grabbable::new(|state: &mut Self, pose| {
						state.pose = pose;
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
