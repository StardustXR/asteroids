// Reexport types used in declarative element struct fields
pub use stardust_xr_fusion::{
	drawable::{MaterialParameter, TextBounds, XAlign, YAlign},
	spatial::BoundingBox,
	types::*,
};
pub use stardust_xr_molecules::{
	DebugSettings, button::ButtonVisualSettings, keyboard_handler::protocol::KeyEvent,
	mouse_handler::ScrollSource,
};

#[macro_export]
macro_rules! mod_expose {
	($mod_name:ident) => {
		pub mod $mod_name;
		pub use $mod_name::*;
	};
}

mod_expose!(axes);
mod_expose!(button);
mod_expose!(derezzable);
mod_expose!(dial);
// mod_expose!(field_viz);
mod_expose!(file_watcher);
mod_expose!(grab_ring);
// mod_expose!(grabbable);
// mod_expose!(handle);
mod_expose!(keyboard_handler);
mod_expose!(lines);
mod_expose!(model);
mod_expose!(mouse_handler);
// mod_expose!(playspace);
mod_expose!(reparentable);
// mod_expose!(pen);
// mod_expose!(size_constrainer);
// mod_expose!(sky_light);
// mod_expose!(sky_texture);
mod_expose!(spatial);
// mod_expose!(spline_rail);
mod_expose!(text);
mod_expose!(turntable);
