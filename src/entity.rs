// ok so now that we got new asteroids stuff and new queryable stuff
// we should like, actually make the proper declarative ECS for stuff
// that it makes sense for?
//
// anyway entities take in a transform and field, make their own
// field using child space spatial,
//

use stardust_xr_fusion::{
	client::FrameInfo,
	fields::{Field, FieldExt, Shape},
	query::QueryableObject,
	spatial::{Spatial, SpatialRef, Transform},
};

use crate::{Context, CreateInnerInfo, CustomElement, Transformable, ValidState};
use futures::FutureExt;
use std::{
	any::{Any, TypeId},
	convert::Infallible,
	error::Error,
	fmt::Debug,
	marker::PhantomData,
};
use tokio::task::JoinHandle;

/// the one place the root component type gets erased, so Inners doesn't have to be generic over it
mod shadow {
	/// unnameable outside the crate, so nobody out there can override find_inner and lie about
	/// which component they are
	pub struct Shadow;
}
use shadow::Shadow;

pub(crate) trait Stack<State: ValidState>: Send + Sync {
	fn find(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)>;
	fn find_mut(&mut self, ty: TypeId) -> Option<&mut (dyn Any + Send + Sync)>;
}
pub(crate) struct Root<'a, State: ValidState, C: Component<State>>(
	&'a mut C::Inner,
	PhantomData<fn() -> State>,
);
impl<'a, State: ValidState, C: Component<State>> Root<'a, State, C> {
	pub(crate) fn new(inner: &'a mut C::Inner) -> Self {
		Root(inner, PhantomData)
	}
}
impl<State: ValidState, C: Component<State>> Stack<State> for Root<'_, State, C> {
	fn find(&self, ty: TypeId) -> Option<&(dyn Any + Send + Sync)> {
		C::find_inner(self.0, ty, Shadow)
	}
	fn find_mut(&mut self, ty: TypeId) -> Option<&mut (dyn Any + Send + Sync)> {
		C::find_inner_mut(self.0, ty, Shadow)
	}
}

/// every component inner on the entity, from Own's point of view
///
/// accessors borrow self, so you can't hold two at once — read what you need off a sibling, then
/// take your own
pub struct Inners<'a, State: ValidState, Own: ?Sized> {
	stack: &'a mut dyn Stack<State>,
	own: PhantomData<fn() -> Box<Own>>,
}
impl<'a, State: ValidState, Own: Component<State>> Inners<'a, State, Own> {
	pub(crate) fn new(stack: &'a mut dyn Stack<State>) -> Self {
		Inners {
			stack,
			own: PhantomData,
		}
	}
	/// infallible — the entity has already placed every inner by the time diff or frame runs
	pub fn self_inner(&mut self) -> &mut Own::Inner {
		self.stack
			.find_mut(TypeId::of::<Own>())
			.and_then(|inner| inner.downcast_mut())
			.expect("component inner missing from its own entity")
	}
	pub fn get<C: Component<State>>(&self) -> Option<&C::Inner> {
		self.stack.find(TypeId::of::<C>())?.downcast_ref()
	}
	pub fn get_mut<C: Component<State>>(&mut self) -> Option<&mut C::Inner> {
		self.stack.find_mut(TypeId::of::<C>())?.downcast_mut()
	}
	fn retype<C: Component<State>>(&mut self) -> Inners<'_, State, C> {
		Inners {
			stack: self.stack,
			own: PhantomData,
		}
	}
}

/// Unifies the heterogeneous component/field/queryable errors into one concrete `Error` type
/// (`Box<dyn Error>` itself doesn't implement `std::error::Error`, so we can't use it directly
/// as an associated `Error`).
#[derive(Debug)]
pub struct BoxError(pub Box<dyn Error + Send + Sync + 'static>);
impl BoxError {
	pub fn new(error: impl Error + Send + Sync + 'static) -> Self {
		BoxError(Box::new(error))
	}
}
impl std::fmt::Display for BoxError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		std::fmt::Display::fmt(&self.0, f)
	}
}
impl Error for BoxError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		self.0.source()
	}
}

/// The shared resources an [`Entity`] hands to each of its [`Component`]s at creation.
///
/// Everything is borrowed, so this is `Copy` and can be passed to every component in turn
/// (including down through the tuple impls).
#[derive(Clone, Copy)]
pub struct ComponentCreateInfo<'a> {
	pub parent_space: &'a SpatialRef,
	/// The entity's shared spatial.
	pub spatial: &'a Spatial,
	/// The entity's shared field.
	pub field: &'a Field,
	/// The entity's shared queryable — `add_interface` onto this to expose protocols.
	pub queryable: &'a QueryableObject,
}

pub trait Component<State: ValidState>: Any + Debug + Send + Sync + Sized + 'static {
	/// The imperative struct containing non-saved state
	type Inner: Send + Sync + 'static;
	/// Error type for the element
	type Error: Error + Send + Sync + 'static;
	/// Create the inner imperative struct, reusing the entity's shared spatial/field/queryable.
	fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> impl Future<Output = Result<Self::Inner, Self::Error>> + Send + Sync;
	/// Update the inner imperative struct with the new state of the node.
	/// You will need to check for changes between `self` and `old_self` and update accordingly.
	///
	/// `info` carries the entity's shared spatial/field/queryable so a component can
	/// (re)create itself live — notably [`Option<C>`] uses it to spawn a component that was
	/// just toggled on (`None` -> `Some`).
	fn diff(
		&self,
		old_self: &Self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	);
	/// Every frame on the server
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	/// how a component answers a sibling lookup, sealed so only the scaffolding in here overrides it
	#[doc(hidden)]
	fn find_inner(inner: &Self::Inner, ty: TypeId, _: Shadow) -> Option<&(dyn Any + Send + Sync)> {
		(TypeId::of::<Self>() == ty).then_some(inner as &(dyn Any + Send + Sync))
	}
	#[doc(hidden)]
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: Shadow,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		(TypeId::of::<Self>() == ty).then_some(inner as &mut (dyn Any + Send + Sync))
	}
}

// implement Component for tuples so they can be composed in a builder (like ElementWrapper)

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

/// Inner state for an [`Option<C>`] component.
///
/// Component creation is async (server round-trips) but [`Component::diff`] is synchronous, so a
/// `None` -> `Some` toggle can't build the inner inline. Instead it spawns the creation and parks
/// the [`JoinHandle`] in `Creating`, which either [`Component::frame`] or [`Component::diff`]
/// finalizes into `Present` once the task completes — mirroring the element-level
/// `ElementInner::Creating` pattern.
/// An in-flight component creation: yields the (cloned) config it was created from alongside the
/// creation result, so finalization can diff from the creation-time config to the current one.
#[allow(type_alias_bounds)]
type ComponentCreation<State: ValidState, C: Component<State>> =
	JoinHandle<(C, Result<C::Inner, C::Error>)>;

pub enum OptionComponentInner<State: ValidState, C: Component<State>> {
	Absent,
	/// Async creation in flight. Returns the config it was created from alongside the result, so
	/// finalization can diff from the *creation-time* config to the current one (config may have
	/// changed during the async window).
	Creating(ComponentCreation<State, C>),
	Present {
		inner: C::Inner,
		/// The config this inner was created from, if not yet reconciled against the current
		/// declarative config. Set when `frame` finalizes a creation (it has no
		/// `ComponentCreateInfo` to diff with); cleared by the next `diff`.
		created_from: Option<C>,
	},
}

impl<State: ValidState, C: Component<State> + Clone> Component<State> for Option<C> {
	type Inner = OptionComponentInner<State, C>;
	type Error = C::Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		match self {
			Some(component) => Ok(OptionComponentInner::Present {
				inner: component.create_inner(context, info).await?,
				// created from the current config, nothing to reconcile
				created_from: None,
			}),
			None => Ok(OptionComponentInner::Absent),
		}
	}

	fn diff(
		&self,
		old_self: &Self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
		inners: &mut Inners<'_, State, Self>,
	) {
		finalize_option::<State, C>(inners);
		// decide while we hold our own inner, then drop that borrow so the forwarded diff can
		// reach the whole stack
		let reconcile_from = {
			match (self, inners.self_inner()) {
				(Some(_), OptionComponentInner::Present { created_from, .. }) => {
					created_from.take().or_else(|| old_self.clone())
				}
				// still in flight, nothing to reconcile against yet
				(Some(_), OptionComponentInner::Creating(_)) => None,
				// None -> Some: spawn the async creation and park the handle
				(Some(new_component), state @ OptionComponentInner::Absent) => {
					*state = OptionComponentInner::Creating(spawn_create_component(
						new_component.clone(),
						context,
						info,
					));
					None
				}
				// Some -> None (or staying None): tear down, aborting an in-flight creation
				(None, state) => {
					if let OptionComponentInner::Creating(handle) = state {
						handle.abort();
					}
					*state = OptionComponentInner::Absent;
					None
				}
			}
		};
		if let (Some(new_component), Some(from)) = (self, reconcile_from) {
			new_component.diff(&from, context, info, &mut inners.retype());
		}
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		finalize_option::<State, C>(inners);
		let present = matches!(inners.self_inner(), OptionComponentInner::Present { .. });
		if let (Some(component), true) = (self, present) {
			component.frame(context, info, state, &mut inners.retype());
		}
	}

	/// answers to Option<C> with the state machine and to C with the live inner, so a sibling names
	/// an optional component the same way it names a required one
	fn find_inner(inner: &Self::Inner, ty: TypeId, _: Shadow) -> Option<&(dyn Any + Send + Sync)> {
		if TypeId::of::<Self>() == ty {
			return Some(inner);
		}
		match inner {
			OptionComponentInner::Present { inner, .. } => C::find_inner(inner, ty, Shadow),
			_ => None,
		}
	}
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: Shadow,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		if TypeId::of::<Self>() == ty {
			return Some(inner);
		}
		match inner {
			OptionComponentInner::Present { inner, .. } => C::find_inner_mut(inner, ty, Shadow),
			_ => None,
		}
	}
}

/// runs from both diff and frame so a component that lands between them still gets to run this
/// frame
fn finalize_option<State: ValidState, C: Component<State> + Clone>(
	inners: &mut Inners<'_, State, Option<C>>,
) {
	let state = inners.self_inner();
	if !matches!(state, OptionComponentInner::Creating(handle) if handle.is_finished()) {
		return;
	}
	let OptionComponentInner::Creating(handle) =
		std::mem::replace(state, OptionComponentInner::Absent)
	else {
		return;
	};
	// the task is finished, so `now_or_never` resolves immediately. On creation error or a
	// panicked/aborted task we just stay `Absent`. `unconstrained` exempts this poll from tokio's
	// cooperative scheduling budget — see element.rs for why `is_finished()` alone isn't enough.
	if let Some(Ok((decl, Ok(inner)))) = tokio::task::unconstrained(handle).now_or_never() {
		*state = OptionComponentInner::Present {
			inner,
			created_from: Some(decl),
		};
	}
}

/// Spawn a component's async creation, owning everything it borrows so the future is `'static`.
/// Returns the (cloned) config alongside the result so finalization can diff from it to the
/// current config.
fn spawn_create_component<State: ValidState, C: Component<State> + Clone>(
	component: C,
	context: &Context,
	info: ComponentCreateInfo<'_>,
) -> ComponentCreation<State, C> {
	let context = context.clone();
	let parent_space = info.parent_space.clone();
	let spatial = info.spatial.clone();
	let field = info.field.clone();
	let queryable = info.queryable.clone();
	tokio::spawn(async move {
		let info = ComponentCreateInfo {
			parent_space: &parent_space,
			spatial: &spatial,
			field: &field,
			queryable: &queryable,
		};
		// `create_inner` only borrows `component`, so we can hand it back for reconciliation
		let result = component.create_inner(&context, info).await;
		(component, result)
	})
}

impl<State: ValidState, A: Component<State>, B: Component<State>> Component<State> for (A, B) {
	type Inner = (A::Inner, B::Inner);
	type Error = BoxError;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let a = self
			.0
			.create_inner(context, info)
			.await
			.map_err(BoxError::new)?;
		let b = self
			.1
			.create_inner(context, info)
			.await
			.map_err(BoxError::new)?;
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
			.diff(&old_self.0, context, info, &mut inners.retype());
		self.1
			.diff(&old_self.1, context, info, &mut inners.retype());
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		self.0.frame(context, info, state, &mut inners.retype());
		self.1.frame(context, info, state, &mut inners.retype());
	}

	/// the walk recurses here, newest component first
	fn find_inner(inner: &Self::Inner, ty: TypeId, _: Shadow) -> Option<&(dyn Any + Send + Sync)> {
		B::find_inner(&inner.1, ty, Shadow).or_else(|| A::find_inner(&inner.0, ty, Shadow))
	}
	fn find_inner_mut(
		inner: &mut Self::Inner,
		ty: TypeId,
		_: Shadow,
	) -> Option<&mut (dyn Any + Send + Sync)> {
		if let Some(found) = B::find_inner_mut(&mut inner.1, ty, Shadow) {
			return Some(found);
		}
		A::find_inner_mut(&mut inner.0, ty, Shadow)
	}
}

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
	/// Add a component, sharing this entity's spatial/field/queryable with it.
	pub fn component<NC: Component<State>>(self, component: NC) -> Entity<State, (C, NC)> {
		Entity {
			transform: self.transform,
			field_shape: self.field_shape,
			components: (self.components, component),
			state_phantom: PhantomData,
		}
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
impl<State: ValidState, C: Component<State>> Debug for Entity<State, C> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Entity")
			.field("transform", &self.transform)
			.field("field_shape", &self.field_shape)
			.field("components", &self.components)
			.finish()
	}
}
impl<State: ValidState, C: Component<State>> CustomElement<State> for Entity<State, C> {
	type Inner = EntityInner<State, C>;
	type Error = BoxError;

	async fn create_inner(
		&self,
		context: &Context,
		info: CreateInnerInfo,
	) -> Result<Self::Inner, Self::Error> {
		let spatial = info.child_space;
		spatial
			.set_local_transform(self.transform)
			.map_err(BoxError::new)?;

		let (field, _field_ref) =
			Field::new(&context.stardust_client, &spatial, self.field_shape.clone())
				.await
				.map_err(BoxError::new)?;

		// one shared queryable for the whole entity — components add their interfaces onto it
		let queryable = context
			.stardust_client
			.query_interface()
			.register_queryable(spatial.clone(), field.clone())
			.await
			.map_err(BoxError::new)?
			.map_err(BoxError::new)?;

		let component_inners = self
			.components
			.create_inner(
				context,
				ComponentCreateInfo {
					parent_space: &info.parent_space,
					spatial: &spatial,
					field: &field,
					queryable: &queryable,
				},
			)
			.await
			.map_err(BoxError::new)?;

		Ok(EntityInner {
			parent_space: info.parent_space,
			spatial,
			field,
			_queryable: queryable,
			component_inners,
		})
	}

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		if self.transform != old_self.transform {
			let _ = inner.spatial.set_local_transform(self.transform);
		}
		if self.field_shape != old_self.field_shape {
			let _ = inner.field.set_shape(self.field_shape.clone());
		}
		// rebuild the shared creation info so components can recreate themselves live
		let info = ComponentCreateInfo {
			parent_space: &inner.parent_space,
			spatial: &inner.spatial,
			field: &inner.field,
			queryable: &inner._queryable,
		};
		self.components.diff(
			&old_self.components,
			context,
			info,
			&mut Inners::new(&mut Root::<State, C>::new(&mut inner.component_inners)),
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
			&mut Inners::new(&mut Root::<State, C>::new(&mut inner.component_inners)),
		);
	}
}

pub struct EntityInner<State: ValidState, C: Component<State>> {
	// the entity's stationary parent space, kept so toggled-on components can be (re)created
	parent_space: SpatialRef,
	spatial: Spatial,
	field: Field,
	// keeps the shared queryable alive for the entity's lifetime; the per-interface guards
	// live inside each component's own inner.
	_queryable: QueryableObject,
	// still the exact component tuple — Inners only walks it, nothing gets boxed
	component_inners: <C as Component<State>>::Inner,
}

// so basically the idea is we make grabbable and derezzable and such
// use the provided field/spatial and then in their callbacks it edits
// the state which then gets passed into the entity transform and such
// so then you can compose pieces of an object together
// where it makes sense to, since sometimes the elements are just
// too rigid, like on the root of whole objects
//
// like, if i had a panel that was derezzable and grabbable and reparentable
// right now those all overlap a TON but duplicated code means
// spatial nesting which isn't quite right... it's all the same object
// but sometimes you wanna vary it so optional components would be super nice

#[cfg(test)]
mod inner_stack_tests {
	use super::*;

	#[derive(Debug, Clone, PartialEq)]
	struct Alpha;
	#[derive(Debug, Clone, PartialEq)]
	struct Beta;

	/// deliberately the same inner type for both, to prove lookups key on the component
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
					unreachable!("the walk is what's under test, not creation")
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

	fn stack() -> <Pair as Component<()>>::Inner {
		(((), Shared(1)), Shared(2))
	}

	#[test]
	fn finds_each_component_despite_a_shared_inner_type() {
		let mut s = stack();
		let mut root = Root::<(), Pair>::new(&mut s);
		let inners = Inners::<'_, (), Alpha>::new(&mut root);
		assert_eq!(inners.get::<Alpha>(), Some(&Shared(1)));
		assert_eq!(inners.get::<Beta>(), Some(&Shared(2)));
	}

	#[test]
	fn self_inner_needs_no_turbofish() {
		let mut s = stack();
		let mut root = Root::<(), Pair>::new(&mut s);
		let mut inners = Inners::<'_, (), Beta>::new(&mut root);
		assert_eq!(inners.self_inner(), &mut Shared(2));
	}

	#[test]
	fn option_is_transparent() {
		type Opt = (((), Alpha), Option<Beta>);

		let mut present = (
			((), Shared(1)),
			OptionComponentInner::<(), Beta>::Present {
				inner: Shared(2),
				created_from: None,
			},
		);
		let mut root = Root::<(), Opt>::new(&mut present);
		let inners = Inners::<'_, (), Alpha>::new(&mut root);
		// named as Beta, not Option<Beta>, exactly like a required component
		assert_eq!(inners.get::<Beta>(), Some(&Shared(2)));

		let mut absent = (((), Shared(1)), OptionComponentInner::<(), Beta>::Absent);
		let mut root = Root::<(), Opt>::new(&mut absent);
		let inners = Inners::<'_, (), Alpha>::new(&mut root);
		assert_eq!(inners.get::<Beta>(), None);
		assert_eq!(inners.get::<Alpha>(), Some(&Shared(1)));
	}
}
