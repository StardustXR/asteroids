use asteroids::{
    client::StardustClient,
    custom::ElementTrait,
    elements::{Spatial, Text},
    Reify,
};
use serde::{Deserialize, Serialize};
use stardust_xr_fusion::{client::Client, project_local_resources};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct State {
    elapsed: f32,
}
impl Reify for State {
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
        .unwrap()
}
