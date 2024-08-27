use manifest_dir_macros::directory_relative_path;
use stardust_xr_fusion::{
    client::Client,
    core::schemas::flex::flexbuffers,
    node::{MethodResult, NodeType},
    root::{ClientState, FrameInfo, RootAspect, RootHandler},
};
use std::sync::Arc;

use crate::{ElementGenerator, ValidState, View};

pub async fn make_stardust_client<State: ValidState>(
    initial_state: impl FnOnce() -> State,
    on_frame: fn(&mut State, &FrameInfo),
    root: ElementGenerator<State>,
) {
    let (client, event_loop) = Client::connect_with_async_loop().await.unwrap();
    client
        .set_base_prefixes(&[directory_relative_path!("res")])
        .unwrap();

    let asteroids = StardustClient::new(client.clone(), initial_state, on_frame, root);
    let _root = client.get_root().alias().wrap(asteroids).unwrap();

    tokio::select! {
        _ = tokio::signal::ctrl_c() => (),
        _ = event_loop => panic!("server crashed"),
    }
}

pub struct StardustClient<State: ValidState> {
    client: Arc<Client>,
    pub state: State,
    on_frame: fn(&mut State, &FrameInfo),
    view: View<State>,
}

impl<State: ValidState> StardustClient<State> {
    pub fn new(
        client: Arc<Client>,
        initial_state: impl FnOnce() -> State,
        on_frame: fn(&mut State, &FrameInfo),
        generator: ElementGenerator<State>,
    ) -> StardustClient<State> {
        let state = client
            .get_state()
            .data
            .as_ref()
            .and_then(|m| flexbuffers::from_slice(m).ok())
            .unwrap_or_else(initial_state);
        let view = View::new(generator, &state, client.get_root());
        StardustClient {
            client,
            state,
            on_frame,
            view,
        }
    }
}
impl<State: ValidState> RootHandler for StardustClient<State> {
    fn frame(&mut self, info: FrameInfo) {
        (self.on_frame)(&mut self.state, &info);
        self.view.update(&mut self.state);
    }

    fn save_state(&mut self) -> MethodResult<ClientState> {
        ClientState::from_data_root(
            Some(flexbuffers::to_vec(&self.state)?),
            self.client.get_root(),
        )
    }
}
