use derive_setters::Setters;
use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Migrate, Reify, Tasker, Transformable, client,
	elements::{Button, Reparentable, Spatial, Text},
	project_local_resources,
};
use stardust_xr_fusion::{
	drawable::{XAlign, YAlign},
	spatial::Transform,
};
use tracing::level_filters::LevelFilter;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	let registry = tracing_subscriber::registry();
	let registry = registry.with(
		tracing_tracy::TracyLayer::new(tracing_tracy::DefaultConfig::default())
			.with_filter(LevelFilter::TRACE),
	);
	let log_layer = tracing_subscriber::fmt::Layer::new()
		.with_thread_names(true)
		.with_ansi(false)
		.with_line_number(true)
		.with_filter(EnvFilter::from_default_env());
	registry.with(log_layer).init();

	client::run::<State>(&[&project_local_resources!("data")])
		.await
		.unwrap()
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
	list: Vec<String>,
}
impl Default for State {
	fn default() -> Self {
		State {
			list: vec!["List Item 0".to_string()],
		}
	}
}
impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const APP_ID: &'static str = "org.asteroids.ElementsAddRemove";
}
impl Reify for State {
	fn reify(&self, _context: &Context, _tasks: impl Tasker<Self>) -> impl Element<Self> {
		Reparentable::default()
			.build()
			.child(
				LabeledButton::new(|state: &mut State| {
					state.list.push(format!("List item {}", state.list.len()));
				})
				.height(0.01)
				.padding(0.0025)
				.label("add")
				.pos([
					-0.03, 0.02, 0.0,
				])
				.build(),
			)
			.child(
				LabeledButton::new(|state: &mut State| {
					state.list.pop();
				})
				.height(0.01)
				.padding(0.0025)
				.label("remove")
				.pos([0.03, 0.02, 0.0])
				.build(),
			)
			.children(
				self.list
					.iter()
					.enumerate()
					.map(|(i, t)| make_list_item(i, t)),
			)
	}
}

#[derive(Setters)]
#[setters(into)]
struct LabeledButton {
	on_click: fn(&mut State),
	padding: f32,
	height: f32,
	label: String,
	transform: Transform,
}
impl LabeledButton {
	fn new(on_click: fn(&mut State)) -> Self {
		LabeledButton {
			on_click,
			padding: 0.001,
			height: 0.01,
			label: String::new(),
			transform: Transform::IDENTITY,
		}
	}
	fn build(self) -> impl Element<State> {
		let padding = self.padding * 2.0;
		Button::new(self.on_click)
			.transform(self.transform)
			.size([
				padding + (self.label.len() as f32 * self.height),
				padding + self.height,
			])
			.build()
			.child(
				Text::new(&self.label)
					.character_height(self.height)
					.align_x(XAlign::Center)
					.align_y(YAlign::Center)
					.build(),
			)
	}
}
impl Transformable for LabeledButton {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

fn make_list_item(index: usize, text: &String) -> impl Element<State> {
	let size = 0.01;
	let padding = 0.0025;
	Spatial::default()
		.pos([
			0.0,
			(index as f32) * -(size + padding),
			0.0,
		])
		.build()
		.child(
			Button::new(move |state: &mut State| {
				state.list.remove(index);
			})
			.size([size; 2])
			.pos([-0.05, 0.0, 0.0])
			.build(),
		)
		.child(
			Text::new("-")
				.character_height(size)
				.align_x(XAlign::Center)
				.pos([-0.05, 0.0, 0.0])
				.build(),
		)
		.child(
			Text::new(text)
				.character_height(size)
				.align_x(XAlign::Left)
				.build(),
		)
}
