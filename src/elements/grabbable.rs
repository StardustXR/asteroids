use crate::{
	custom::{ElementTrait, FnWrapper, Transformable},
	ValidState,
};
use derive_setters::Setters;
use derive_where::derive_where;
use mint::Vector2;
use rustc_hash::{FxHashMap, FxHashSet};
use stardust_xr_fusion::{
	core::values::{Color, ResourceID},
	drawable::{
		Line, LinesAspect, MaterialParameter, ModelPartAspect, TextAspect, TextBounds, TextStyle,
		XAlign, YAlign,
	},
	fields::{Field, FieldAspect, Shape},
	items::panel::{PanelItem, PanelItemAspect, SurfaceId},
	node::{NodeError, NodeResult},
	spatial::{SpatialAspect, SpatialRef, Transform},
	values::color::rgba_linear,
};
use stardust_xr_molecules::{
	button::ButtonVisualSettings,
	keyboard::{KeyboardHandler, KeypressInfo},
	DebugSettings, UIElement, VisualDebug,
};
use std::{fmt::Debug, hash::Hash};
use zbus::Connection;

#[derive_where(Debug, Clone, PartialEq, Setters)]
#[setters(into, strip_option)]
pub struct Grabbable {
	transform: Transform,
}
impl<State: ValidState> ElementTrait<State> for Grabbable {
	type Inner = stardust_xr_molecules::Grabbable;
	type Error = NodeError;

	fn create_inner(&self, parent_space: &SpatialRef) -> Result<Self::Inner, Self::Error> {
		stardust_xr_molecules::Grabbable::create(
			parent_space,
			self.transform.clone(),
			field,
			GrabbableSettings::default(),
		)
	}

	fn update(&self, old_decl: &Self, inner: &mut Self::Inner) {
		self.apply_transform(old_decl, inner.content_parent())
	}

	fn spatial_aspect<'a>(&self, inner: &Self::Inner) -> SpatialRef {
		Some(inner.content_parent())
	}
}
impl Transformable for Grabbable {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}
