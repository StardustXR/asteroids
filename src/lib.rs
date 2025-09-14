use std::marker::PhantomData;

use element::ElementFlattener;
use inner::ElementInnerMap;
use mapped::Mapped;
use resource::ResourceRegistry;
use stardust_xr_fusion::root::FrameInfo;
use stardust_xr_fusion::spatial::SpatialRef;

pub mod client;
mod context;
mod custom;
mod element;
pub mod elements;
mod inner;
mod mapped;
mod resource;
mod tree;
mod util;

pub use client::ClientState;
pub use context::*;
pub use custom::*;
pub use element::{Element, Identifiable};
use tree::Trees;
pub use util::*;

pub trait ValidState: Sized + Send + Sync + 'static {}
impl<T: Sized + Send + Sync + 'static> ValidState for T {}

pub trait Reify: ValidState + Sized + Send + Sync + 'static {
	fn reify(&self) -> impl Element<Self>;

	fn reify_substate<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut Self> + Send + Sync + 'static,
	>(
		&self,
		mapper: F,
	) -> Mapped<SuperState, Self, F, impl Element<Self>> {
		self.reify().map(mapper)
	}
}

pub struct Projector<State: Reify> {
	root: SpatialRef,
	trees: Trees<State>,
	inner_map: ElementInnerMap,
	resources: ResourceRegistry,
	phantom: PhantomData<State>,
}
impl<State: Reify> Projector<State> {
	pub fn new(state: &State, context: &Context, parent_spatial: SpatialRef) -> Projector<State> {
		let blueprint = state.reify();

		let mut inner_map = ElementInnerMap::default();
		let mut resources = ResourceRegistry::default();
		let trees = Trees::new(
			blueprint,
			context,
			&parent_spatial,
			&mut inner_map,
			&mut resources,
		);
		Projector {
			root: parent_spatial,
			trees,
			inner_map,
			resources,
			phantom: PhantomData,
		}
	}

	#[tracing::instrument(level = "debug", skip_all)]
	pub fn update(&mut self, context: &Context, state: &mut State) {
		let blueprint = state.reify();
		self.trees.diff_and_apply(
			blueprint,
			context,
			&self.root,
			&mut self.inner_map,
			&mut self.resources,
		);
	}
	pub fn frame(&mut self, info: &FrameInfo, state: &mut State) {
		self.trees.frame(info, state, &mut self.inner_map);
	}
}
