use asteroids::{
	client::{self, ClientState},
	custom::ElementTrait,
	elements::{Spatial, Text},
};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::project_local_resources;
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt()
		.compact()
		.with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
		.init();
	client::run(State::default, &[&project_local_resources!("res")]).await
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
	elapsed: f32,
}
impl ClientState for State {
	fn on_frame(&mut self, info: &stardust_xr_fusion::root::FrameInfo) {
		self.elapsed = info.elapsed;
	}

	fn reify(&self) -> asteroids::Element<Self> {
		// every odd second
		if self.elapsed % 2.0 > 1.0 {
			Spatial::default().build()
		} else {
			Text::default().build()
		}
	}
}
