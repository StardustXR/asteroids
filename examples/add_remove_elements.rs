use std::f32::consts::PI;

use asteroids::{make_stardust_client, Button, Element, ElementTrait, Spatial, Text, Transformable};
use glam::Quat;
use mint::Vector2;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{
    drawable::{XAlign, YAlign},
    spatial::Transform,
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub struct State {
    list: Vec<String>,
}
impl Default for State {
    fn default() -> Self {
        State {
            list: vec!["List Item 1".to_string()],
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
        .init();
    make_stardust_client::<State>(|_, _| (), make_internals).await
}

fn make_button(
    on_click: fn(&mut State),
    size: impl Into<Vector2<f32>>,
    text: &str,
    transform: Transform,
) -> Element<State> {
    let size = size.into();
    Button::new(on_click)
        .transform(transform)
        .size(size)
        .with_children([Text::default()
            .text(text)
            .character_height(size.y)
            .text_align_x(XAlign::Center)
            .text_align_y(YAlign::Center)
            .build()])
}

fn make_internals(state: &State) -> Element<State> {
    Spatial::default().zoneable(true).with_children([
        Spatial::default().with_children(
            state
                .list
                .iter()
                .enumerate()
                .map(|(i, t)| make_list_item(i, t)),
        ),
        make_button(
            |state: &mut State| {
                state
                    .list
                    .push(format!("List item {}", state.list.len() + 1));
            },
            [0.01, 0.01],
            "+",
            Transform::from_translation([0.0, 0.02, 0.0]),
        ),
        make_button(
            |state: &mut State| {
                state.list.pop();
            },
            [0.01, 0.01],
            "-",
            Transform::from_translation([0.02, 0.02, 0.0]),
        ),
    ])
}

fn make_list_item(index: usize, text: &String) -> Element<State> {
    Text::default()
        .text(text)
        .character_height(0.01)
        .pos([
            0.0,
            (index as f32) * -0.0125,
            0.0,
        ])
        .rot(Quat::from_rotation_y(PI))
        .build()
}
