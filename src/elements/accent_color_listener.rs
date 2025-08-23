use crate::{
	CreateInnerInfo, ValidState,
	context::Context,
	custom::{CustomElement, FnWrapper},
};
use ashpd::desktop::settings::Settings;
use futures_util::StreamExt;
use stardust_xr_fusion::{
	node::NodeError,
	spatial::SpatialRef,
	values::{Color, color::rgba_linear},
};
use tokio::{sync::watch, task::AbortHandle};

fn accent_color_to_color(accent_color: ashpd::desktop::Color) -> Color {
	rgba_linear!(
		accent_color.red() as f32,
		accent_color.green() as f32,
		accent_color.blue() as f32,
		1.0
	)
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

pub struct AccentColorListenerResource {
	accent_color_loop: AbortHandle,
	accent_color: watch::Receiver<Color>,
}
impl Default for AccentColorListenerResource {
	fn default() -> Self {
		let (accent_color_sender, accent_color) = watch::channel(rgba_linear!(1.0, 1.0, 1.0, 1.0));
		let accent_color_loop =
			tokio::task::spawn(accent_color_loop(accent_color_sender)).abort_handle();
		Self {
			accent_color_loop,
			accent_color,
		}
	}
}
impl Drop for AccentColorListenerResource {
	fn drop(&mut self) {
		self.accent_color_loop.abort();
	}
}

pub struct AccentColorInner {
	spatial: SpatialRef,
	color_rx: watch::Receiver<Color>,
}

#[derive_where::derive_where(Debug, PartialEq)]
#[allow(clippy::type_complexity)]
pub struct AccentColorListener<State: ValidState> {
	pub on_accent_color_changed: FnWrapper<dyn Fn(&mut State, Color) + Send + Sync>,
}

impl<State: ValidState> AccentColorListener<State> {
	pub fn new<F: Fn(&mut State, Color) + Send + Sync + 'static>(
		on_accent_color_changed: F,
	) -> Self {
		AccentColorListener {
			on_accent_color_changed: FnWrapper(Box::new(on_accent_color_changed)),
		}
	}
}
impl<State: ValidState> CustomElement<State> for AccentColorListener<State> {
	type Inner = AccentColorInner;
	type Resource = AccentColorListenerResource;
	type Error = NodeError;

	fn create_inner(
		&self,
		_asteroids_context: &Context,
		info: CreateInnerInfo,
		resource: &mut Self::Resource,
	) -> Result<Self::Inner, Self::Error> {
		Ok(AccentColorInner {
			spatial: info.parent_space.clone(),
			color_rx: resource.accent_color.clone(),
		})
	}

	fn diff(&self, _old_self: &Self, _inner: &mut Self::Inner, _resource: &mut Self::Resource) {}
	fn frame(
		&self,
		_info: &stardust_xr_fusion::root::FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		if inner.color_rx.has_changed().is_ok_and(|t| t) {
			(self.on_accent_color_changed.0)(state, *inner.color_rx.borrow())
		}
	}

	fn spatial_aspect(&self, inner: &Self::Inner) -> SpatialRef {
		inner.spatial.clone()
	}
}
