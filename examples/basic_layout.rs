use std::f32::consts::PI;
use asteroids::{
    client::StardustClient,
    custom::{ElementTrait, Transformable},
    elements::{Button, Lines, Model, Spatial, Text},
    Element, Reify,
};
use glam::{vec3, Quat};
use map_range::MapRange;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{
    client::Client,
    drawable::{XAlign, YAlign},
    project_local_resources,
    spatial::Transform,
    values::color::{Deg, Hsv, ToRgba},
};
use stardust_xr_molecules::{
    lines::{self, LineExt},
    DebugSettings,
};
use tracing_subscriber::EnvFilter;

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
impl Reify for State {
    fn reify(&self) -> Element<Self> {
        Spatial::default()
            .zoneable(true)
            .with_children([Spatial::default()
                .transform(Transform::from_translation(
                    vec3(self.elapsed.sin(), 0.0, self.elapsed.cos()) * 0.1,
                ))
                .with_children(make_internals(self))])
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_default_env())
        .init();
    let mut client = Client::connect().await.unwrap();
    client
        .setup_resources(&[&project_local_resources!("res")])
        .unwrap();

    let mut asteroids = StardustClient::new(&mut client, State::default)
        .await
        .unwrap();
    client
        .event_loop(|_, _| {
            asteroids.event_loop_update(|state, info| {
                state.elapsed = info.elapsed;
            })
        })
        .await
        .unwrap();
}

fn make_internals(state: &State) -> Vec<Element<State>> {
    vec![
        Model::namespaced("asteroids", "grabbable").build(),
        if state.elapsed - state.pressed_time > 1.0 {
            Button::new(|state: &mut State| {
                state.text = "button press".to_string();
                state.pressed_time = state.elapsed;
            })
            .size([0.15, 0.3])
            .debug(DebugSettings::default())
            .build()
        } else {
            Spatial::default().build()
        },
        Spatial::default().with_children(make_triangles(0.3, 25, 0.01)),
        // yummy text nom nom nom
        Text::default()
            .pos([0.0, -0.2, 0.0])
            .rot(Quat::from_rotation_y(PI))
            .text(&state.text)
            .text_align_x(XAlign::Center)
            .text_align_y(YAlign::Top)
            .character_height(0.1)
            .build(),
    ]
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
        Lines::default()
            .pos([0.0, 0.0, offset])
            .lines([
                lines::circle(3, 0.0, size)
                    .thickness(0.01)
                    .color(Hsv::new(Deg(color), 1.0, 1.0).to_rgba()),
            ])
            .build()
    })
}
