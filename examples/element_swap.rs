use asteroids::{ElementTrait, Spatial, StardustClient, Text, ValidState};
use manifest_dir_macros::directory_relative_path;
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::client::Client;
use tracing_subscriber::EnvFilter;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    elapsed: f32,
}
impl ValidState for State {
    fn reify(&self) -> asteroids::Element<Self> {
        // every odd second
        if self.elapsed % 2.0 > 1.0 {
            Spatial::default().build()
        } else {
            Text::default().build()
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

    let _asteroids = StardustClient::new(client.clone(), State::default, |state, frame_info| {
        state.elapsed = frame_info.elapsed;
    })
    .unwrap();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => (),
        _ = event_loop => panic!("server crashed"),
    }
}
