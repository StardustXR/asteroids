use std::sync::Arc;

use stardust_xr_fusion::client::{Client, DefaultHandler};
use stardust_xr_molecules::accent_color::AccentColor;
use zbus::Connection;

#[derive(Clone)]
pub struct Context {
	pub stardust_client: Arc<Client<DefaultHandler>>,
	pub dbus_connection: Connection,
	pub accent_color: Arc<AccentColor>,
}
