use stardust_xr_molecules::accent_color::AccentColor;
use zbus::Connection;

pub struct Context {
	pub dbus_connection: Connection,
	pub accent_color: AccentColor,
}
