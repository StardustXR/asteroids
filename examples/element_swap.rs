use asteroids::{make_stardust_client, ElementTrait, Spatial, Text};
use serde::{Deserialize, Serialize};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    elapsed: f32,
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    tracing_subscriber::fmt()
        .compact()
        .with_env_filter(EnvFilter::from_env("LOG_LEVEL"))
        .init();
    make_stardust_client::<State>(
        |state, frame_info| {
            state.elapsed = frame_info.elapsed;
        },
        |state| {
            // every odd second
            if state.elapsed % 2.0 > 1.0 {
                Spatial::default().build()
            } else {
                Text::default().build()
            }
        },
    )
    .await
}
