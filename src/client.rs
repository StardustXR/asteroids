use crate::{ValidState, View};
use stardust_xr_fusion::{
    client::Client,
    core::schemas::flex::flexbuffers,
    node::{MethodResult, NodeResult},
    root::{ClientState, FrameInfo, RootAspect, RootEvent},
    ClientHandle,
};
use std::sync::Arc;

pub struct StardustClient<State: ValidState> {
    client: Arc<ClientHandle>,
    pub state: State,
    view: View<State>,
}
impl<State: ValidState> StardustClient<State> {
    pub async fn new(
        client: &mut Client,
        initial_state: impl FnOnce() -> State,
    ) -> NodeResult<StardustClient<State>> {
        let raw_state = client
            .with_event_loop(client.handle().get_root().get_state())
            .await??;
        let state = raw_state
            .data
            .as_ref()
            .and_then(|m| flexbuffers::from_slice(m).ok())
            .unwrap_or_else(initial_state);
        let view = View::new(&state, client.get_root());
        Ok(StardustClient {
            client: client.handle(),
            state,
            view,
        })
    }

    pub fn event_loop_update<F: FnMut(&mut State, &FrameInfo)>(&mut self, mut on_frame: F) {
        while let Some(root_event) = self.client.get_root().recv_root_event() {
            match root_event {
                RootEvent::Frame { info } => {
                    (on_frame)(&mut self.state, &info);
                    self.update();
                }
                RootEvent::SaveState { response } => response.wrap(|| self.save_state()),
            }
        }
    }
    pub fn update(&mut self) {
        self.view.update(&mut self.state);
    }
    pub fn save_state(&mut self) -> MethodResult<ClientState> {
        ClientState::from_data_root(
            Some(flexbuffers::to_vec(&self.state)?),
            self.client.get_root(),
        )
    }
}
