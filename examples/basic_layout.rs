use std::f32::consts::PI;
use asteroids::{
    make_stardust_client, Element, ElementTrait, Lines, Model, Root, Spatial, Text, Transformable,
    Button,
};
use color::ToRgba;
use glam::Quat;
use map_range::MapRange;
use stardust_xr_fusion::drawable::{XAlign, YAlign};
use stardust_xr_molecules::{
    lines::{self, LineExt},
    DebugSettings,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let root = Root::<()> {
        on_frame: |_, _frame_info| {
            // dbg!(frame_info);
        },
    }
    .with_children([Spatial::default()
        .zoneable(true)
        .with_children([
            Model::namespaced("asteroids", "grabbable").build(),
            Button::default()
                .size([0.15, 0.3])
                .debug(DebugSettings::default())
                .build(),
            Spatial::default().with_children(make_triangles(0.3, 25, 0.01)),
            // yummy text nom nom nom
            Text::default()
                .pos([0.0, -0.2, 0.0])
                .rot(Quat::from_rotation_y(PI))
                .text("triangle :D")
                .text_align_x(XAlign::Center)
                .text_align_y(YAlign::Top)
                .character_height(0.1)
                .build(),
        ])]);

    make_stardust_client(root).await
}

fn make_triangles(
    size: f32,
    triangle_count: usize,
    spacing: f32,
) -> impl IntoIterator<Item = Element> {
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
                    .color(color::Hsv::new(color::Deg(color), 1.0, 1.0).to_rgba()),
            ])
            .build()
    })
}
