use std::sync::Arc;

use stardust_xr_fusion::object_registry::ObjectRegistry;
use stardust_xr_molecules::accent_color::AccentColor;
use zbus::Connection;

pub struct Context {
	pub dbus_connection: Connection,
	pub object_registry: Arc<ObjectRegistry>,
	pub accent_color: AccentColor,
}
