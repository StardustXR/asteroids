use std::f32::consts::PI;
use asteroids::{
    make_stardust_client, Element, ElementTrait, Lines, Model, Root, Spatial, Text, Transformable,
};
use color::rgba_linear;
use glam::Quat;
use mint::Vector3;
use stardust_xr_fusion::{
    core::values::Color,
    drawable::{XAlign, YAlign},
};
use stardust_xr_molecules::lines::{self, LineExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let root = Root::<()> {
        on_frame: |_, _frame_info| {
            // dbg!(frame_info);
        },
    }
    .with_children([
        Model::namespaced("asteroids", "grabbable").build(),
        Spatial::default()
            .pos([0.0, 0.0, 0.04])
            .with_children([
                make_triangle(0.5, [0.0, 0.0, -0.0], rgba_linear!(1.0, 1.0, 1.0, 1.0)),
                make_triangle(0.5, [0.0, 0.0, -0.02], rgba_linear!(1.0, 1.0, 1.0, 0.8)),
                make_triangle(0.5, [0.0, 0.0, -0.04], rgba_linear!(1.0, 1.0, 1.0, 0.6)),
                make_triangle(0.5, [0.0, 0.0, -0.06], rgba_linear!(1.0, 1.0, 1.0, 0.4)),
                make_triangle(0.5, [0.0, 0.0, -0.08], rgba_linear!(1.0, 1.0, 1.0, 0.2)),
            ]),
        // yummy text nom nom nom
        Text::default()
            .pos([0.0, -0.4, 0.0])
            .rot(Quat::from_rotation_y(PI))
            .text("THE TRI")
            .text_align_x(XAlign::Center)
            .text_align_y(YAlign::Top)
            .character_height(0.1)
            .build(),
    ]);

    make_stardust_client(root).await
}

fn make_triangle(size: f32, offset: impl Into<Vector3<f32>>, color: Color) -> Element {
    Lines::default()
        .pos(offset)
        .lines([
            lines::circle(3, 0.0, size)
                .thickness(0.01)
                .color(color),
        ])
        .build()
}
