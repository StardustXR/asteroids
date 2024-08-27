use std::f32::consts::PI;
use asteroids::{Button, Element, ElementTrait, Spatial, StardustClient, Text, Transformable};
use derive_setters::Setters;
use glam::Quat;
use manifest_dir_macros::directory_relative_path;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{
    client::Client,
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

    let (client, event_loop) = Client::connect_with_async_loop().await.unwrap();
    client
        .set_base_prefixes(&[directory_relative_path!("res")])
        .unwrap();

    let _asteroids =
        StardustClient::new(client.clone(), State::default, |_, _| (), make_internals).unwrap();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => (),
        _ = event_loop => panic!("server crashed"),
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
                .rot(Quat::from_rotation_y(PI))
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
        .padding(0.0025)
        .label("add")
        .pos([-0.03, 0.02, 0.0])
        .build(),
        LabeledButton::new(|state: &mut State| {
            state.list.pop();
        })
        .height(0.01)
        .padding(0.0025)
        .label("remove")
        .pos([0.03, 0.02, 0.0])
        .build(),
    ])
}

fn make_list_item(index: usize, text: &String) -> Element<State> {
    let size = 0.01;
    let padding = 0.0025;
    Spatial::default()
        .pos([
            0.0,
            (index as f32) * -(size + padding),
            0.0,
        ])
        .with_children([
            Button::new(move |state: &mut State| {
                state.list.remove(index);
            })
            .size([size; 2])
            .pos([-0.05, 0.0, 0.0])
            .build(),
            Text::default()
                .text("-")
                .character_height(size)
                .text_align_x(XAlign::Center)
                .pos([-0.05, 0.0, 0.0])
                .rot(Quat::from_rotation_y(PI))
                .build(),
            Text::default()
                .text(text)
                .character_height(size)
                .text_align_x(XAlign::Left)
                .rot(Quat::from_rotation_y(PI))
                .build(),
        ])
}
