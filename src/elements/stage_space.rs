use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{
	Error,
	tracked::{Tracked, TrackedExt},
};
use std::fmt::Debug;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StageSpace;
impl<State: ValidState> CustomElement<State> for StageSpace {
	type Inner = ();
	type Error = Error;

	async fn create_inner(
		&self,
		_context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let stage_spatial = Tracked::stage_spatial().await?;
		info.child_space.set_parent(stage_spatial)?;
		Ok(())
	}
	fn diff(&self, _old_self: &Self, _context: &Context, _inner: &mut Self::Inner) {}
}

#[tokio::test]
async fn asteroids_playspace_element() {
	use crate::{
		Tasker,
		client::{self, ClientState},
		elements::StageSpace,
	};
	use serde::{Deserialize, Serialize};

	#[derive(Default, Serialize, Deserialize)]
	struct TestState;

	impl crate::util::Migrate for TestState {
		type Old = Self;
	}

	impl ClientState for TestState {
		const APP_ID: &'static str = "org.asteroids.playspace";
	}
	impl crate::Reify for TestState {
		fn reify(
			&self,
			_context: &Context,
			_tasks: impl Tasker<Self>,
		) -> impl crate::Element<Self> {
			StageSpace
				.build()
				.child(crate::elements::Lines::new([crate::elements::circle(4, 0.0, 0.1)]).build())
		}
	}

	client::run::<TestState>(&[]).await.unwrap();
}
