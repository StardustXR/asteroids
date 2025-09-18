use std::path::PathBuf;

use bumpalo::{Bump, boxed::Box};
use element::ElementDiffer;
use inner::ElementInnerMap;
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
pub use element::{Element, gen_inner_key};

use resource::ResourceRegistry;
use stardust_xr_fusion::{root::FrameInfo, spatial::SpatialRef};
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

pub struct Projector<State: Reify>(Option<ProjectorInner<State>>);
impl<State: Reify> Projector<State> {
	pub fn create(
		state: &State,
		context: &Context,
		parent_spatial: SpatialRef,
		root_element_path: PathBuf,
	) -> Projector<State> {
		let mut inner_map = ElementInnerMap::default();
		let mut resource_registry = ResourceRegistry::default();

		let blueprint = state.reify();
		blueprint.create_inner_recursive(
			0,
			context,
			&parent_spatial,
			&root_element_path,
			&mut inner_map,
			&mut resource_registry,
		);
		let bump = Bump::new();

		Self(Some(ProjectorInner::new(
			parent_spatial,
			inner_map,
			resource_registry,
			root_element_path,
			bump,
			move |bump| unsafe {
				let concrete = Box::new_in(blueprint, bump);
				let raw = Box::into_raw(concrete);
				Box::from_raw(raw) // coerce the type manually since bumpalo can't implement `CoerceUnsized`
			},
		)))
	}

	#[tracing::instrument(level = "debug", skip_all)]
	pub fn update(&mut self, context: &Context, state: &mut State) {
		let Some(mut projector) = self.0.take() else {
			tracing::warn!("Projector not found on update... how??");
			return;
		};
		let blueprint = state.reify();
		projector.with_mut(|fields| {
			blueprint.dynamic_diff(
				0,
				fields.old.as_ref(),
				context,
				fields.root,
				fields.root_element_path,
				fields.inner_map,
				&mut *fields.resource_registry,
			);
		});

		// Move out fields by destructuring
		let ouroboros_impl_projector_inner::Heads {
			mut bump,
			root_element_path,
			resource_registry,
			inner_map,
			root,
			..
		} = projector.into_heads();
		bump.reset();
		self.0.replace(ProjectorInner::new(
			root,
			inner_map,
			resource_registry,
			root_element_path,
			bump,
			move |bump| unsafe {
				let old_concrete = Box::new_in(blueprint, bump);
				let old_raw = Box::into_raw(old_concrete);
				// coerce the type manually since bumpalo can't implement `CoerceUnsized`
				Box::from_raw(old_raw)
			},
		));
	}
	pub fn frame(&mut self, context: &Context, info: &FrameInfo, state: &mut State) {
		let Some(projector) = self.0.as_mut() else {
			tracing::warn!("Projector not found on frame... how??");
			return;
		};
		projector.with_mut(|fields| {
			fields
				.old
				.dynamic_frame_recursive(context, info, state, fields.inner_map);
		});
	}
}

#[ouroboros::self_referencing]
pub struct ProjectorInner<State: Reify> {
	root: SpatialRef,
	inner_map: ElementInnerMap,
	resource_registry: ResourceRegistry,
	root_element_path: PathBuf,
	bump: Bump,
	#[borrows(bump)]
	#[covariant]
	old: Box<'this, dyn DynamicDiffer<State>>,
}
