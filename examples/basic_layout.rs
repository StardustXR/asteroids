use asteroids::{
	client::{self, ClientState},
	custom::{ElementTrait, Transformable},
	elements::{Button, Lines, Model, Spatial, Text},
	util::Migrate,
	Element,
};
use glam::Quat;
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{
	drawable::{XAlign, YAlign},
	project_local_resources,
	root::FrameInfo,
	spatial::Transform,
	values::color::{Deg, Hsv, ToRgba},
};
use stardust_xr_molecules::{
	lines::{self, LineExt},
	DebugSettings,
};
use std::f32::consts::PI;
use tracing_subscriber::EnvFilter;

#[tokio::main(flavor = "current_thread")]
async fn main() {
	tracing_subscriber::fmt()
		.compact()
		.with_env_filter(EnvFilter::from_default_env())
		.init();
	client::run::<State>(&[&project_local_resources!("res")]).await
}

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
	elapsed: f32,
	pressed_time: f32,
	text: String,
}
impl Default for State {
	fn default() -> Self {
		State {
			elapsed: 0.0,
			pressed_time: -10000.0,
			text: "triangle :D".to_string(),
		}
	}
}
impl Migrate for State {
	type Old = Self;
}
impl ClientState for State {
	const QUALIFIER: &'static str = "org";
	const ORGANIZATION: &'static str = "asteroids";
	const NAME: &'static str = "basic_layout";

	fn on_frame(&mut self, info: &FrameInfo) {
		self.elapsed = info.elapsed;
	}

	fn reify(&self) -> Element<Self> {
		let model = Model::namespaced("asteroids", "grabbable").build();
		let button = if self.elapsed - self.pressed_time > 1.0 {
			Button::new(|state: &mut State| {
				state.text = "button press".to_string();
				state.pressed_time = state.elapsed;
			})
			.size([0.15, 0.3])
			.debug(DebugSettings::default())
			.build()
		} else {
			Spatial::default().build()
		};

		let triangles = Spatial::default().with_children(make_triangles(0.3, 25, 0.01));

		let text = Text::default()
			.pos([0.0, -0.2, 0.0])
			.rot(Quat::from_rotation_y(PI))
			.text(&self.text)
			.text_align_x(XAlign::Center)
			.text_align_y(YAlign::Top)
			.character_height(0.1)
			.build();

		let bobber = Spatial::default()
			.transform(Transform::from_translation([
				self.elapsed.sin() * 0.1,
				0.0,
				self.elapsed.cos() * 0.1,
			]))
			.with_children([
				model, button, triangles, // yummy text nom nom nom
				text,
			]);

		Spatial::default().zoneable(true).with_children([bobber])
	}
}

fn make_triangles(
	size: f32,
	triangle_count: usize,
	spacing: f32,
) -> impl IntoIterator<Item = Element<State>> {
	let half_spacing = triangle_count as f32 * spacing * 0.5;
	(0..triangle_count).map(move |n| {
		let f = n as f32;
		let offset = f * spacing - half_spacing;
		let turns = f / triangle_count as f32;
		let color = turns.map_range(0.0..1.0, 130.0..180.0);

		let lines = lines::circle(3, 0.0, size)
			.thickness(0.01)
			.color(Hsv::new(Deg(color), 1.0, 1.0).to_rgba());
		Lines::default()
			.pos([
				0.0, 0.0, offset,
			])
			.lines([lines])
			.build()
	})
}
