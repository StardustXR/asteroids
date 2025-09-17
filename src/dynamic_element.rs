use crate::{
	Context, ValidState, element::ElementDiffer, inner::ElementInnerMap, resource::ResourceRegistry,
};
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::path::Path;

/// Trait for elements that support dynamic type swapping (rare cases like KDL environments)
pub(crate) trait DynamicDiffer<State: ValidState>: Send + Sync + std::any::Any {
	/// Create the inner imperative struct and all children
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);

	/// Every frame on the server
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	);

	/// Dynamic path: handles type checking and bridges to fast path
	#[allow(clippy::too_many_arguments)]
	fn diff_dynamic(
		&self,
		inner_key: u64,
		old: &dyn DynamicDiffer<State>,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	);

	/// Clean up this element and all children
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap);
}

// Blanket implementation for any ElementDiffer + Any
impl<T, State: ValidState> DynamicDiffer<State> for T
where
	T: ElementDiffer<State> + std::any::Any,
{
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		ElementDiffer::create_inner_recursive(
			self,
			inner_key,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		)
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		ElementDiffer::frame_recursive(self, context, info, state, inner_map)
	}

	fn diff_dynamic(
		&self,
		inner_key: u64,
		old: &dyn DynamicDiffer<State>,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Try to downcast to same type for fast path
		use std::any::Any;
		if let Some(old_same) = (old as &dyn Any).downcast_ref::<Self>() {
			// Same type - jump to zero-cost fast path!
			self.diff_same_type(
				inner_key,
				old_same,
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
			);
		} else {
			// Different types - destroy old and create new
			old.destroy_inner_recursive(inner_map);
			self.create_inner_recursive(
				inner_key,
				context,
				parent_space,
				element_path,
				inner_map,
				resources,
			);
		}
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		ElementDiffer::destroy_inner_recursive(self, inner_map)
	}
}

pub struct DynamicElement<State: ValidState>(Box<dyn DynamicDiffer<State> + Send + Sync>);
impl<State: ValidState> DynamicElement<State> {
	pub(crate) fn new<D: DynamicDiffer<State>>(element: D) -> Self {
		Self(Box::new(element))
	}
}

impl<State: ValidState> ElementDiffer<State> for DynamicElement<State> {
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		self.0.create_inner_recursive(
			inner_key,
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		)
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		self.0.frame_recursive(context, info, state, inner_map)
	}

	fn diff_same_type(
		&self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		// Use dynamic diffing since we don't know the concrete types
		self.0.diff_dynamic(
			inner_key,
			old.0.as_ref(),
			context,
			parent_space,
			element_path,
			inner_map,
			resources,
		)
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.0.destroy_inner_recursive(inner_map)
	}
}
