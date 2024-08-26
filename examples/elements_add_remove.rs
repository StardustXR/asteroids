use std::f32::consts::PI;

use asteroids::{make_stardust_client, Button, Element, ElementTrait, Spatial, Text, Transformable};
use derive_setters::Setters;
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
            list: vec!["List Item 0".to_string()],
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
            height: 0.0,
            label: String::new(),
            transform: Transform::identity(),
        }
    }
    fn build(self) -> Element<State> {
        let padding = self.padding * 2.0;
        Button::new(self.on_click)
            .transform(self.transform)
            .size([
                padding + (self.label.len() as f32 * self.height),
                padding + self.height,
            ])
            .with_children([Text::default()
                .text(&self.label)
                .character_height(self.height)
                .text_align_x(XAlign::Center)
                .text_align_y(YAlign::Center)
                .build()])
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

fn make_internals(state: &State) -> Element<State> {
    Spatial::default().zoneable(true).with_children([
        Spatial::default().with_children(
            state
                .list
                .iter()
                .enumerate()
                .map(|(i, t)| make_list_item(i, t)),
        ),
        LabeledButton::new(|state: &mut State| {
            state
                .list
                .push(format!("List item {}", state.list.len()));
        })
        .height(0.01)
        .label("+")
        .pos([0.0, 0.02, 0.0])
        .build(),
        LabeledButton::new(|state: &mut State| {
            state.list.pop();
        })
        .height(0.01)
        .label("-")
        .pos([0.02, 0.02, 0.0])
        .build(),
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
