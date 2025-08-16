use std::{
	any::{Any, TypeId},
	marker::PhantomData,
};

use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};

use crate::{
	Context, Element, ElementInnerMap, ResourceRegistry, ValidState, inner::ElementInnerKey,
	scenegraph::GenericElement,
};
pub(crate) struct MappedElement<
	State: ValidState,
	SubState: ValidState,
	F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
> {
	element: Element<SubState>,
	mapper: F,
	data: PhantomData<State>,
}

impl<
	State: ValidState,
	SubState: ValidState,
	F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
> MappedElement<State, SubState, F>
{
	pub fn new(element: Element<SubState>, mapper: F) -> Self {
		Self {
			element,
			mapper,
			data: PhantomData,
		}
	}
}
impl<
	State: ValidState,
	SubState: ValidState,
	F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
> GenericElement<State> for MappedElement<State, SubState, F>
{
	fn type_id(&self) -> TypeId {
		GenericElement::type_id(&*self.element.0)
	}
	fn type_name(&self) -> &'static str {
		GenericElement::type_name(&*self.element.0)
	}

	fn add_child(&mut self, _child: Element<State>) {
		panic!("can't add a child to a mapped element yet... sorry! try adding it before mapping")
	}

	fn create_inner_recursive(
		&self,
		parent: &SpatialRef,
		inner_map: &mut ElementInnerMap,
		context: &Context,
		resources: &mut ResourceRegistry,
	) -> Result<(), String> {
		self.element
			.0
			.create_inner_recursive(parent, inner_map, context, resources)
	}
	fn frame_recursive(
		&self,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		let Some(mapped) = (self.mapper)(state) else {
			return;
		};
		self.element.0.frame_recursive(info, mapped, inner_map);
	}
	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.element.0.destroy_inner_recursive(inner_map)
	}

	fn spatial_aspect(&self, inner_map: &ElementInnerMap) -> SpatialRef {
		self.element.0.spatial_aspect(inner_map)
	}
	fn inner_key(&self) -> Option<ElementInnerKey> {
		self.element.0.inner_key()
	}
	fn apply_element_keys(&self, parent_path: &[(usize, TypeId, &'static str)], order: usize) {
		self.element.0.apply_element_keys(parent_path, order)
	}
	fn identify(&mut self, key: ElementInnerKey) {
		self.element.0.identify(key);
	}
	fn diff_and_apply(
		&self,
		parent_spatial: SpatialRef,
		old: &Element<State>,
		context: &Context,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
		resources: &mut ResourceRegistry,
	) {
		let old_upcast: &(dyn Any + 'static) = &*old.0;
		let old_mapper: &Self = old_upcast.downcast_ref().unwrap();
		let Some(mapped) = (self.mapper)(state) else {
			return;
		};
		self.element.0.diff_and_apply(
			parent_spatial,
			&old_mapper.element,
			context,
			mapped,
			inner_map,
			resources,
		)
	}
}

impl<
	State: ValidState,
	SubState: ValidState,
	F: Fn(&mut State) -> Option<&mut SubState> + Send + Sync + 'static,
> std::fmt::Debug for MappedElement<State, SubState, F>
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("MappedElement").field(&self.element).finish()
	}
}
