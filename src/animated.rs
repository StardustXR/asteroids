#![allow(private_bounds)]

use crate::{
	__private::AnimatedInner,
	Animatable, Context, CustomElement, Element, ValidState,
	element::{ElementDiffer, ElementWrapper},
	inner::{ElementInner, ElementInnerMap},
};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::{client::FrameInfo, spatial::SpatialRef, types::Timestamp};
use std::{fmt::Debug, hash::Hash, path::Path};
use tokio::sync::watch;

/// an element with `.animation(..)` or `.appear(..)` on it
pub struct Animated<
	State: ValidState,
	E: Animatable + CustomElement<State>,
	C: ElementDiffer<State>,
> {
	wrapper: ElementWrapper<State, E, C>,
	animation: E::Animation,
	appear: Option<E::Transition>,
	creating: bool,
}
impl<State: ValidState, E, C: ElementDiffer<State>, I> ElementWrapper<State, E, C>
where
	E: Animatable + CustomElement<State, Inner = AnimatedInner<I, E::Tracks>>,
{
	pub fn animation(self, f: impl FnOnce(E::Animation) -> E::Animation) -> Animated<State, E, C> {
		Animated::new(self).animation(f)
	}
	pub fn appear(self, f: impl FnOnce(E::Transition) -> E::Transition) -> Animated<State, E, C> {
		Animated::new(self).appear(f)
	}
}
impl<State: ValidState, E, C: ElementDiffer<State>, I> Animated<State, E, C>
where
	E: Animatable + CustomElement<State, Inner = AnimatedInner<I, E::Tracks>>,
{
	fn new(wrapper: ElementWrapper<State, E, C>) -> Self {
		Animated {
			wrapper,
			animation: Default::default(),
			appear: None,
			creating: false,
		}
	}
	fn with<NC: ElementDiffer<State>>(
		self,
		f: impl FnOnce(ElementWrapper<State, E, C>) -> ElementWrapper<State, E, NC>,
	) -> Animated<State, E, NC> {
		Animated {
			wrapper: f(self.wrapper),
			animation: self.animation,
			appear: self.appear,
			creating: self.creating,
		}
	}

	pub fn animation(mut self, f: impl FnOnce(E::Animation) -> E::Animation) -> Self {
		self.animation = f(self.animation);
		self
	}
	pub fn appear(mut self, f: impl FnOnce(E::Transition) -> E::Transition) -> Self {
		self.appear = Some(f(self.appear.unwrap_or_default()));
		self
	}

	pub fn child<NC: Element<State>>(self, child: NC) -> Animated<State, E, (C, NC)> {
		self.with(|w| w.child(child))
	}
	pub fn maybe_child<NC: Element<State>>(
		self,
		child: Option<NC>,
	) -> Animated<State, E, (C, Option<NC>)> {
		self.with(|w| w.maybe_child(child))
	}
	pub fn children<NC: Element<State>>(
		self,
		children: impl IntoIterator<Item = NC>,
	) -> Animated<State, E, (C, Vec<NC>)> {
		self.with(|w| w.children(children))
	}
	pub fn stable_children<NC: Element<State>, K: Eq + Hash + Clone + Send + Sync + 'static>(
		self,
		children: impl IntoIterator<Item = (K, NC)>,
	) -> Animated<State, E, (C, FxHashMap<K, NC>)> {
		self.with(|w| w.stable_children(children))
	}

	fn hold_at_start(&mut self) {
		self.creating = true;
		if let Some(e) = &mut self.wrapper.custom_element
			&& let Some(t) = &mut self.appear
		{
			e.appear_from(t);
		}
	}
	fn arrive(&mut self, tracks: &mut E::Tracks, now: Option<Timestamp>) {
		if let Some(e) = &mut self.wrapper.custom_element
			&& let Some(t) = &mut self.appear
		{
			e.arrive(t, &self.animation, tracks, now);
		}
	}
}
impl<State: ValidState, E, C: ElementDiffer<State>, I> ElementDiffer<State>
	for Animated<State, E, C>
where
	E: Animatable + CustomElement<State, Inner = AnimatedInner<I, E::Tracks>>,
{
	fn create_inner_recursive(
		&mut self,
		inner_key: u64,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		self.hold_at_start();
		self.wrapper.create_inner_recursive(
			inner_key,
			context,
			parent_space,
			element_path,
			inner_map,
		);
	}

	fn frame_recursive(
		&self,
		context: &Context,
		info: &FrameInfo,
		state: &mut State,
		inner_map: &mut ElementInnerMap,
	) {
		self.wrapper
			.frame_recursive(context, info, state, inner_map);
	}

	fn diff_same_type(
		&mut self,
		inner_key: u64,
		old: &Self,
		context: &Context,
		parent_space: watch::Receiver<Option<SpatialRef>>,
		element_path: &Path,
		inner_map: &mut ElementInnerMap,
	) {
		let now = context.predicted_display_time();
		// the wrapper diffs straight from the creation struct when creation finishes, so the
		// arrival is blended before that and its tracks wait here until the inner exists
		let mut arrival = None;
		match inner_map.get_mut::<State, E>(inner_key) {
			Some(ElementInner::Creating(Some(c)))
				if c.handle.is_finished() && old.wrapper.custom_element.is_some() =>
			{
				let mut tracks = E::Tracks::default();
				self.arrive(&mut tracks, now);
				arrival = Some(tracks);
			}
			Some(ElementInner::Creating(_)) => self.hold_at_start(),
			// last update, creation finished between this check and the wrapper's, so arrive a step late
			Some(ElementInner::Done(inner, _)) if old.creating && self.appear.is_some() => {
				self.arrive(&mut inner.tracks, now)
			}
			Some(ElementInner::Done(inner, _)) => {
				if let Some(e) = &mut self.wrapper.custom_element
					&& let Some(old_e) = &old.wrapper.custom_element
				{
					e.blend(old_e, &self.animation, &mut inner.tracks, now);
				}
			}
			_ => (),
		}
		self.wrapper.diff_same_type(
			inner_key,
			&old.wrapper,
			context,
			parent_space,
			element_path,
			inner_map,
		);
		if let Some(tracks) = arrival
			&& let Some(ElementInner::Done(inner, _)) = inner_map.get_mut::<State, E>(inner_key)
		{
			inner.tracks = tracks;
		}
	}

	fn destroy_inner_recursive(&self, inner_map: &mut ElementInnerMap) {
		self.wrapper.destroy_inner_recursive(inner_map);
	}
}
impl<State: ValidState, E, C: ElementDiffer<State>, I> Element<State> for Animated<State, E, C> where
	E: Animatable + CustomElement<State, Inner = AnimatedInner<I, E::Tracks>>
{
}
impl<State: ValidState, E: Animatable + CustomElement<State>, C: ElementDiffer<State>> Debug
	for Animated<State, E, C>
where
	ElementWrapper<State, E, C>: Debug,
{
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_tuple("Animated").field(&self.wrapper).finish()
	}
}
