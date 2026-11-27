//! an entity is one spatial, one field and one queryable, shared by a list of components.
//!
//! the problem this solves: a panel that's grabbable, derezzable and reparentable is one object,
//! but building it from nested elements gives it three spatials and a lot of duplicated code. an
//! [`Entity`] creates the spatial/field/queryable once, and every [`Component`] (grabbable,
//! derezzable, ...) uses those same ones.
//!
//! - add components with [`Entity::component`]. they're stored as nested tuples: `(((), A), B)`
//! - wrap one in `Option` to turn it on and off
//! - a component can read other components' inners through [`Inners`]
//!
//! # 1. config vs live state
//!
//! you write an `Entity` (the config). the first time it's seen, `create_inner` builds an
//! `EntityInner` on the server. after that, every change runs `diff` instead of rebuilding.
//! components never own the server objects, they borrow them through `ComponentCreateInfo`.
//!
//! ```text
//!  +-------------------------+                         +--------------------------------+
//!  | Entity         (config) |  create_inner: builds   | EntityInner             (live) |
//!  |                         |------------------------>|                                |
//!  |  transform              |  diff(old, new)         |  parent_space    anchor        |
//!  |  field_shape            |------------------------>|  anchor_space    spatial       |
//!  |  components:            |  frame: every frame     |  field           queryable     |
//!  |    (((), Grabbable),    |------------------------>|                                |
//!  |     Option<Derezzable>) |                         |  ComponentInners:              |
//!  +-------------------------+                         |    (((), Grabbable::Inner),    |
//!                                                      |     OptionComponentInner)      |
//!                                                      +--------------------------------+
//!                                                            |
//!                                                            | lent out as
//!                                                            v
//!                                      +--------------------------------------------+
//!                                      | ComponentCreateInfo             (borrowed) |
//!                                      |  the server objects above + the transform, |
//!                                      |  given to every component's create_inner   |
//!                                      |  and diff                                  |
//!                                      +--------------------------------------------+
//! ```
//!
//! # 2. spatial tree inside one entity
//!
//! the anchor is an extra spatial whose only job is to be moved. `Containable` reparents the
//! anchor, so the entity's own transform stays relative to the same space.
//!
//! ```text
//!                                                                        +-----------+
//!                                                                   +--->| field     |
//!  +--------------+  parent of  +--------+  parent of  +---------+  |    +-----------+
//!  | parent_space |------------>| anchor |------------>| spatial |--+
//!  +--------------+             +--------+             +---------+  |    +-----------+
//!                                                                   +--->| queryable |
//!                                                                        +-----------+
//!
//!  parent_space: where the element sits
//!  anchor:       Containable reparents this
//!  spatial:      the entity's transform lives here
//!  queryable:    components add their interfaces here
//! ```
//!
//! # 3. how a component finds another component
//!
//! the tuple's real type is `(((), A), B)`, which a component can't name. `InnerLookup` hides
//! that type, so `Inners` works no matter what else is on the entity. lookups go by component
//! type, not inner type, so two components with the same inner type never get mixed up.
//!
//! ```text
//!  Grabbable::frame calls inners.get::<Containable>()
//!     |
//!     v
//!  +-------------------+              +-----------------+       +-----------------+
//!  | Inners<Grabbable> | find(TypeId) | dyn InnerLookup |       | ComponentInners |
//!  | one component's   |------------->| hides the tuple |------>| the whole tuple |
//!  | view              |              | type            |       | of inners       |
//!  +-------------------+              +-----------------+       +-----------------+
//!                                                                        |
//!                                                     find_inner, walks  |
//!                                                     the tuple          |
//!         +--------------------------------------------------------------+
//!         v
//!  (A, B)          ask B first, then A, so the newest component wins
//!     |
//!     v
//!  Option<C>       asking for Option<C> gives the enum,
//!     |            asking for C gives C's inner once it's built
//!     v
//!  any component   is it my TypeId? then return &dyn Any,
//!                  which Inners downcasts to &C::Inner
//!
//!  sealed::Token: a type only this crate can name, so only these impls can
//!  override find_inner
//! ```
//!
//! # 4. turning an optional component on and off ([`OptionComponentInner`])
//!
//! creating a component waits on the server, but `diff` can't wait, so turning one on starts a
//! background task. the config can keep changing while it runs, so a finished component sits in
//! `Built` (holding the config it was made from) until the next diff catches it up. `frame`
//! already runs it while it's `Built`.
//!
//! ```text
//!    create_inner(None)                                          create_inner(Some)
//!           |                                                            |
//!           v                                                            v
//!      +--------+  diff(Some): +----------+  task  +-------+ next diff: +---------+
//!      | Absent |------------->| Creating |------->| Built |----------->| Present |
//!      +--------+  spawn task  +----------+  done  +-------+  catch up  +---------+
//!           ^                       |                                        |
//!           |                       | task error                             |
//!           |                       v                                        |
//!           |                  +--------+                                    |
//!           |                  | Failed |                                    |
//!           |                  +--------+                                    |
//!           |                                                                |
//!           +---- diff(None), from any state: back to Absent, stops task ----+
//! ```

use crate::{Animatable, AnimatedElement, Context, CreateInnerInfo, Transformable, ValidState};
use futures::FutureExt;
use stardust_xr_fusion::{
	client::FrameInfo,
	fields::{Field, FieldExt, Shape},
	query::QueryableObject,
	spatial::{Spatial, SpatialExt, SpatialRef, Transform},
};
use std::{
	any::{Any, TypeId},
	convert::Infallible,
	error::Error,
	fmt::Debug,
	marker::PhantomData,
};
use tokio::task::JoinHandle;

// ---------------------------------------------------------------------------------------------
// the component trait
// ---------------------------------------------------------------------------------------------

/// the entity's spatial, field, queryable and transform, borrowed.
/// every component gets this in `create_inner` and `diff`
#[derive(Clone, Copy)]
pub struct ComponentCreateInfo<'a> {
	/// the space the entity is placed in
	pub parent_space: &'a SpatialRef,
	/// sits between `parent_space` and `spatial`. `Containable` moves this one,
	/// so the entity's own transform never has to change
	pub anchor: &'a Spatial,
	pub anchor_space: &'a SpatialRef,
	/// the entity's spatial. its transform is the entity's transform
	pub spatial: &'a Spatial,
	/// the entity's field
	pub field: &'a Field,
	/// the entity's queryable. call `add_interface` on it to expose a protocol
	pub queryable: &'a QueryableObject,
	/// the entity's transform, relative to `anchor`
	pub transform: &'a Transform,
}

pub trait Component<State: ValidState>: Any + Debug + Send + Sync + Sized + 'static {
	/// the component's live state: server objects, callbacks, anything not in the config
	type Inner: Send + Sync + 'static;
	type Error: Error + Send + Sync + 'static;

	/// build the inner the first time this component appears
	fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> impl Future<Output = Result<Self::Inner, Self::Error>> + Send + Sync;

	/// the config changed from `old_self` to `self`. update the inner to match
	fn diff(
		&self,
		old_self: &Self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	);

	/// runs once per server frame
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	/// if `ty` is this component's type, return its inner. tuples and `Option` override this to
	/// search inside themselves. nothing outside this crate can override it (see [`sealed::Token`])
	#[doc(hidden)]
	fn find_inner(
		inner: &Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&(dyn Any + Send + Sync)> {
		(TypeId::of::<Self>() == ty).then_some(inner as &(dyn Any + Send + Sync))
	}
	#[doc(hidden)]
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		(TypeId::of::<Self>() == ty).then_some(inner as &mut (dyn Any + Send + Sync))
	}
}

mod sealed {
	/// a type only this crate can name. `find_inner` takes one as an argument, so code outside
	/// the crate can't override it and claim to be a different component
	pub struct Token;
}

// ---------------------------------------------------------------------------------------------
// finding other components
// ---------------------------------------------------------------------------------------------

/// every component's inner on one entity, as the nested tuple `(((), A::Inner), B::Inner)`
pub(crate) struct ComponentInners<State: ValidState, C: Component<State>> {
	tuple: C::Inner,
	_state: PhantomData<fn() -> State>,
}
impl<State: ValidState, C: Component<State>> ComponentInners<State, C> {
	pub(crate) fn new(tuple: C::Inner) -> Self {
		ComponentInners {
			tuple,
			_state: PhantomData,
		}
	}
}

/// lets [`Inners`] search a [`ComponentInners`] without knowing which components it holds
pub(crate) trait InnerLookup<State: ValidState>: Send + Sync {
	fn find(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)>;
	fn find_mut(&mut self, ty: TypeId) -> Option<&mut (dyn Any + Send + Sync)>;
}
impl<State: ValidState, C: Component<State>> InnerLookup<State> for ComponentInners<State, C> {
	fn find(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)> {
		C::find_inner(&self.tuple, ty, sealed::Token)
	}
	fn find_mut(&mut self, ty: TypeId) -> Option<&mut (dyn Any + Send + Sync)> {
		C::find_inner_mut(&mut self.tuple, ty, sealed::Token)
	}
}

/// what a component gets in `diff` and `frame`: its own inner, plus the other components' inners
///
/// each method borrows `self`, so you can only hold one inner at a time.
/// read what you need from another component first, then get your own
pub struct Inners<'a, State: ValidState, Own: ?Sized> {
	lookup: &'a mut dyn InnerLookup<State>,
	_own: PhantomData<fn() -> Box<Own>>,
}
impl<'a, State: ValidState, Own: Component<State>> Inners<'a, State, Own> {
	pub(crate) fn new(lookup: &'a mut dyn InnerLookup<State>) -> Self {
		Inners {
			lookup,
			_own: PhantomData,
		}
	}
	/// this component's own inner. never fails: every inner exists before diff or frame runs
	pub fn self_inner(&mut self) -> &mut Own::Inner {
		self.lookup
			.find_mut(TypeId::of::<Own>())
			.and_then(|inner| inner.downcast_mut())
			.expect("component inner missing from its own entity")
	}
	/// another component's inner, if the entity has that component
	pub fn get<C: Component<State>>(&self) -> Option<&C::Inner> {
		self.lookup.find(TypeId::of::<C>())?.downcast_ref()
	}
	pub fn get_mut<C: Component<State>>(&mut self) -> Option<&mut C::Inner> {
		self.lookup.find_mut(TypeId::of::<C>())?.downcast_mut()
	}
	/// the same lookup with `C` as "own". tuples and `Option` use this to call diff/frame on the
	/// components inside them
	fn view_as<C: Component<State>>(&mut self) -> Inners<'_, State, C> {
		Inners {
			lookup: self.lookup,
			_own: PhantomData,
		}
	}
}

// ---------------------------------------------------------------------------------------------
// the entity
// ---------------------------------------------------------------------------------------------

#[derive(Animatable)]
pub struct Entity<State: ValidState, C: Component<State>> {
	transform: Transform,
	field_shape: Shape,
	components: C,
	state_phantom: PhantomData<State>,
}
impl<State: ValidState> Entity<State, ()> {
	pub fn new(field_shape: Shape) -> Self {
		Self {
			transform: Transform::IDENTITY,
			field_shape,
			components: (),
			state_phantom: PhantomData,
		}
	}
}
impl<State: ValidState, C: Component<State>> Entity<State, C> {
	/// add a component. it uses this entity's spatial, field and queryable
	pub fn component<NC: Component<State>>(self, component: NC) -> Entity<State, (C, NC)> {
		Entity {
			transform: self.transform,
			field_shape: self.field_shape,
			components: (self.components, component),
			state_phantom: PhantomData,
		}
	}
}
// written by hand because derive would require `State: Clone`
impl<State: ValidState, C: Component<State> + Clone> Clone for Entity<State, C> {
	fn clone(&self) -> Self {
		Self {
			transform: self.transform,
			field_shape: self.field_shape.clone(),
			components: self.components.clone(),
			state_phantom: PhantomData,
		}
	}
}
// written by hand because derive would require `State: Debug`
impl<State: ValidState, C: Component<State>> Debug for Entity<State, C> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Entity")
			.field("transform", &self.transform)
			.field("field_shape", &self.field_shape)
			.field("components", &self.components)
			.finish()
	}
}
impl<State: ValidState, C: Component<State>> Transformable for Entity<State, C> {
	fn transform(&self) -> &Transform {
		&self.transform
	}
	fn transform_mut(&mut self) -> &mut Transform {
		&mut self.transform
	}
}

/// the entity's live state. see [`ComponentCreateInfo`] for what each server object is for
pub struct EntityInner<State: ValidState, C: Component<State>> {
	parent_space: SpatialRef,
	anchor: Spatial,
	anchor_space: SpatialRef,
	spatial: Spatial,
	field: Field,
	queryable: QueryableObject,
	component_inners: ComponentInners<State, C>,
}
impl<State: ValidState, C: Component<State>> EntityInner<State, C> {
	/// borrow the server objects for the components, and the components' inners, at the same time
	fn split<'a>(
		&'a mut self,
		transform: &'a Transform,
	) -> (ComponentCreateInfo<'a>, &'a mut ComponentInners<State, C>) {
		let info = ComponentCreateInfo {
			parent_space: &self.parent_space,
			anchor: &self.anchor,
			anchor_space: &self.anchor_space,
			spatial: &self.spatial,
			field: &self.field,
			queryable: &self.queryable,
			transform,
		};
		(info, &mut self.component_inners)
	}
}

impl<State: ValidState, C: Component<State>> AnimatedElement<State> for Entity<State, C> {
	type Inner = EntityInner<State, C>;
	type Error = AnyError;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let client = &context.stardust_client;

		// parent_space -> anchor -> spatial
		let (anchor, anchor_space) = Spatial::new(client, &info.parent_space, Transform::IDENTITY)
			.await
			.map_err(AnyError::new)?;
		let spatial = info.child_space;
		spatial
			.set_parent(anchor_space.clone())
			.map_err(AnyError::new)?;
		spatial
			.set_relative_transform(anchor_space.clone(), self.transform)
			.map_err(AnyError::new)?;

		let (field, _field_ref) = Field::new(client, &spatial, self.field_shape.clone())
			.await
			.map_err(AnyError::new)?;

		let queryable = client
			.query_interface()
			.register_queryable(spatial.clone(), field.clone())
			.await
			// first error: the message didn't get through. second: the server said no
			.map_err(AnyError::new)?
			.map_err(AnyError::new)?;

		let parent_space = info.parent_space;
		let create_info = ComponentCreateInfo {
			parent_space: &parent_space,
			anchor: &anchor,
			anchor_space: &anchor_space,
			spatial: &spatial,
			field: &field,
			queryable: &queryable,
			transform: &self.transform,
		};
		let tuple =
			(self.components.create_inner(context, create_info).await).map_err(AnyError::new)?;

		Ok(EntityInner {
			parent_space,
			anchor,
			anchor_space,
			spatial,
			field,
			queryable,
			component_inners: ComponentInners::new(tuple),
		})
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		self.apply_transform(old_self, &inner.spatial, &inner.anchor_space);
		if self.field_shape != old_self.field_shape {
			let _ = inner.field.set_shape(self.field_shape.clone());
		}
		let (info, component_inners) = inner.split(&self.transform);
		self.components.diff(
			&old_self.components,
			context,
			info,
			&mut Inners::new(component_inners),
		);
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		self.components.frame(
			context,
			info,
			state,
			&mut Inners::new(&mut inner.component_inners),
		);
	}
}

// ---------------------------------------------------------------------------------------------
// combining components: (), (A, B) and Option<C>
// ---------------------------------------------------------------------------------------------

/// no components. every entity starts from this
impl<State: ValidState> Component<State> for () {
	type Inner = ();
	type Error = Infallible;

	async fn create_inner(
		&self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		Ok(())
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}
}

/// two components. [`Entity::component`] nests these: `(((), A), B)`
impl<State: ValidState, A: Component<State>, B: Component<State>> Component<State> for (A, B) {
	type Inner = (A::Inner, B::Inner);
	type Error = AnyError;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let a = (self.0.create_inner(context, info).await).map_err(AnyError::new)?;
		let b = (self.1.create_inner(context, info).await).map_err(AnyError::new)?;
		Ok((a, b))
	}

	fn diff(
		&self,
		old_self: &Self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		self.0
			.diff(&old_self.0, context, info, &mut inners.view_as());
		self.1
			.diff(&old_self.1, context, info, &mut inners.view_as());
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		self.0.frame(context, info, state, &mut inners.view_as());
		self.1.frame(context, info, state, &mut inners.view_as());
	}

	/// search B first, then A, so the most recently added component wins
	fn find_inner(
		inner: &Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&(dyn Any + Send + Sync)> {
		B::find_inner(&inner.1, ty, sealed::Token)
			.or_else(|| A::find_inner(&inner.0, ty, sealed::Token))
	}
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		let (a, b) = inner;
		B::find_inner_mut(b, ty, sealed::Token).or_else(|| A::find_inner_mut(a, ty, sealed::Token))
	}
}

/// the inner of an `Option<C>` component.
///
/// creating a component waits on the server, but `diff` can't wait. so when `None` becomes
/// `Some`, diff starts a background task to create it, and later diffs/frames check on it.
///
/// ```text
/// create_inner:  Some -> Present         None -> Absent
/// diff(Some):    Absent -> Creating      Built -> Present
/// task finishes: Creating -> Built       Creating -> Failed (if it errored)
/// diff(None):    any state -> Absent     (stops the task if there is one)
/// ```
pub enum OptionComponentInner<State: ValidState, C: Component<State>> {
	/// the config is `None`
	Absent,
	/// the config is `Some`, and the background task is still creating it
	Creating(PendingCreation<State, C>),
	/// the task finished. `inner` was made from `built_from`, the config from when the task
	/// started. the next diff updates it from `built_from` to the current config and moves it to
	/// `Present`. (frame can't do that itself, because it doesn't get a `ComponentCreateInfo`)
	Built { inner: C::Inner, built_from: C },
	/// created and up to date with the config
	Present(C::Inner),
	/// the task failed. it isn't retried until the config goes to `None` and back to `Some`
	Failed,
}

impl<State: ValidState, C: Component<State>> OptionComponentInner<State, C> {
	/// the inner, if it exists yet
	fn live(&self) -> Option<&C::Inner> {
		match self {
			Self::Built { inner, .. } | Self::Present(inner) => Some(inner),
			Self::Absent | Self::Creating(_) | Self::Failed => None,
		}
	}
	fn live_mut(&mut self) -> Option<&mut C::Inner> {
		match self {
			Self::Built { inner, .. } | Self::Present(inner) => Some(inner),
			Self::Absent | Self::Creating(_) | Self::Failed => None,
		}
	}

	/// if the background task finished, move to `Built` or `Failed`. called from both diff and
	/// frame, so a component that finishes between them can already run this frame
	fn poll_creation(&mut self) {
		let Self::Creating(pending) = self else {
			return;
		};
		let Some(outcome) = pending.try_take() else {
			return;
		};
		*self = match outcome {
			Ok((built_from, inner)) => Self::Built { inner, built_from },
			Err(err) => {
				tracing::warn!(
					component_type = std::any::type_name::<C>(),
					%err,
					"failed to create optional component, leaving it off"
				);
				Self::Failed
			}
		};
	}
}

/// the background task creating an optional component. it returns the config it started from
/// along with the result. dropping this stops the task
pub struct PendingCreation<State: ValidState, C: Component<State>>(
	#[allow(clippy::type_complexity)] JoinHandle<(C, Result<C::Inner, C::Error>)>,
);
impl<State: ValidState, C: Component<State> + Clone> PendingCreation<State, C> {
	fn spawn(component: C, context: &Context, info: ComponentCreateInfo<'_>) -> Self {
		// the task can outlive this call, so it needs its own copies of everything
		let context = context.clone();
		let parent_space = info.parent_space.clone();
		let anchor = info.anchor.clone();
		let anchor_space = info.anchor_space.clone();
		let spatial = info.spatial.clone();
		let field = info.field.clone();
		let queryable = info.queryable.clone();
		let transform = *info.transform;
		PendingCreation(tokio::spawn(async move {
			let info = ComponentCreateInfo {
				parent_space: &parent_space,
				anchor: &anchor,
				anchor_space: &anchor_space,
				spatial: &spatial,
				field: &field,
				queryable: &queryable,
				transform: &transform,
			};
			let result = component.create_inner(&context, info).await;
			(component, result)
		}))
	}
}
impl<State: ValidState, C: Component<State>> PendingCreation<State, C> {
	/// `None` while the task is still running
	fn try_take(&mut self) -> Option<Result<(C, C::Inner), AnyError>> {
		if !self.0.is_finished() {
			return None;
		}
		// the task is done, so this poll returns right away. `unconstrained` is needed because
		// tokio can make a poll return "not ready" when the task's poll budget runs out, even
		// though the result is there (more detail in element.rs)
		match tokio::task::unconstrained(&mut self.0).now_or_never()? {
			Ok((built_from, Ok(inner))) => Some(Ok((built_from, inner))),
			Ok((_, Err(err))) => Some(Err(AnyError::new(err))),
			Err(join_error) => Some(Err(AnyError::new(join_error))),
		}
	}
}
impl<State: ValidState, C: Component<State>> Drop for PendingCreation<State, C> {
	fn drop(&mut self) {
		self.0.abort();
	}
}

/// a component that can be turned on (`Some`) and off (`None`)
impl<State: ValidState, C: Component<State> + Clone> Component<State> for Option<C> {
	type Inner = OptionComponentInner<State, C>;
	type Error = C::Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		Ok(match self {
			Some(component) => {
				OptionComponentInner::Present(component.create_inner(context, info).await?)
			}
			None => OptionComponentInner::Absent,
		})
	}

	fn diff(
		&self,
		old_self: &Self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		let state = inners.self_inner();
		state.poll_creation();

		let Some(new_component) = self else {
			// turned off. if a task was running, dropping it here stops it
			*state = OptionComponentInner::Absent;
			return;
		};

		// work out which old config to diff from. `state` has to be done with before calling
		// `C::diff`, because that needs `inners` too
		let built_from = match state {
			OptionComponentInner::Absent => {
				*state = OptionComponentInner::Creating(PendingCreation::spawn(
					new_component.clone(),
					context,
					info,
				));
				return;
			}
			OptionComponentInner::Creating(_) | OptionComponentInner::Failed => return,
			OptionComponentInner::Present(_) => None,
			OptionComponentInner::Built { .. } => {
				let OptionComponentInner::Built { inner, built_from } =
					std::mem::replace(state, OptionComponentInner::Absent)
				else {
					unreachable!()
				};
				*state = OptionComponentInner::Present(inner);
				Some(built_from)
			}
		};
		let Some(from) = built_from.as_ref().or(old_self.as_ref()) else {
			return;
		};
		new_component.diff(from, context, info, &mut inners.view_as());
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let inner = inners.self_inner();
		inner.poll_creation();
		if let Some(component) = self
			&& inner.live().is_some()
		{
			component.frame(context, info, state, &mut inners.view_as());
		}
	}

	/// asking for `Option<C>` gives this enum. asking for `C` gives the inner if it exists, so
	/// other components look up an optional component the same way as a required one
	fn find_inner(
		inner: &Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&(dyn Any + Send + Sync)> {
		if TypeId::of::<Self>() == ty {
			return Some(inner);
		}
		C::find_inner(inner.live()?, ty, sealed::Token)
	}
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: sealed::Token,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		if TypeId::of::<Self>() == ty {
			return Some(inner);
		}
		C::find_inner_mut(inner.live_mut()?, ty, sealed::Token)
	}
}

// ---------------------------------------------------------------------------------------------
// errors
// ---------------------------------------------------------------------------------------------

/// any error, boxed. it exists because an associated `Error` type has to implement `Error`, and
/// `Box<dyn Error>` doesn't
#[derive(Debug)]
pub struct AnyError(pub Box<dyn Error + Send + Sync + 'static>);
impl AnyError {
	pub fn new(error: impl Error + Send + Sync + 'static) -> Self {
		AnyError(Box::new(error))
	}
}
impl std::fmt::Display for AnyError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}
impl Error for AnyError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		self.0.source()
	}
}

#[cfg(test)]
mod lookup_tests {
	use super::*;

	#[derive(Debug, Clone, PartialEq)]
	struct Alpha;
	#[derive(Debug, Clone, PartialEq)]
	struct Beta;

	/// both components use this same inner type, to check lookups go by component, not inner
	#[derive(Debug, PartialEq)]
	struct Shared(u32);

	macro_rules! stub {
		($component:ty) => {
			impl<State: ValidState> Component<State> for $component {
				type Inner = Shared;
				type Error = Infallible;

				async fn create_inner(
					&self,
					_context: &Context,
					_info: ComponentCreateInfo<'_>,
				) -> Result<Self::Inner, Self::Error> {
					unreachable!("these tests only cover lookup")
				}
				fn diff(
					&self,
					_old_self: &Self,
					_context: &Context,
					_info: ComponentCreateInfo<'_>,
					_inners: &mut Inners<'_, State, Self>,
				) {
				}
			}
		};
	}
	stub!(Alpha);
	stub!(Beta);

	type Pair = (((), Alpha), Beta);

	fn pair() -> ComponentInners<(), Pair> {
		ComponentInners::new((((), Shared(1)), Shared(2)))
	}

	#[test]
	fn finds_each_component_despite_a_shared_inner_type() {
		let mut tuple = pair();
		let inners = Inners::<'_, (), Alpha>::new(&mut tuple);
		assert_eq!(inners.get::<Alpha>(), Some(&Shared(1)));
		assert_eq!(inners.get::<Beta>(), Some(&Shared(2)));
	}

	#[test]
	fn self_inner_needs_no_turbofish() {
		let mut tuple = pair();
		let mut inners = Inners::<'_, (), Beta>::new(&mut tuple);
		assert_eq!(inners.self_inner(), &mut Shared(2));
	}

	#[test]
	fn option_is_transparent() {
		type Opt = (((), Alpha), Option<Beta>);

		let mut present = ComponentInners::<(), Opt>::new((
			((), Shared(1)),
			OptionComponentInner::Present(Shared(2)),
		));
		let inners = Inners::<'_, (), Alpha>::new(&mut present);
		// looked up as Beta, not Option<Beta>, same as a required component
		assert_eq!(inners.get::<Beta>(), Some(&Shared(2)));

		let mut built = ComponentInners::<(), Opt>::new((
			((), Shared(1)),
			OptionComponentInner::Built {
				inner: Shared(2),
				built_from: Beta,
			},
		));
		let inners = Inners::<'_, (), Alpha>::new(&mut built);
		// built but not diffed yet still counts as there
		assert_eq!(inners.get::<Beta>(), Some(&Shared(2)));

		let mut absent =
			ComponentInners::<(), Opt>::new((((), Shared(1)), OptionComponentInner::Absent));
		let inners = Inners::<'_, (), Alpha>::new(&mut absent);
		assert_eq!(inners.get::<Beta>(), None);
		assert_eq!(inners.get::<Alpha>(), Some(&Shared(1)));
	}
}
