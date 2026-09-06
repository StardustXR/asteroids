use crate::{
	CloneFnWrapper, Component, ComponentCreateInfo, Context, Inners, ValidState,
	components::{Grabbable, GrabbableInner},
};
use gluon::{HandledBy, Handler, RefExt};
use stardust_xr_fusion::{
	Error,
	client::{Client, ClientHandler, FrameInfo},
	query::QueryableObject,
	spatial::{PartialTransform, Spatial, SpatialInterface, SpatialRef, Transform},
	types::{Posef, QuatF, Vec3F},
};
use stardust_xr_molecules::transformable::protocol::{
	self, PoseableHandler, RotatableHandler, ScalableHandler, TransformableHandler,
	TranslatableHandler,
};
use std::sync::{Arc, Mutex};

type OnChange<State, T> = CloneFnWrapper<dyn Fn(&mut State, T) + Send + Sync>;

/// lets other clients move the entity around however they like, reporting the new transform back
/// so `State` stays the thing that actually owns the pose
///
/// the whole hierarchy comes with it, [`Poseable`] and [`Scalable`] and everything under them, so
/// don't add any of those alongside
#[derive_where::derive_where(Debug, Clone)]
pub struct Transformable<State: ValidState> {
	on_transform: OnChange<State, Transform>,
}
impl<State: ValidState> Transformable<State> {
	pub fn new(on_transform: impl Fn(&mut State, Transform) + Send + Sync + 'static) -> Self {
		Self {
			on_transform: CloneFnWrapper(Arc::new(on_transform)),
		}
	}
}
impl<State: ValidState> Component<State> for Transformable<State> {
	type Inner = TransformableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let mut inner = TransformableInner::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.parent_space.clone(),
		);
		inner.transformable(info.queryable).await?;
		Ok(inner)
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let Some(transform) = take_pending(inners) else {
			return;
		};
		(self.on_transform.0)(state, transform);
	}
}

/// pose only, for when scale isn't the entity's to hand out
///
/// [`Translatable`] and [`Rotatable`] come with it, so don't add either alongside
#[derive_where::derive_where(Debug, Clone)]
pub struct Poseable<State: ValidState> {
	on_pose: OnChange<State, Posef>,
}
impl<State: ValidState> Poseable<State> {
	pub fn new(on_pose: impl Fn(&mut State, Posef) + Send + Sync + 'static) -> Self {
		Self {
			on_pose: CloneFnWrapper(Arc::new(on_pose)),
		}
	}
}
impl<State: ValidState> Component<State> for Poseable<State> {
	type Inner = TransformableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let mut inner = TransformableInner::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.parent_space.clone(),
		);
		inner.poseable(info.queryable).await?;
		Ok(inner)
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let Some(transform) = take_pending(inners) else {
			return;
		};
		(self.on_pose.0)(
			state,
			Posef {
				position: transform.translation,
				orientation: transform.rotation,
			},
		);
	}
}

/// translation only
#[derive_where::derive_where(Debug, Clone)]
pub struct Translatable<State: ValidState> {
	on_translation: OnChange<State, Vec3F>,
}
impl<State: ValidState> Translatable<State> {
	pub fn new(on_translation: impl Fn(&mut State, Vec3F) + Send + Sync + 'static) -> Self {
		Self {
			on_translation: CloneFnWrapper(Arc::new(on_translation)),
		}
	}
}
impl<State: ValidState> Component<State> for Translatable<State> {
	type Inner = TransformableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let mut inner = TransformableInner::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.parent_space.clone(),
		);
		inner.translatable(info.queryable).await?;
		Ok(inner)
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let Some(transform) = take_pending(inners) else {
			return;
		};
		(self.on_translation.0)(state, transform.translation);
	}
}

/// rotation only
#[derive_where::derive_where(Debug, Clone)]
pub struct Rotatable<State: ValidState> {
	on_rotation: OnChange<State, QuatF>,
}
impl<State: ValidState> Rotatable<State> {
	pub fn new(on_rotation: impl Fn(&mut State, QuatF) + Send + Sync + 'static) -> Self {
		Self {
			on_rotation: CloneFnWrapper(Arc::new(on_rotation)),
		}
	}
}
impl<State: ValidState> Component<State> for Rotatable<State> {
	type Inner = TransformableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let mut inner = TransformableInner::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.parent_space.clone(),
		);
		inner.rotatable(info.queryable).await?;
		Ok(inner)
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let Some(transform) = take_pending(inners) else {
			return;
		};
		(self.on_rotation.0)(state, transform.rotation);
	}
}

/// scale only
#[derive_where::derive_where(Debug, Clone)]
pub struct Scalable<State: ValidState> {
	on_scale: OnChange<State, Vec3F>,
}
impl<State: ValidState> Scalable<State> {
	pub fn new(on_scale: impl Fn(&mut State, Vec3F) + Send + Sync + 'static) -> Self {
		Self {
			on_scale: CloneFnWrapper(Arc::new(on_scale)),
		}
	}
}
impl<State: ValidState> Component<State> for Scalable<State> {
	type Inner = TransformableInner;
	type Error = Error;

	async fn create_inner(
		&self,
		context: &Context,
		info: ComponentCreateInfo<'_>,
	) -> Result<Self::Inner, Self::Error> {
		let mut inner = TransformableInner::new(
			&context.stardust_client,
			info.spatial.clone(),
			info.parent_space.clone(),
		);
		inner.scalable(info.queryable).await?;
		Ok(inner)
	}

	fn diff(
		&self,
		_old_self: &Self,
		_context: &Context,
		_info: ComponentCreateInfo<'_>,
		_inners: &mut Inners<'_, State, Self>,
	) {
	}

	fn frame(
		&self,
		_context: &Context,
		_info: &FrameInfo,
		state: &mut State,
		inners: &mut Inners<'_, State, Self>,
	) {
		let Some(transform) = take_pending(inners) else {
			return;
		};
		(self.on_scale.0)(state, transform.scale);
	}
}

fn take_pending<State: ValidState, Own: Component<State, Inner = TransformableInner>>(
	inners: &mut Inners<'_, State, Own>,
) -> Option<Transform> {
	// read the sibling out first, Inners accessors borrow self so we can't hold both
	let grabbing = inners
		.get::<Grabbable<State>>()
		.is_some_and(GrabbableInner::grabbing);

	// a grab owns the pose while it lasts, so anything that came in meanwhile is dropped
	inners.self_inner().take_pending().filter(|_| !grabbing)
}

/// what every component in the hierarchy runs on: one core plus however many nodes it takes to
/// serve the slice of the hierarchy that component asked for
pub struct TransformableInner {
	core: Arc<TransformableCore>,

	/// nothing ever looks in here, the nodes and their advertisements just have to outlive the
	/// entity, and they can't live in the core since a node owns its handler
	served: Vec<Box<dyn Send + Sync>>,
}
impl TransformableInner {
	/// `spatial` is what moves and `parent` is the space the reported transforms come back in,
	/// so a custom element hands in its own two rather than an entity's
	pub fn new<H: ClientHandler>(client: &Client<H>, spatial: Spatial, parent: SpatialRef) -> Self {
		TransformableInner {
			core: Arc::new(TransformableCore {
				spatial_interface: client.spatial_interface().clone(),
				spatial,
				parent,
				pending_transform_change: Mutex::new(None),
			}),
			served: Vec::new(),
		}
	}

	/// whatever came in since the last call, as a local transform of `parent`
	pub fn take_pending(&self) -> Option<Transform> {
		self.core.take_pending()
	}

	async fn serve<I: RefExt + HandledBy<H>, H: Handler>(
		&mut self,
		handler: H,
		queryable: &QueryableObject,
	) -> Result<(), Error> {
		let (node, interface) = I::new_node(handler)?;
		let advertisement = queryable.add_interface(&interface, I::ID).await??;
		self.served.push(Box::new((node, advertisement)));
		Ok(())
	}

	pub async fn transformable(&mut self, queryable: &QueryableObject) -> Result<(), Error> {
		self.serve::<protocol::Transformable, _>(TransformableNode(self.core.clone()), queryable)
			.await?;
		self.poseable(queryable).await?;
		self.scalable(queryable).await
	}
	pub async fn poseable(&mut self, queryable: &QueryableObject) -> Result<(), Error> {
		self.serve::<protocol::Poseable, _>(PoseableNode(self.core.clone()), queryable)
			.await?;
		self.translatable(queryable).await?;
		self.rotatable(queryable).await
	}
	pub async fn translatable(&mut self, queryable: &QueryableObject) -> Result<(), Error> {
		self.serve::<protocol::Translatable, _>(TranslatableNode(self.core.clone()), queryable)
			.await
	}
	pub async fn rotatable(&mut self, queryable: &QueryableObject) -> Result<(), Error> {
		self.serve::<protocol::Rotatable, _>(RotatableNode(self.core.clone()), queryable)
			.await
	}
	pub async fn scalable(&mut self, queryable: &QueryableObject) -> Result<(), Error> {
		self.serve::<protocol::Scalable, _>(ScalableNode(self.core.clone()), queryable)
			.await
	}
}

/// the one pending transform every node in the hierarchy writes into, and the frame math they all
/// share
pub struct TransformableCore {
	spatial_interface: SpatialInterface,
	spatial: Spatial,
	parent: SpatialRef,

	pending_transform_change: Mutex<Option<Transform>>,
}
impl TransformableCore {
	fn take_pending(&self) -> Option<Transform> {
		self.pending_transform_change.lock().unwrap().take()
	}

	/// the reference frame both ways round plus where the entity sits right now, all as local
	/// transforms of the entity's parent space
	///
	/// both directions get fetched rather than inverting one, `Transform::inverse` only round
	/// trips under uniform scale
	async fn frames(&self, reference: SpatialRef) -> Option<(Transform, Transform, Transform)> {
		let (reference_in_parent, parent_in_reference, local) = tokio::join!(
			self.spatial_interface
				.get_relative_transform(self.parent.clone(), reference.clone()),
			self.spatial_interface
				.get_relative_transform(reference, self.parent.clone()),
			self.spatial.get_relative_transform(self.parent.clone()),
		);
		Some((
			reference_in_parent.ok()?.ok()?,
			parent_in_reference.ok()?.ok()?,
			local.ok()?.ok()?,
		))
	}

	async fn offset(&self, reference: SpatialRef, offset_transform: PartialTransform) {
		let Some((reference_in_parent, parent_in_reference, local)) = self.frames(reference).await
		else {
			return;
		};
		// unset components of an offset mean "don't move that way"
		let offset = fill(offset_transform, Transform::IDENTITY);

		let mut pending = self.pending_transform_change.lock().unwrap();
		let current = pending.unwrap_or(local);
		*pending = Some(reference_in_parent * (offset * (parent_in_reference * current)));
	}

	async fn set(&self, reference: SpatialRef, transform: PartialTransform) {
		let Some((reference_in_parent, parent_in_reference, local)) = self.frames(reference).await
		else {
			return;
		};

		let mut pending = self.pending_transform_change.lock().unwrap();
		let current = pending.unwrap_or(local);
		*pending = Some(reference_in_parent * fill(transform, parent_in_reference * current));
	}
}

#[derive(Handler)]
struct TransformableNode(Arc<TransformableCore>);
impl TransformableHandler for TransformableNode {
	async fn offset_relative_transform(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		offset_transform: PartialTransform,
	) {
		self.0.offset(reference, offset_transform).await
	}

	async fn set_relative_transform(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		transform: PartialTransform,
	) {
		self.0.set(reference, transform).await
	}
}

#[derive(Handler)]
struct PoseableNode(Arc<TransformableCore>);
impl PoseableHandler for PoseableNode {
	async fn offset_relative_pse(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		offset: Posef,
	) {
		self.0.offset(reference, pose(offset)).await
	}

	async fn set_relative_pose(&self, _ctx: gluon::Context, reference: SpatialRef, pose: Posef) {
		self.0.set(reference, self::pose(pose)).await
	}
}

#[derive(Handler)]
struct TranslatableNode(Arc<TransformableCore>);
impl TranslatableHandler for TranslatableNode {
	async fn offset_relative_translation(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		offset: Vec3F,
	) {
		self.0.offset(reference, translation(offset)).await
	}

	async fn set_relative_translation(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		translation: Vec3F,
	) {
		self.0.set(reference, self::translation(translation)).await
	}
}

#[derive(Handler)]
struct RotatableNode(Arc<TransformableCore>);
impl RotatableHandler for RotatableNode {
	async fn offset_relative_rotation(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		offset: QuatF,
	) {
		self.0.offset(reference, rotation(offset)).await
	}

	async fn set_relative_rotation(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		rotation: QuatF,
	) {
		self.0.set(reference, self::rotation(rotation)).await
	}
}

#[derive(Handler)]
struct ScalableNode(Arc<TransformableCore>);
impl ScalableHandler for ScalableNode {
	async fn offset_relative_scale(
		&self,
		_ctx: gluon::Context,
		reference: SpatialRef,
		offset: Vec3F,
	) {
		self.0.offset(reference, scale(offset)).await
	}

	async fn set_relative_scale(&self, _ctx: gluon::Context, reference: SpatialRef, scale: Vec3F) {
		self.0.set(reference, self::scale(scale)).await
	}
}

fn translation(translation: Vec3F) -> PartialTransform {
	PartialTransform {
		translation: Some(translation),
		rotation: None,
		scale: None,
	}
}
fn rotation(rotation: QuatF) -> PartialTransform {
	PartialTransform {
		translation: None,
		rotation: Some(rotation),
		scale: None,
	}
}
fn scale(scale: Vec3F) -> PartialTransform {
	PartialTransform {
		translation: None,
		rotation: None,
		scale: Some(scale),
	}
}
fn pose(pose: Posef) -> PartialTransform {
	PartialTransform {
		translation: Some(pose.position),
		rotation: Some(pose.orientation),
		scale: None,
	}
}

fn fill(partial: PartialTransform, base: Transform) -> Transform {
	Transform {
		translation: partial.translation.unwrap_or(base.translation),
		rotation: partial.rotation.unwrap_or(base.rotation),
		scale: partial.scale.unwrap_or(base.scale),
	}
}
