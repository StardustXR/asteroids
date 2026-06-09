use crate::{Context, CreateInnerInfo, ValidState, custom::CustomElement};
use stardust_xr_fusion::{Error, drawable::SkyGuard, types::Resource};
use tokio::sync::watch;

#[derive(Debug)]
pub struct SkyLight(pub Resource);
impl<State: ValidState> CustomElement<State> for SkyLight {
	type Inner = watch::Receiver<Option<SkyGuard>>;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		_info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let (_, rx) = watch::channel(
			context
				.stardust_client
				.sky_interface()
				.set_sky_light(self.0.clone())
				.await?,
		);
		Ok(rx)
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		if self.0 != old_self.0 {
			_ = context
				.stardust_client
				.sky_interface()
				.set_sky_light(context.stardust_client, Some(&self.0));
		}
	}
}
// pub struct SkyLightInner(SpatialRef);
// fix this later
// impl Drop for SkyLightInner {
// 	fn drop(&mut self) {
// 		_ = set_sky_light(, None);
// 	}
// }
