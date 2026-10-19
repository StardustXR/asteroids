use crate::{Context, ValidState, custom::CustomElement};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::spatial::SpatialRef;
use std::{any::Any, fmt::Debug};
use tokio::{sync::watch, task::JoinHandle};

#[allow(type_alias_bounds)]
pub(crate) struct CreatingInner<State: ValidState, E: CustomElement<State>> {
	#[allow(clippy::type_complexity)]
	pub(crate) handle: JoinHandle<(E, Result<E::Inner, E::Error>, SpatialRef)>,
	pub(crate) pending_spatial: watch::Receiver<Option<SpatialRef>>,
}

pub(crate) enum ElementInner<State: ValidState, E: CustomElement<State>> {
	Creating(Option<CreatingInner<State, E>>),
	Done(E::Inner, SpatialRef),
	Error(E::Error),
}

/// keeps removed inners alive until they finish leaving
pub(crate) trait Vanishing: Send + Sync {
	/// false once it's done
	fn tick(&mut self, context: &Context) -> bool;
}
impl Debug for dyn Vanishing {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.write_str("Vanishing")
	}
}

#[derive(Debug, Default)]
pub(crate) struct ElementInnerMap {
	inners: FxHashMap<u64, Box<dyn Any + Send + Sync>>,
	captured: Option<Vec<Box<dyn Any + Send + Sync>>>,
	vanishing: Vec<Box<dyn Vanishing>>,
	// while the client closes everything else stays put so its vanishes have something to sit in
	leaving: bool,
}
impl ElementInnerMap {
	pub fn insert<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
		inner_future: CreatingInner<State, E>,
	) {
		self.inners
			.insert(key, Box::new(ElementInner::Creating(Some(inner_future))));
	}
	pub fn get<State: ValidState, E: CustomElement<State>>(
		&self,
		key: u64,
	) -> Option<&ElementInner<State, E>> {
		self.inners.get(&key)?.downcast_ref()
	}
	pub fn get_mut<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
	) -> Option<&mut ElementInner<State, E>> {
		self.inners.get_mut(&key)?.downcast_mut()
	}
	pub fn remove(&mut self, key: u64) {
		if self.leaving && self.captured.is_none() {
			return;
		}
		if let Some(inner) = self.inners.remove(&key)
			&& let Some(captured) = &mut self.captured
		{
			captured.push(inner);
		}
	}
	pub fn take<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
	) -> Option<ElementInner<State, E>> {
		match self.inners.remove(&key)?.downcast() {
			Ok(inner) => Some(*inner),
			Err(inner) => {
				self.inners.insert(key, inner);
				None
			}
		}
	}

	/// everything `f` removes is handed back instead of dropped
	pub fn capture(&mut self, f: impl FnOnce(&mut Self)) -> Vec<Box<dyn Any + Send + Sync>> {
		self.captured = Some(Vec::new());
		f(self);
		self.captured.take().unwrap_or_default()
	}
	pub fn capturing(&self) -> bool {
		self.captured.is_some()
	}
	pub fn vanish(&mut self, vanishing: impl Vanishing + 'static) {
		self.vanishing.push(Box::new(vanishing));
	}
	pub fn tick(&mut self, context: &Context) {
		self.vanishing.retain_mut(|v| v.tick(context));
	}
	pub fn leave(&mut self, f: impl FnOnce(&mut Self)) {
		self.leaving = true;
		f(self);
	}
	pub fn vanishing(&self) -> bool {
		!self.vanishing.is_empty()
	}
}
