use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{Error, drawable::SkyGuard, types::Resource};
use tokio::sync::watch;

#[derive(Debug)]
pub struct SkyLight(pub Resource);
impl<State: ValidState> CustomElement<State> for SkyLight {
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
				.set_sky_light(self.0.clone())
				.await?,
		);
		Ok((tx, rx))
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		if self.0 != old_self.0 {
			let _ = inner.0.send(None);
			let resource = self.0.clone();
			let watch = inner.0.clone();
			let sky_interface = context.stardust_client.sky_interface().clone();
			tokio::spawn(async move {
				let Ok(sky_guard) = sky_interface.set_sky_light(resource).await else {
					return;
				};
				_ = watch.send(sky_guard);
			});
		}
	}
}
