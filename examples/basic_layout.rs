use glam::Quat;
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Entity, Migrate, Reify, Tasker, Transformable,
	client,
	components::{Containable, Derezzable, Poseable},
	ease_in_out,
	elements::{Button, Lines, Model, Spatial, Text},
	project_local_resources, spring,
};
use stardust_xr_fusion::{
	drawable::{XAlign, YAlign},
	fields::Shape,
	types::{
		Posef,
		color::{Deg, Hsv, ToRgba},
	},
};
use stardust_xr_molecules::{
	DebugSettings,
	lines::{self, LineExt},
};
use std::f32::consts::FRAC_PI_2;
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let registry = tracing_subscriber::registry();
	let registry = registry.with(
		tracing_tracy::TracyLayer::new(tracing_tracy::DefaultConfig::default())
			.with_filter(LevelFilter::TRACE),
	);
	let log_layer = tracing_subscriber::fmt::Layer::new()
		.with_thread_names(true)
		.with_line_number(true)
		.with_filter(EnvFilter::from_default_env());
	registry.with(log_layer).init();

	client::run::<State>(&[&project_local_resources!("data")])
		.await
		.unwrap()
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
	pressed_time: f32,
	text: String,
	pose: Posef,
}
impl Default for State {
	fn default() -> Self {
		State {
			pressed_time: -10000.0,
			text: "triangle :D".to_string(),
			pose: Posef::default(),
		}
	}
}
impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "org.asteroids.BasicLayout";

	fn initial_state_update(&mut self) {
		println!("Initial state yippee");
	}
}
impl Reify for State {
	fn reify(
		&self,
		context: &Context,
		_tasks: impl Tasker<Self>,
		_props: (),
	) -> impl Element<Self> {
		let time = context.time();
		let pressed = time - self.pressed_time <= 1.0;
		Entity::new(Shape::Sphere { radius: 0.05 })
			.pos(self.pose.position)
			.rot(self.pose.orientation)
			.component(Derezzable::program_stopper(context))
			.component(Containable::default())
			.component(Poseable::new(|state: &mut Self, pose| {
				state.pose = pose;
			}))
			.build()
			.child(
				Spatial::default()
					.pos([
						context.time().sin() * 0.1,
						0.0,
						context.time().cos() * 0.1,
					])
					.build()
					.child(Model::namespaced(Self::APP_ID, "grabbable").build())
					.child(
						Button::new(move |state: &mut State| {
							state.text = "button press".to_string();
							state.pressed_time = time;
						})
						.size(if pressed { [0.0; 2] } else { [0.15, 0.3] })
						.debug(DebugSettings::default())
						.build()
						.animation(|a| a.size(ease_in_out(0.1))),
					)
					.child(
						Text::new(&self.text)
							.pos([
								0.0,
								if pressed { -0.3 } else { -0.2 },
								0.0,
							])
							.align_x(XAlign::Center)
							.align_y(YAlign::Top)
							.character_height(0.1)
							.build()
							.animation(|a| a.transform(spring(0.1, 0.4))),
					)
					.children(make_triangles(0.3, 25, 0.01)),
			)
	}
}

fn make_triangles(
	size: f32,
	triangle_count: usize,
	spacing: f32,
) -> impl IntoIterator<Item = impl Element<State>> {
	let half_spacing = triangle_count as f32 * spacing * 0.5;
	(0..triangle_count).map(move |n| {
		let f = n as f32;
		let offset = f * spacing - half_spacing;
		let turns = f / triangle_count as f32;
		let color = turns.map_range(0.0..1.0, 130.0..180.0);

		let lines = lines::circle(3, 0.0, size)
			.thickness(0.01)
			.color(Hsv::new(Deg(color), 1.0, 1.0).to_rgba());
		Lines::new([lines])
			.pos([
				0.0, 0.0, offset,
			])
			.rot(Quat::from_rotation_x(-FRAC_PI_2))
			.build()
	})
}
