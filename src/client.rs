use crate::{ElementGenerator, ValidState, View};
use stardust_xr_fusion::{
    client::Client,
    core::schemas::flex::flexbuffers,
    node::{MethodResult, NodeResult, NodeType},
    root::{ClientState, FrameInfo, Root, RootAspect, RootHandler},
    HandlerWrapper,
};
use std::sync::Arc;

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
    ) -> NodeResult<HandlerWrapper<Root, StardustClient<State>>> {
        let state = client
            .get_state()
            .data
            .as_ref()
            .and_then(|m| flexbuffers::from_slice(m).ok())
            .unwrap_or_else(initial_state);
        let view = View::new(generator, &state, client.get_root());
        client.get_root().alias().wrap(StardustClient {
            client,
            state,
            on_frame,
            view,
        })
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
