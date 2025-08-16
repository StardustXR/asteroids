use crate::{ValidState, custom::CustomElement};
use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};
use std::any::Any;

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, Serialize, Deserialize)]
pub struct ElementInnerKey(pub u64);

#[derive(Debug, Default)]
pub struct ElementInnerMap(FxHashMap<ElementInnerKey, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
	pub fn insert<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: ElementInnerKey,
		inner: E::Inner,
	) {
		self.0.insert(key, Box::new(inner));
	}
	pub fn get<State: ValidState, E: CustomElement<State>>(
		&self,
		key: ElementInnerKey,
	) -> Option<&E::Inner> {
		self.0.get(&key)?.downcast_ref()
	}
	pub fn get_mut<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: ElementInnerKey,
	) -> Option<&mut E::Inner> {
		self.0.get_mut(&key)?.downcast_mut()
	}
	pub fn remove(&mut self, key: &ElementInnerKey) {
		self.0.remove(key);
	}
}
