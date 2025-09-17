use element::ElementDiffer;
use mapped::Mapped;

pub mod client;
mod context;
mod custom;
mod dynamic_element;
mod element;
pub mod elements;
mod inner;
mod mapped;
mod resource;
mod util;

pub use client::ClientState;
pub use context::*;
pub use custom::*;
pub use dynamic_element::*;
pub use element::{Element, Identifiable, generate_keyed_inner_key, generate_positional_inner_key};
pub use util::*;

pub trait ValidState: Sized + Send + Sync + 'static {}
impl<T: Sized + Send + Sync + 'static> ValidState for T {}

pub trait Reify: ValidState + Sized + Send + Sync + 'static {
	type Output: Element<Self> + 'static;
	fn reify(&self) -> Self::Output;

	fn reify_substate<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut Self> + Send + Sync + 'static,
	>(
		&self,
		mapper: F,
	) -> Mapped<SuperState, Self, F, Self::Output> {
		self.reify().map(mapper)
	}
}

// pub struct Projector<E: Element<State>, State: Reify<E>> {
// 	root: SpatialRef,
// 	trees: Trees<State>,
// 	inner_map: ElementInnerMap,
// 	resources: ResourceRegistry,
// 	phantom: PhantomData<State>,
// }
// impl<E: Element<State>, State: Reify<E>> Projector<E, State> {
// 	pub fn new(
// 		state: &State,
// 		context: &Context,
// 		parent_spatial: SpatialRef,
// 		root_element_path: PathBuf,
// 	) -> Projector<State> {
// 		let blueprint = state.reify();

// 		let mut inner_map = ElementInnerMap::default();
// 		let mut resources = ResourceRegistry::default();
// 		let trees = Trees::new(
// 			blueprint,
// 			context,
// 			&parent_spatial,
// 			&mut inner_map,
// 			&mut resources,
// 			root_element_path,
// 		);
// 		Projector {
// 			root: parent_spatial,
// 			trees,
// 			inner_map,
// 			resources,
// 			phantom: PhantomData,
// 		}
// 	}

// 	#[tracing::instrument(level = "debug", skip_all)]
// 	pub fn update(&mut self, context: &Context, state: &mut State) {
// 		let blueprint = state.reify();
// 		self.trees.diff_and_apply(
// 			blueprint,
// 			context,
// 			&self.root,
// 			&mut self.inner_map,
// 			&mut self.resources,
// 		);
// 	}
// 	pub fn frame(&mut self, context: &Context, info: &FrameInfo, state: &mut State) {
// 		self.trees.frame(context, info, state, &mut self.inner_map);
// 	}
// }
