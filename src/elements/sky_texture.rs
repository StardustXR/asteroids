use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{Error, drawable::SkyGuard, types::Resource};
use tokio::sync::watch;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkyTex {
	pub resource: Resource,
	pub opaque: bool,
}
impl<State: ValidState> CustomElement<State> for SkyTex {
	type Inner = (
		watch::Sender<Option<SkyGuard>>,
		watch::Receiver<Option<SkyGuard>>,
	);
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		_info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (tx, rx) = watch::channel(
			context
				.stardust_client
				.sky_interface()
				.set_sky_tex(self.resource.clone(), self.opaque)
				.await?,
		);
		Ok((tx, rx))
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		if self != old_self {
			let _ = inner.0.send(None);
			let tex = self.clone();
			let watch = inner.0.clone();
			let sky_interface = context.stardust_client.sky_interface().clone();
			tokio::spawn(async move {
				let Ok(sky_guard) = sky_interface.set_sky_tex(tex.resource, tex.opaque).await
				else {
					return;
				};
				_ = watch.send(sky_guard);
			});
		}
	}
}
