use crate::{
	Context, Element, Projector, Reify,
	util::{Migrate, RonFile},
};
use serde::{Serialize, de::DeserializeOwned};
use stardust_xr_fusion::{
	Client,
	core::schemas::flex::flexbuffers,
	objects::connect_client,
	root::{FrameInfo, RootAspect, RootEvent},
};
use std::fs::read_to_string;
use tokio::signal::unix::{SignalKind, signal};

/// Represents a client that connects to the stardust server
pub trait ClientState: Reify + Default + Migrate + Serialize + DeserializeOwned {
	/// App ID, inverse domain name e.g. "org.stardustxr.asteroids_test".
	const APP_ID: &'static str;

	/// Update the client state when newly launched (e.g. for program arguments)
	fn initial_state_update(&mut self) {}
	fn on_frame(&mut self, _info: &FrameInfo) {}
	fn reify(&self) -> Element<Self>;
}
impl<T: ClientState> Reify for T {
	fn reify(&self) -> Element<Self> {
		<T as ClientState>::reify(self)
	}
}

fn initial_state<State: ClientState>() -> State {
	// this is a dumb heuristic for determining if it's installed or not, may wanna replace
	#[cfg(debug_assertions)]
	let initial_state_path =
		std::path::PathBuf::from("/tmp/asteroids_config").join(State::APP_ID.to_string() + ".ron");
	#[cfg(not(debug_assertions))]
	let initial_state_path = directories::BaseDirs::new()
		.unwrap()
		.config_dir()
		.join(State::APP_ID)
		.join("initial_state.ron");
	let mut state = match read_to_string(&initial_state_path).ok().map(RonFile) {
		Some(initial_state_string) => State::deserialize_with_migrate(&initial_state_string)
			.unwrap_or_else(|_| State::default()),
		None => State::default(),
	};
	if !initial_state_path.exists() {
		let _ = std::fs::create_dir_all(initial_state_path.parent().unwrap());
		let _ = std::fs::write(&initial_state_path, ron::to_string(&state).unwrap());
	}
	state.initial_state_update();
	state
}

async fn state<State: ClientState>(client: &mut Client) -> Option<State> {
	if let Some(state) = load_dev_state() {
		return Some(state);
	}

	let saved_state = client
		.await_method(client.handle().get_root().get_state())
		.await
		.ok()?
		.ok()?;

	let state = saved_state
		.data
		.and_then(|m| flexbuffers::from_slice(&m).ok())
		.unwrap_or_else(initial_state);
	Some(state)
}

fn load_dev_state<State: ClientState>() -> Option<State> {
	if std::env::var("ASTEROIDS_DEV").is_err() {
		return None;
	}

	let initial_state_path = std::path::PathBuf::from("/tmp/asteroids_config")
		.join(State::APP_ID.to_string() + "_dev.ron");

	let serialized = std::fs::read_to_string(initial_state_path).ok()?;
	ron::from_str(&serialized).ok()
}
fn save_dev_state<State: ClientState>(state: &State) {
	if std::env::var("ASTEROIDS_DEV").is_err() {
		return;
	}

	let initial_state_path = std::path::PathBuf::from("/tmp/asteroids_config")
		.join(State::APP_ID.to_string() + "_dev.ron");

	let _ = std::fs::create_dir_all(initial_state_path.parent().unwrap());
	let _ = std::fs::write(&initial_state_path, ron::to_string(&state).unwrap());
}

pub async fn run<State: ClientState>(resources: &[&std::path::Path]) {
	let Ok(mut client) = stardust_xr_fusion::client::Client::connect().await else {
		return;
	};
	if !resources.is_empty() {
		let _ = client.setup_resources(resources);
	}
	let context = Context {
		dbus_connection: connect_client().await.unwrap(),
	};

	let Some(mut state): Option<State> = state(&mut client).await else {
		return;
	};

	dioxus_devtools::connect_subsecond();

	let mut view = Projector::new(&state, &context, client.get_root());

	let event_loop_future = client.sync_event_loop(|client, _| {
		while let Some(root_event) = client.get_root().recv_root_event() {
			match root_event {
				RootEvent::Ping { response: pong } => pong.send(Ok(())),
				RootEvent::Frame { info } => {
					state.on_frame(&info);
					#[cfg(feature = "tracy")]
					{
						use tracing::info;
						info!("frame info {info:#?}");
						tracy_client::frame_mark();
					}
					view.frame(&info, &mut state);
					view.update(&context, &mut state);
				}
				RootEvent::SaveState { response } => response.wrap(|| {
					stardust_xr_fusion::root::ClientState::from_data_root(
						Some(flexbuffers::to_vec(&state)?),
						client.get_root(),
					)
				}),
			}
		}
	});
	let mut sigterm = signal(SignalKind::terminate()).unwrap();
	// make sure we call Drop impls
	tokio::select! {
		_ = event_loop_future => {}
		_ = tokio::signal::ctrl_c() => {}
		_ = sigterm.recv() => {}
	}
	save_dev_state(&state);
	drop(view);
	_ = client.try_flush().await;
}
