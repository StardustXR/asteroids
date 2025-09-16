use stardust_xr_asteroids::{ClientState, CustomElement, Element, Migrate, Reify, client, elements::Spatial};
use serde::{Deserialize, Serialize};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	client::run::<Test>(&[]).await
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Test {
	next: Option<Box<Self>>,
}
impl Migrate for Test {
	type Old = Self;
}
impl ClientState for Test {
	const APP_ID: &'static str = "org.test";
}
impl Reify for Test {
	fn reify(&self) -> impl Element<Self> {
		Spatial::default()
			.build()
			.maybe_child(self.next.as_ref().map(|n| n.reify().heap()))
	}
}
