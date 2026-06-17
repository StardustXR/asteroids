use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::project_local_resources;
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Migrate, Reify, Tasker, client, elements::Lines,
};
use stardust_xr_fusion::fields::Shape;
use stardust_xr_fusion::types::rgba_linear;
use stardust_xr_molecules::lines::{LineExt, shape};
use std::time::Duration;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let registry = tracing_subscriber::registry();
	let registry = registry.with(
		tracing_tracy::TracyLayer::new(tracing_tracy::DefaultConfig::default())
			.with_filter(LevelFilter::DEBUG),
	);
	let log_layer = tracing_subscriber::fmt::Layer::new()
		.with_thread_names(true)
		.with_ansi(true)
		.with_line_number(true)
		.with_filter(EnvFilter::from_default_env());
	registry.with(log_layer).init();

	client::run::<State>(&[&project_local_resources!("data")])
		.await
		.unwrap()
}

#[derive(Default, Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
	thingy: bool,
}
impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "org.stardustxr.asteroids.TaskTest";

	fn on_start(&mut self, _context: &Context, tasks: impl Tasker<Self>) {
		self.thingy = false;
		tasks.spawn(tokio::time::sleep(Duration::from_secs(5)), |_, state, _| {
			state.thingy = true;
			println!("async time elapsed");
		});
	}
}
impl Reify for State {
	fn reify(&self, _context: &Context, _tasks: impl Tasker<Self>) -> impl Element<Self> {
		Lines::new(
			shape(Shape::Box {
				size: [0.1; 3].into(),
			})
			.into_iter()
			.map(|line| {
				line.color(if self.thingy {
					rgba_linear!(0.25, 1.0, 0.25, 1.0)
				} else {
					rgba_linear!(1.0, 0.25, 0.25, 1.0)
				})
			}),
		)
		.build()
	}
}
