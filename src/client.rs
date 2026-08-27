use crate::{
	Context, Projector, Reify, Tasker,
	task::RootTasker,
	util::{Migrate, RonFile},
};
use gluon::Liveness;
use serde::{Serialize, de::DeserializeOwned};
use stardust_xr_fusion::{Result, client::FrameInfo};
use stardust_xr_molecules::accent_color::AccentColor;
use std::{
	fs::read_to_string,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering},
		mpsc,
	},
};
use tokio::signal::unix::{SignalKind, signal};
use zbus::Connection;

#[macro_export]
macro_rules! project_local_resources {
	($relative_path:expr) => {
		std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join($relative_path)
	};
}

/// Represents a client that connects to the stardust server
pub trait ClientState: Reify + Default + Migrate + Serialize + DeserializeOwned {
	/// App ID, inverse domain name e.g. "org.stardustxr.asteroids_test".
	const APP_ID: &'static str;

	/// Update the client state when newly launched (e.g. for program arguments)
	fn initial_state_update(&mut self) {}
	/// Run this first thing any time! for tasks
	fn on_start(&mut self, _context: &Context, _tasks: impl Tasker<Self>) {}
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

// Bring this back when we have session service
// async fn state<State: ClientState>(client: &mut Client) -> Option<State> {
// 	if let Some(state) = load_dev_state() {
// 		return Some(state);
// 	}

// 	let saved_state = client
// 		.await_method(client.handle().get_root().get_state())
// 		.await
// 		.ok()?
// 		.ok()?;

// 	let state = saved_state
// 		.data
// 		.and_then(|m| ron::from_str(&String::from_utf8(m).ok()?).ok())
// 		.unwrap_or_else(initial_state);

// 	Some(state)
// }

// fn load_dev_state<State: ClientState>() -> Option<State> {
// 	if std::env::var("ASTEROIDS_DEV").is_err() {
// 		return None;
// 	}

// 	let initial_state_path = std::path::PathBuf::from("/tmp/asteroids_config")
// 		.join(State::APP_ID.to_string() + "_dev.ron");

// 	let serialized = std::fs::read_to_string(initial_state_path).ok()?;
// 	ron::from_str(&serialized).ok()
// }
fn save_dev_state<State: ClientState>(state: &State) {
	if std::env::var("ASTEROIDS_DEV").is_err() {
		return;
	}

	let initial_state_path = std::path::PathBuf::from("/tmp/asteroids_config")
		.join(State::APP_ID.to_string() + "_dev.ron");

	let _ = std::fs::create_dir_all(initial_state_path.parent().unwrap());
	let _ = std::fs::write(&initial_state_path, ron::to_string(&state).unwrap());
}

pub async fn run<State: ClientState>(resources: &[&std::path::Path]) -> Result<()> {
	let (stardust_client, root) = stardust_xr_fusion::client::Client::connect(resources).await?;
	tracing::debug!("connected to stardust server");

	let dbus_connection = Connection::session().await.unwrap();
	tracing::debug!("connected to dbus");
	let accent_color = AccentColor::new(dbus_connection.clone());
	let context = Context {
		stardust_client: Arc::new(stardust_client),
		dbus_connection,
		accent_color: Arc::new(accent_color),
		stop: Arc::new(AtomicBool::new(false)),
	};

	let mut state: State = initial_state();
	// let Some(mut state): Option<State> = state(&mut context.stardust_client).await else {
	// return;
	// };

    #[cfg(feature = "subsecond")]
	dioxus_devtools::connect_subsecond();

	let (tx, rx) = mpsc::channel();
	let root_tasker = RootTasker(tx);

	state.on_start(&context, root_tasker.clone());

	let mut projector = Projector::create(&state, &context, root_tasker, rx, root, "/".into());
	let mut frame_awaiter = context.stardust_client.frame_receiver();
	let mut sigterm = signal(SignalKind::terminate()).unwrap();

	let server = context.stardust_client.server();
	tracing::debug!("entering frame select loop");
	loop {
		let first_frame = tokio::select! {
			result = frame_awaiter.recv() => match result {
				Ok(info) => {
					tracing::debug!("select: got frame");
					info
				}
				Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
					tracing::warn!("Skipped {n} frames waiting for receiver");
					continue;
				}
				Err(_) => {
					tracing::debug!("select: frame channel closed");
					break;
				}
			},
			_ = server.death_notification() => {
				tracing::debug!("select: server death notification");
				break;
			}
			_ = tokio::signal::ctrl_c() => {
				tracing::debug!("select: ctrl_c");
				break;
			}
			_ = sigterm.recv() => {
				tracing::debug!("select: sigterm");
				break;
			}
		};

		let mut frames = vec![first_frame];
		loop {
			match frame_awaiter.try_recv() {
				Ok(info) => frames.push(info),
				Err(tokio::sync::broadcast::error::TryRecvError::Lagged(n)) => {
					tracing::warn!("Dropped {n} frames!!");
					break;
				}
				_ => break,
			}
		}

		if frames.len() > 1 {
			tracing::warn!("Dropped {} frames!!", frames.len() - 1);
		}

		for frame in &frames {
			tracing::debug!("run frame events");
			#[cfg(feature = "tracy")]
			{
				use tracing::info;
				info!("frame info {frame:#?}");
				tracy_client::frame_mark();
			}
			state.on_frame(frame);
			projector.frame(&context, frame, &mut state);
		}
		tracing::debug!("diff the tree");
		projector.update(&context, &mut state);
		if context.stop.load(Ordering::Acquire) {
			break;
		}
	}

	save_dev_state(&state);
	drop(projector);
	Ok(())
}

// pub struct Asteroids<State: ClientState> {
// 	context: OnceLock<Context>,
// 	projector: Mutex<Projector<State>>,
// }
// impl<State: ClientState> ClientHandler for Asteroids<State> {
// 	fn ping(&self, _ctx: gluon::Context) -> impl Future<Output = ()> + Send + Sync {
// 		todo!()
// 	}

// 	fn frame(
// 		&self,
// 		_ctx: gluon::Context,
// 		info: FrameInfo,
// 	) -> impl Future<Output = ()> + Send + Sync {
// 		todo!()
// 	}
// }
