use asteroids::{make_stardust_client, Element, ElementTrait, Lines, Model, Root, Spatial};
use color::rgba_linear;
use mint::Vector3;
use stardust_xr_fusion::core::values::{Color, ResourceID};
use stardust_xr_molecules::lines::{self, LineExt};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let root = Root::<()> {
        on_frame: |_, frame_info| {
            dbg!(frame_info);
        },
    }
    .with_children([
        Model(ResourceID::new_namespaced("asteroids", "grabbable")).build(),
        Spatial::default()
            .pos([0.0, 0.0, 0.04])
            .with_children([
                make_triangle(0.5, [0.0, 0.0, -0.0], rgba_linear!(1.0, 1.0, 1.0, 1.0)),
                make_triangle(0.5, [0.0, 0.0, -0.02], rgba_linear!(1.0, 1.0, 1.0, 0.8)),
                make_triangle(0.5, [0.0, 0.0, -0.04], rgba_linear!(1.0, 1.0, 1.0, 0.6)),
                make_triangle(0.5, [0.0, 0.0, -0.06], rgba_linear!(1.0, 1.0, 1.0, 0.4)),
                make_triangle(0.5, [0.0, 0.0, -0.08], rgba_linear!(1.0, 1.0, 1.0, 0.2)),
            ]),
    ]);

    make_stardust_client(root).await
}

fn make_triangle(size: f32, offset: impl Into<Vector3<f32>>, color: Color) -> Element {
    Spatial::default()
        .pos(offset)
        .with_children([Lines(vec![
            lines::circle(3, 0.0, size)
                .thickness(0.01)
                .color(color),
        ])
        .build()])
}
