use crate::{
	Element, Identifiable, ValidState, ElementDiffer,
	inner::ElementInnerMap,
	resource::ResourceRegistry,
	Context,
};
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
use std::marker::PhantomData;

pub struct Mapped<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> {
	wrapped: E,
	mapper: Option<F>,
	phantom: PhantomData<State>,
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Mapped<State, WrappedState, F, E>
{
	pub fn new(wrapped: E, mapper: F) -> Self {
		Self {
			wrapped,
			mapper: Some(mapper),
			phantom: PhantomData,
		}
	}
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> ElementDiffer<State> for Mapped<State, WrappedState, F, E>
{
	fn create_inner_recursive(
		&self,
		inner_key: u64,
		context: &Context,
		parent_space: &SpatialRef,
		element_path: &std::path::Path,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		self.wrapped.create_inner_recursive(inner_key, context, parent_space, element_path, inner_map, resources);
	}
	
	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		if let Some(mapper) = &self.mapper {
			if let Some(mapped_state) = mapper(state) {
				self.wrapped.frame_recursive(context, info, mapped_state, inner_map);
			}
		}
	}
	
    fn diff_same_type(
        &self,
        inner_key: u64,
        old: &Self,
        context: &Context,
        parent_space: &SpatialRef,
        element_path: &std::path::Path,
        inner_map: &mut ElementInnerMap,
        resources: &mut ResourceRegistry,
    ) {
        self.wrapped.diff_same_type(inner_key, &old.wrapped, context, parent_space, element_path, inner_map, resources);
    }
	
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.wrapped.destroy_inner_recursive(inner_map);
	}
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Element<State> for Mapped<State, WrappedState, F, E>
{
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Identifiable for Mapped<State, WrappedState, F, E>
where
	E: Identifiable,
{
	fn identify<H: std::hash::Hash>(self, h: &H) -> Self {
		Mapped {
			wrapped: self.wrapped.identify(h),
			mapper: self.mapper,
			phantom: PhantomData,
		}
	}
}
