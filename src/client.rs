use crate::{
	Context, Projector, Reify,
	util::{Migrate, RonFile},
};
use ashpd::desktop::settings::Settings;
use color::{ToRgba, rgba};
use futures_util::StreamExt;
use serde::{Serialize, de::DeserializeOwned};
use stardust_xr_fusion::{
	Client,
	node::NodeType,
	objects::connect_client,
	root::{FrameInfo, RootAspect, RootEvent},
	values::{Color, color::rgba_linear},
};
use std::fs::read_to_string;
use tokio::{
	signal::unix::{SignalKind, signal},
	sync::watch,
};

fn accent_color_to_color(accent_color: ashpd::desktop::Color) -> Color {
	rgba!(
		accent_color.red() as f32,
		accent_color.green() as f32,
		accent_color.blue() as f32,
		1.0
	)
	.to_linear()
}

async fn accent_color_loop(accent_color_sender: watch::Sender<Color>) -> Result<(), ashpd::Error> {
	let settings = Settings::new().await?;
	let initial_color = accent_color_to_color(settings.accent_color().await?);
	let _ = accent_color_sender.send(initial_color);
	tracing::info!("Accent color initialized to {:?}", initial_color);
	let mut accent_color_stream = settings.receive_accent_color_changed().await?;
	tracing::info!("Got accent color stream");

	while let Some(accent_color) = accent_color_stream.next().await {
		let accent_color = accent_color_to_color(accent_color);
		tracing::info!("Accent color changed to {:?}", accent_color);
		let _ = accent_color_sender.send(accent_color);
	}

	tracing::error!("why the sigma is this activating");
	Ok(())
}

/// Represents a client that connects to the stardust server
pub trait ClientState: Reify + Default + Migrate + Serialize + DeserializeOwned {
	/// App ID, inverse domain name e.g. "org.stardustxr.asteroids_test".
	const APP_ID: &'static str;

	/// Update the client state when newly launched (e.g. for program arguments)
	fn initial_state_update(&mut self) {}
	fn on_frame(&mut self, _info: &FrameInfo) {}
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
		.and_then(|m| ron::from_str(&String::from_utf8(m).ok()?).ok())
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

	let dbus_connection = connect_client().await.unwrap();
	let _ = ashpd::set_session_connection(dbus_connection.clone());

	let (accent_color_sender, accent_color) = watch::channel(rgba_linear!(1.0, 1.0, 1.0, 1.0));
	let accent_color_loop =
		tokio::task::spawn(accent_color_loop(accent_color_sender)).abort_handle();
	let mut context = Context {
		dbus_connection,
		accent_color: *accent_color.borrow(),
	};

	let Some(mut state): Option<State> = state(&mut client).await else {
		return;
	};

	dioxus_devtools::connect_subsecond();

	let mut projector = Projector::create(
		&state,
		&context,
		client.get_root().clone().as_spatial_ref(),
		"/".into(),
	);
	let event_loop_future = client.sync_event_loop(|client, _| {
		let mut frames = vec![];
		while let Some(root_event) = client.get_root().recv_root_event() {
			match root_event {
				RootEvent::Ping { response: pong } => pong.send_ok(()),
				RootEvent::Frame { info } => {
					#[cfg(feature = "tracy")]
					{
						use tracing::info;
						info!("frame info {info:#?}");
						tracy_client::frame_mark();
					}
					frames.push(info);
				}
				RootEvent::SaveState { response } => {
					response.send_ok(stardust_xr_fusion::root::ClientState {
						data: ron::to_string(&state).ok().map(|s| s.into_bytes()),
						root: client.get_root().id(),
						spatial_anchors: Default::default(),
					})
				}
			}
		}
		if frames.is_empty() {
			return;
		}
		context.accent_color = *accent_color.borrow();
		if frames.len() > 1 {
			tracing::warn!("Dropped {} frames!!", frames.len() - 1);
		}

		for frame in frames {
			state.on_frame(&frame);
			projector.frame(&context, &frame, &mut state);
		}
		projector.update(&context, &mut state);
	});
	let mut sigterm = signal(SignalKind::terminate()).unwrap();
	// make sure we call Drop impls
	tokio::select! {
		_ = event_loop_future => {}
		_ = tokio::signal::ctrl_c() => {}
		_ = sigterm.recv() => {}
	}
	accent_color_loop.abort();
	save_dev_state(&state);
	_ = client.try_flush().await;
}
