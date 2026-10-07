use crate::{CustomElement, FnWrapper, ValidState};
use futures::FutureExt;
use gluon_ipc::{Handler, Node};
use stardust_xr_fusion::{
	Result,
	spatial::{Spatial, SpatialRef},
	suis::Chirality,
	tracked::{TrackedExt, TrackedGuard, TrackedStateReceiverHandler},
};
use std::{
	fmt::Debug,
	sync::{
		Mutex, OnceLock,
		atomic::{AtomicBool, Ordering},
	},
};
use tokio::task::JoinHandle;

type OnTrackedChanged<State> = FnWrapper<dyn Fn(&mut State, bool) + Send + Sync>;

#[derive(Debug)]
pub struct Tracked<State: ValidState + Debug> {
	name: String,
	on_tracked_changed: OnTrackedChanged<State>,
}
impl<State: ValidState + Debug> Tracked<State> {
	pub fn new(
		name: impl ToString,
		on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static,
	) -> Self {
		Tracked {
			name: name.to_string(),
			on_tracked_changed: FnWrapper(Box::new(on_tracked_changed)),
		}
	}
	pub fn hmd(on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static) -> Self {
		Self::new("stardust-hmd", on_tracked_changed)
	}
	pub fn stage(on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static) -> Self {
		Self::new("stardust-stage", on_tracked_changed)
	}
	pub fn hand_palm(
		chirality: Chirality,
		on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static,
	) -> Self {
		Self::new(
			match chirality {
				Chirality::Left => "stardust-hand/palm/left",
				Chirality::Right => "stardust-hand/palm/right",
			},
			on_tracked_changed,
		)
	}
	pub fn controller_aim(
		chirality: Chirality,
		on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static,
	) -> Self {
		Self::new(
			match chirality {
				Chirality::Left => "stardust-controller/aim/left",
				Chirality::Right => "stardust-controller/aim/right",
			},
			on_tracked_changed,
		)
	}
	pub fn controller_grip(
		chirality: Chirality,
		on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static,
	) -> Self {
		Self::new(
			match chirality {
				Chirality::Left => "stardust-controller/grip/left",
				Chirality::Right => "stardust-controller/grip/right",
			},
			on_tracked_changed,
		)
	}
	pub fn controller_grip_surface(
		chirality: Chirality,
		on_tracked_changed: impl Fn(&mut State, bool) + Send + Sync + 'static,
	) -> Self {
		Self::new(
			match chirality {
				Chirality::Left => "stardust-controller/grip_surface/left",
				Chirality::Right => "stardust-controller/grip_surface/right",
			},
			on_tracked_changed,
		)
	}
}
impl<State: ValidState + Debug> CustomElement<State> for Tracked<State> {
	type Inner = (Spatial, Node<TrackedInner>);
	type Error = stardust_xr_fusion::Error;

	async fn create_inner(
		&self,
		_asteroids_context: &crate::Context,
		info: crate::CreateInnerInfo,
	) -> Result<Self::Inner> {
		let inner = TrackedInner::new(&self.name).await?;
		info.child_space
			.set_parent(inner.spatial_ref.get().unwrap().clone())?;
		Ok((info.child_space.clone(), inner))
	}

	fn diff(&self, old_self: &Self, _context: &crate::Context, inner: &mut Self::Inner) {
		if self.name != old_self.name {
			let name = self.name.clone();
			inner
				.1
				.pending_replacement
				.lock()
				.unwrap()
				.replace(tokio::spawn(async move { TrackedInner::new(&name).await }));
		}
	}

	fn frame(
		&self,
		_context: &crate::Context,
		_info: &stardust_xr_fusion::client::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		let replacement = inner
			.1
			.pending_replacement
			.lock()
			.unwrap()
			.take_if(|p| p.is_finished())
			.and_then(|p| tokio::task::unconstrained(p).now_or_never());
		if let Some(Ok(Ok(replacement))) = replacement {
			_ = inner
				.0
				.set_parent(replacement.spatial_ref.get().unwrap().clone());
			inner.1 = replacement;
		}

		if inner.1.tracked_state_dirty.swap(false, Ordering::AcqRel) {
			(self.on_tracked_changed.0)(state, inner.1.tracked_state.load(Ordering::Acquire));
		}
	}
}

#[derive(Debug, Handler)]
pub struct TrackedInner {
	guard: OnceLock<TrackedGuard>,
	spatial_ref: OnceLock<SpatialRef>,
	tracked_state: AtomicBool,
	tracked_state_dirty: AtomicBool,
	pending_replacement: Mutex<Option<JoinHandle<Result<Node<TrackedInner>>>>>,
}
impl TrackedInner {
	async fn new(name: &str) -> Result<Node<Self>> {
		let tracked = stardust_xr_fusion::tracked::Tracked::binding(name).await?;

		let (inner, inner_ref) = TrackedInner {
			guard: OnceLock::default(),
			spatial_ref: OnceLock::default(),
			tracked_state: AtomicBool::new(false),
			tracked_state_dirty: AtomicBool::new(true),
			pending_replacement: Mutex::default(),
		}
		.to_node()?;
		let (spatial_ref, guard, currently_tracked) = tracked.get(inner_ref.into_proxy()).await?;
		_ = inner.guard.set(guard);
		_ = inner.spatial_ref.set(spatial_ref);
		inner
			.tracked_state
			.store(currently_tracked, Ordering::Release);
		inner.tracked_state_dirty.store(true, Ordering::Release);

		Ok(inner)
	}
}
impl TrackedStateReceiverHandler for TrackedInner {
	async fn tracked(&self, _ctx: gluon_ipc::Context, tracked: bool) {
		self.tracked_state.store(tracked, Ordering::Release);
		self.tracked_state_dirty.store(true, Ordering::Release);
	}
}

#[tokio::test]
async fn asteroids_tracked_element() {
	use crate::{
		Tasker, Transformable,
		client::{self, ClientState},
		elements::{Axes, Spatial, Text},
	};
	use glam::Quat;
	use serde::{Deserialize, Serialize};
	use stardust_xr_fusion::drawable::{XAlign, YAlign};
	use std::f32::consts::FRAC_PI_2;

	#[derive(Debug, Default, Serialize, Deserialize)]
	struct TestState([bool; 10]);
	impl crate::util::Migrate for TestState {
		type Old = Self;
	}
	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.tracked";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &crate::Context,
			_tasks: impl Tasker<Self>,
			_props: (),
		) -> impl crate::Element<Self> {
			let on = |i: usize| move |state: &mut Self, tracked: bool| state.0[i] = tracked;
			Spatial::default().build().children(
				[
					("hmd", Tracked::hmd(on(0))),
					("stage", Tracked::stage(on(1))),
					("left palm", Tracked::hand_palm(Chirality::Left, on(2))),
					("right palm", Tracked::hand_palm(Chirality::Right, on(3))),
					("left aim", Tracked::controller_aim(Chirality::Left, on(4))),
					(
						"right aim",
						Tracked::controller_aim(Chirality::Right, on(5)),
					),
					(
						"left grip",
						Tracked::controller_grip(Chirality::Left, on(6)),
					),
					(
						"right grip",
						Tracked::controller_grip(Chirality::Right, on(7)),
					),
					(
						"left grip surface",
						Tracked::controller_grip_surface(Chirality::Left, on(8)),
					),
					(
						"right grip surface",
						Tracked::controller_grip_surface(Chirality::Right, on(9)),
					),
				]
				.into_iter()
				.zip(self.0)
				.map(|((label, t), tracked)| {
					t.build().maybe_child(tracked.then(|| {
						Axes::default().build().child(
							Text::new(label)
								.pos([0.015, 0.0, 0.0])
								.rot(Quat::from_rotation_x(-FRAC_PI_2))
								.align_x(XAlign::Left)
								.align_y(YAlign::Center)
								.build(),
						)
					}))
				}),
			)
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
