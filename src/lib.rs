use scenegraph::{ElementInnerMap, GenericElement, ResourceRegistry};
use stardust_xr_fusion::root::FrameInfo;
use stardust_xr_fusion::spatial::{Spatial, SpatialRefAspect, Transform};

pub mod client;
mod context;
mod custom;
pub mod elements;
mod scenegraph;
mod util;

pub use client::ClientState;
pub use context::*;
pub use custom::*;
pub use scenegraph::Element;
pub use util::*;

pub trait ValidState: Sized + Send + Sync + 'static {}
impl<T: Sized + Send + Sync + 'static> ValidState for T {}

pub trait Reify: ValidState + Sized + Send + Sync + 'static {
	fn reify(&self) -> Element<Self>;

	fn reify_substate<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut Self> + Send + Sync + 'static,
	>(
		&self,
		mapper: F,
	) -> Element<SuperState> {
		self.reify().map(mapper)
	}
}

pub struct View<State: Reify> {
	_root: Spatial,
	vdom_root: Element<State>,
	inner_map: ElementInnerMap,
	resources: ResourceRegistry,
}
impl<State: Reify> View<State> {
	pub fn new(
		state: &State,
		context: &Context,
		parent_spatial: &impl SpatialRefAspect,
	) -> View<State> {
		let _root = Spatial::create(parent_spatial, Transform::identity(), false).unwrap();
		let mut inner_map = ElementInnerMap::default();
		let vdom_root = elements::Spatial::default().with_children([state.reify()]);
		vdom_root.0.apply_element_keys(vec![(
			0,
			GenericElement::type_id(vdom_root.0.as_ref()),
			String::new(),
		)]);
		let mut resources = ResourceRegistry::default();
		vdom_root
			.0
			.create_inner_recursive(
				&_root.clone().as_spatial_ref(),
				&mut inner_map,
				context,
				&mut resources,
			)
			.unwrap();
		View {
			_root,
			vdom_root,
			inner_map,
			resources,
		}
	}

	pub fn update(&mut self, context: &Context, state: &mut State) {
		let new_vdom = elements::Spatial::default().with_children([state.reify()]);
		new_vdom.0.apply_element_keys(vec![(
			0,
			GenericElement::type_id(new_vdom.0.as_ref()),
			String::new(),
		)]);
		new_vdom.0.diff_and_apply(
			self.vdom_root.0.spatial_aspect(&self.inner_map),
			&self.vdom_root,
			context,
			state,
			&mut self.inner_map,
			&mut self.resources,
		);
		self.vdom_root = new_vdom;
	}
	pub fn frame(&mut self, info: &FrameInfo, state: &mut State) {
		self.vdom_root
			.0
			.frame_recursive(info, state, &mut self.inner_map);
	}
}
