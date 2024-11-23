use crate::{Element, Reify, View};
use serde::{de::DeserializeOwned, Serialize};
use stardust_xr_fusion::{
	core::schemas::flex::flexbuffers,
	root::{FrameInfo, RootAspect, RootEvent},
};
use std::path::Path;

pub trait ClientState: Reify + Default + Serialize + DeserializeOwned {
	fn on_frame(&mut self, info: &FrameInfo);
	fn reify(&self) -> Element<Self>;
}
impl<T: ClientState> Reify for T {
	fn reify(&self) -> Element<Self> {
		<T as ClientState>::reify(self)
	}
}

pub async fn run<State: ClientState>(initial_state: impl FnOnce() -> State, resources: &[&Path]) {
	let Ok(mut client) = stardust_xr_fusion::client::Client::connect().await else {
		return;
	};
	if !resources.is_empty() {
		let _ = client.setup_resources(resources);
	}
	let Ok(Ok(raw_state)) = client
		.await_method(client.handle().get_root().get_state())
		.await
	else {
		return;
	};
	let mut state = raw_state
		.data
		.as_ref()
		.and_then(|m| flexbuffers::from_slice(m).ok())
		.unwrap_or_else(initial_state);
	let mut view = View::new(&state, client.get_root());

	let _ = client
		.sync_event_loop(|client, _| {
			while let Some(root_event) = client.get_root().recv_root_event() {
				match root_event {
					RootEvent::Frame { info } => {
						state.on_frame(&info);
						view.frame(&info);
						view.update(&mut state);
					}
					RootEvent::SaveState { response } => response.wrap(|| {
						stardust_xr_fusion::root::ClientState::from_data_root(
							Some(flexbuffers::to_vec(&state)?),
							client.get_root(),
						)
					}),
				}
			}
		})
		.await;
}
