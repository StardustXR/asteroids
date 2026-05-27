use stardust_xr_fusion::client::{Client, DefaultHandler};
use stardust_xr_molecules::accent_color::AccentColor;
use zbus::Connection;

pub struct Context {
	pub stardust_client: Client<DefaultHandler>,
	pub dbus_connection: Connection,
	pub accent_color: AccentColor,
}
