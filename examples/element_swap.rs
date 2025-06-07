use asteroids::{
	ClientState, CustomElement, Element, Migrate, Transformable, client,
	elements::{Spatial, Text},
};
use glam::Quat;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::project_local_resources;
use std::f32::consts::PI;
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt()
		.compact()
		.with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
		.init();
	client::run::<State>(&[&project_local_resources!("res")]).await
}

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
	elapsed: f32,
}
impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "org.asteroids.element_swap";

	fn on_frame(&mut self, info: &stardust_xr_fusion::root::FrameInfo) {
		self.elapsed = info.elapsed;
	}

	fn reify(&self) -> Element<Self> {
		// every odd second
		let odd_second = self.elapsed % 2.0 > 1.0;
		let text = Text::default()
			.text(if odd_second {
				"Spatial root"
			} else {
				"Text root"
			})
			.character_height(0.02)
			.rot(Quat::from_rotation_y(PI))
			.build();

		if odd_second {
			Spatial::default().build().child(text)
		} else {
			text
		}
	}
}
