use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

use stardust_xr_fusion::{
	client::{Client, DefaultHandler},
	types::Timestamp,
};
use stardust_xr_molecules::accent_color::AccentColor;
use tokio::sync::watch;
use zbus::Connection;

#[derive(Clone)]
pub struct Context {
	pub stardust_client: Arc<Client<DefaultHandler>>,
	pub dbus_connection: Connection,
	pub accent_color: Arc<AccentColor>,
	pub(crate) stop: Arc<AtomicBool>,
	pub(crate) predicted_display_time: watch::Receiver<Option<Timestamp>>,
}
impl Context {
	pub fn stop(&self) {
		self.stop.store(true, Ordering::Release);
	}
	/// of the latest frame, `None` before the first one
	pub fn predicted_display_time(&self) -> Option<Timestamp> {
		*self.predicted_display_time.borrow()
	}
}
