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
use std::{any::Any, convert::Infallible, error::Error, fmt::Debug, marker::PhantomData};

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
	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner);
	/// Every frame on the server
	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		_state: &mut State,
		_inner: &mut Self::Inner,
	) {
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

	fn diff(&self, _old_self: &Self, _context: &Context, _inner: &mut Self::Inner) {}
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

	fn diff(&self, old_self: &Self, context: &Context, inner: &mut Self::Inner) {
		self.0.diff(&old_self.0, context, &mut inner.0);
		self.1.diff(&old_self.1, context, &mut inner.1);
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		self.0.frame(context, info, state, &mut inner.0);
		self.1.frame(context, info, state, &mut inner.1);
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
		self.components
			.diff(&old_self.components, context, &mut inner.component_inners);
	}

	fn frame(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner: &mut Self::Inner,
	) {
		self.components
			.frame(context, info, state, &mut inner.component_inners);
	}
}

pub struct EntityInner<State: ValidState, C: Component<State>> {
	spatial: Spatial,
	field: Field,
	// keeps the shared queryable alive for the entity's lifetime; the per-interface guards
	// live inside each component's own inner.
	_queryable: QueryableObject,
	// this matches the components perfectly so no dynamic dispatch
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
