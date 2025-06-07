use crate::{
	ValidState,
	custom::{ElementTrait, FnWrapper},
};
use derive_setters::Setters;
use glam::Affine3A;
use mint::{Quaternion, Vector3};
use stardust_xr_fusion::{
	fields::{Field, FieldAspect, Shape},
	node::NodeError,
	root::FrameInfo,
	spatial::{SpatialAspect, SpatialRef, Transform},
};
use stardust_xr_molecules::{
	FrameSensitive, GrabbableSettings, MomentumSettings, PointerMode, UIElement,
};

#[derive_where::derive_where(Debug)]
#[derive(Setters)]
#[setters(into, strip_option)]
pub struct Grabbable<State: ValidState> {
	#[setters(skip)]
	pos: Vector3<f32>,
	#[setters(skip)]
	rot: Quaternion<f32>,
	#[setters(skip)]
	field_shape: Shape,
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
impl<State: ValidState> ElementTrait<State> for Grabbable<State> {
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
			Transform::identity(),
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
		field.set_spatial_parent(grabbable.content_parent())?;
		Ok(grabbable)
	}

	fn update(
		&self,
		old_decl: &Self,
		state: &mut State,
		inner: &mut Self::Inner,
		_resource: &mut Self::Resource,
	) {
		if self.field_shape != old_decl.field_shape {
			let _ = inner.field().set_shape(self.field_shape.clone());
		}
		let (_, rot, pos) = inner.pose.to_scale_rotation_translation();
		if self.pos != pos.into() || self.rot != rot.into() {
			let _ = inner
				.content_parent()
				.set_local_transform(Transform::from_translation_rotation(self.pos, self.rot));
		}
		inner.pose = Affine3A::from_rotation_translation(self.rot.into(), self.pos.into());
		if inner.handle_events() {
			let (_, rot, pos) = inner.pose.to_scale_rotation_translation();
			(self.on_change_pose.0)(state, pos.into(), rot.into())
		}

		if inner.grab_action().actor_started() {
			(self.grab_start.0)(state);
		}
		if inner.grab_action().actor_stopped() {
			(self.grab_stop.0)(state);
		}
	}

	fn frame(&self, info: &FrameInfo, _state: &mut State, inner: &mut Self::Inner) {
		inner.frame(info);
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.content_parent().clone().as_spatial_ref()
	}
}

#[tokio::test]
async fn asteroids_grabbable_element() {
	use crate::{
		Element,
		client::{self, ClientState},
		elements::Grabbable,
	};
	use mint::Vector3;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::values::color::rgba_linear;
	use stardust_xr_molecules::lines::LineExt as _;

	#[derive(Serialize, Deserialize)]
	struct TestState {
		pos: Vector3<f32>,
		rot: Quaternion<f32>,
		grabbed: bool,
	}
	impl Default for TestState {
		fn default() -> Self {
			TestState {
				pos: [0.0; 3].into(),
				rot: glam::Quat::IDENTITY.into(),
				grabbed: false,
			}
		}
	}

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.grabbable";

		fn reify(&self) -> Element<Self> {
			let shape = Shape::Box([0.1; 3].into());

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
				state.pos = [0.0; 3].into();
				state.rot = glam::Quat::IDENTITY.into();
			})
			.pointer_mode(PointerMode::Move)
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

	client::run::<TestState>(&[]).await
}
