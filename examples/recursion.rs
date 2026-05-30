use serde::{Deserialize, Serialize};
use stardust_xr_asteroids::{
	ClientState, Context, CustomElement, Element, Migrate, Reify, Tasker, client, elements::Spatial,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
	client::run::<Test>(&[]).await.unwrap()
}

#[derive(Default, Debug, Serialize, Deserialize)]
pub struct Test {
	next: Option<Box<Self>>,
}
impl Migrate for Test {
	type Old = Self;
}
impl ClientState for Test {
	const APP_ID: &'static str = "org.stardustxr.asteroids.Recursion";
}
impl Reify for Test {
	fn reify(&self, context: &Context, tasks: impl Tasker<Self>) -> impl Element<Self> {
		Spatial::default().build().maybe_child(
			self.next
				.as_ref()
				.map(|n| n.reify(context, tasks).dynamic()),
		)
	}
}
