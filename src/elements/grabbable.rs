use crate::ValidState;
use crate::custom::{CustomElement, FnWrapper};
use derive_setters::Setters;
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	fields::{Field, FieldAspect, Shape},
	node::NodeError,
	root::FrameInfo,
	spatial::{SpatialAspect, SpatialRef, Transform},
};
use stardust_xr_molecules::{FrameSensitive, GrabbableSettings, UIElement};

pub use stardust_xr_molecules::{MomentumSettings, PointerMode};

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into)]
pub struct Grabbable<State: ValidState> {
	#[setters(skip)]
	pos: Vector3<f32>,
	#[setters(skip)]
	rot: Quaternion<f32>,
	#[setters(skip)]
	field_shape: Shape,
	field_transform: Transform,
	#[setters(skip)]
	#[allow(clippy::type_complexity)]
	on_change_pose: FnWrapper<dyn Fn(&mut State, Vector3<f32>, Quaternion<f32>) + Send + Sync>,
	#[setters(skip)]
	grab_start: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
	#[setters(skip)]
	grab_stop: FnWrapper<dyn Fn(&mut State) + Send + Sync>,
	/// Max distance that you can be to start grabbing
	max_distance: f32,
	/// None means no linear momentum.
	linear_momentum: Option<MomentumSettings>,
	/// None means no angular momentum.
	angular_momentum: Option<MomentumSettings>,
	/// Should the grabbable be magnetized to the grab point?
	magnet: bool,
	/// How should pointers be handled?
	pointer_mode: PointerMode,
	/// Should the object be movable by zones?
	zoneable: bool,
}
impl<State: ValidState> Grabbable<State> {
	pub fn new<F: Fn(&mut State, Vector3<f32>, Quaternion<f32>) + Send + Sync + 'static>(
		field_shape: Shape,
		pos: impl Into<Vector3<f32>>,
		rot: impl Into<Quaternion<f32>>,
		on_change: F,
	) -> Self {
		Grabbable {
			field_shape,
			field_transform: Transform::identity(),
			pos: pos.into(),
			rot: rot.into(),
			on_change_pose: FnWrapper(Box::new(on_change)),
			grab_start: FnWrapper(Box::new(|_| ())),
			grab_stop: FnWrapper(Box::new(|_| ())),
			max_distance: 0.05,
			linear_momentum: Some(MomentumSettings {
				drag: 8.0,
				threshold: 0.01,
			}),
			angular_momentum: Some(MomentumSettings {
				drag: 15.0,
				threshold: 0.2,
			}),
			magnet: true,
			pointer_mode: PointerMode::Parent,
			zoneable: true,
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
impl<State: ValidState> CustomElement<State> for Grabbable<State> {
	type Inner = stardust_xr_molecules::Grabbable;
	type Resource = ();
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &crate::Context,
		info: crate::CreateInnerInfo,
		_resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		let field = Field::create(
			info.parent_space,
			self.field_transform,
			self.field_shape.clone(),
		)?;
		let grabbable = stardust_xr_molecules::Grabbable::create(
			info.parent_space,
			Transform::from_translation_rotation(self.pos, self.rot),
			&field,
			GrabbableSettings {
				max_distance: self.max_distance,
				linear_momentum: self.linear_momentum,
				angular_momentum: self.angular_momentum,
				magnet: self.magnet,
				pointer_mode: self.pointer_mode,
				zoneable: self.zoneable,
			},
		)?;
		field.set_spatial_parent(&grabbable.content_parent())?;
		Ok(grabbable)
	}

	fn diff(&self, old_self: &Self, inner: &mut Self::Inner, _resource: &mut Self::Resource) {
		if self.field_shape != old_self.field_shape {
			let _ = inner.field().set_shape(self.field_shape.clone());
		}
		if self.field_transform != old_self.field_transform {
			let _ = inner.field().set_local_transform(self.field_transform);
		}
		if (self.pos, self.rot) != inner.pose() {
			inner.set_pose(self.pos, self.rot);
		}
	}

	fn frame(&self, info: &FrameInfo, state: &mut State, inner: &mut Self::Inner) {
		if inner.handle_events() {
			let (pos, rot) = inner.pose();
			(self.on_change_pose.0)(state, pos, rot)
		}
		inner.frame(info);

		if inner.grab_action().actor_started() {
			(self.grab_start.0)(state);
		}
		if inner.grab_action().actor_stopped() {
			(self.grab_stop.0)(state);
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.content_parent()
	}
}

#[tokio::test]
async fn asteroids_grabbable_element() {
	use crate::{
		Transformable,
		client::{self, ClientState},
		elements::{Grabbable, Spatial},
	};
	use glam::Quat;
	use mint::Vector3;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::values::color::rgba_linear;
	use stardust_xr_molecules::lines::LineExt as _;

	#[derive(Debug, Serialize, Deserialize)]
	struct TestState {
		pos: Vector3<f32>,
		rot: Quaternion<f32>,
		grabbed: bool,
		second: Option<Box<TestState>>,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				pos: [0.0; 3].into(),
				rot: Quat::IDENTITY.into(),
				grabbed: false,
				second: Some(Box::new(TestState {
					pos: [0.0; 3].into(),
					rot: Quat::IDENTITY.into(),
					grabbed: false,
					second: None,
				})),
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.grabbable";

		fn initial_state_update(&mut self) {
			self.second = Some(Box::new(TestState {
				pos: [0.0; 3].into(),
				rot: Quat::IDENTITY.into(),
				grabbed: false,
				second: None,
			}));
		}
	}
	impl crate::Reify for TestState {
		fn reify(&self) -> impl crate::Element<Self> {
			let shape = Shape::Box([0.1; 3].into());
			Spatial::default().pos([0.0, 0.5, 0.0]).build().child(
				Grabbable::new(
					shape.clone(),
					self.pos,
					self.rot,
					|state: &mut Self, pos, rot| {
						state.pos = pos;
						state.rot = rot;
					},
				)
				.grab_start(|state: &mut Self| {
					state.grabbed = true;
				})
				.grab_stop(|state: &mut Self| {
					state.grabbed = false;
					// state.pos = [0.0; 3].into();
				})
				.pointer_mode(PointerMode::Align)
				.linear_momentum(None)
				.angular_momentum(None)
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
				.maybe_child(
					self.second
						.as_ref()
						.map(|s| s.reify_substate(|state: &mut Self| state.second.as_deref_mut())),
				),
			)
		}
	}

	client::run::<TestState>(&[]).await
}
