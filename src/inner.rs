use crate::{ValidState, custom::CustomElement};
use rustc_hash::FxHashMap;
use stardust_xr_fusion::spatial::SpatialRef;
use std::any::Any;
use tokio::task::JoinHandle;

#[allow(type_alias_bounds)]
pub(crate) type CreatingInner<State: ValidState, E: CustomElement<State>> =
	JoinHandle<(E, Result<E::Inner, E::Error>, SpatialRef)>;

pub(crate) enum ElementInner<State: ValidState, E: CustomElement<State>> {
	Creating(Option<CreatingInner<State, E>>),
	Done(E::Inner, SpatialRef),
	Error(E::Error),
}

#[derive(Debug, Default)]
pub(crate) struct ElementInnerMap(FxHashMap<u64, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
	pub fn insert<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
		inner_future: CreatingInner<State, E>,
	) {
		self.0
			.insert(key, Box::new(ElementInner::Creating(Some(inner_future))));
	}
	pub fn get<State: ValidState, E: CustomElement<State>>(
		&self,
		key: u64,
	) -> Option<&ElementInner<State, E>> {
		self.0.get(&key)?.downcast_ref()
	}
	pub fn get_mut<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
	) -> Option<&mut ElementInner<State, E>> {
		self.0.get_mut(&key)?.downcast_mut()
	}
	pub fn remove(&mut self, key: u64) {
		self.0.remove(&key);
	}
}
