use crate::{
	util::{Migrate, RonFile},
	Element, Reify, View,
};
use convert_case::{Case, Casing};
use serde::{de::DeserializeOwned, Serialize};
use stardust_xr_fusion::{
	core::schemas::flex::flexbuffers,
	objects::connect_client,
	root::{FrameInfo, RootAspect, RootEvent},
	Client,
};
use std::{
	fs::read_to_string,
	path::{Path, PathBuf},
};

pub trait ClientState: Reify + Default + Migrate + Serialize + DeserializeOwned {
	const QUALIFIER: &'static str;
	const ORGANIZATION: &'static str;
	const NAME: &'static str;

	fn on_frame(&mut self, info: &FrameInfo);
	fn reify(&self) -> Element<Self>;
}
impl<T: ClientState> Reify for T {
	fn reify(&self) -> Element<Self> {
		<T as ClientState>::reify(self)
	}
}

fn initial_state<State: ClientState>() -> Option<State> {
	let qualified_name = format!(
		"{}.{}.{}",
		State::QUALIFIER,
		State::ORGANIZATION.to_case(Case::Pascal),
		State::NAME.to_case(Case::Pascal)
	);

	// this is a dumb heuristic for determining if it's installed or not, may wanna replace
	#[cfg(debug_assertions)]
	let initial_state_path = PathBuf::from("/tmp/asteroids_config").join(qualified_name + ".ron");
	#[cfg(not(debug_assertions))]
	let initial_state_path = directories::BaseDirs::new()?
		.config_dir()
		.join(qualified_name)
		.join("initial_state.ron");
	let initial_state_string = RonFile(read_to_string(initial_state_path).ok()?);
	State::deserialize_with_migrate(&initial_state_string).ok()
}

async fn state<State: ClientState>(client: &mut Client) -> Option<State> {
	let saved_state = client
		.await_method(client.handle().get_root().get_state())
		.await
		.ok()?
		.ok()?;

	let state = saved_state
		.data
		.as_ref()
		.and_then(|m| flexbuffers::from_slice(m).ok())
		.or_else(initial_state)
		.unwrap_or_default();
	Some(state)
}

pub async fn run<State: ClientState>(resources: &[&Path]) {
	let Ok(mut client) = stardust_xr_fusion::client::Client::connect().await else {
		return;
	};
	if !resources.is_empty() {
		let _ = client.setup_resources(resources);
	}
	let dbus_connection = connect_client().await.unwrap();

	let Some(mut state): Option<State> = state(&mut client).await else {
		return;
	};

	let mut view = View::new(&state, dbus_connection, client.get_root());

	let _ = client
		.sync_event_loop(|client, _| {
			while let Some(root_event) = client.get_root().recv_root_event() {
				match root_event {
					RootEvent::Frame { info } => {
						state.on_frame(&info);
						view.frame(&info, &mut state);
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
