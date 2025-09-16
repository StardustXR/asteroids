use stardust_xr_fusion::values::Color;
use zbus::Connection;

pub struct Context {
	pub dbus_connection: Connection,
	pub accent_color: Color,
}
