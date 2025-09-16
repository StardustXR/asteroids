use crate::{ValidState, custom::CustomElement};
use rustc_hash::FxHashMap;
use std::any::Any;

#[derive(Debug, Default)]
pub struct ElementInnerMap(FxHashMap<u64, Box<dyn Any + Send + Sync>>);
impl ElementInnerMap {
	pub fn insert<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
		inner: E::Inner,
	) {
		self.0.insert(key, Box::new(inner));
	}
	pub fn get<State: ValidState, E: CustomElement<State>>(&self, key: u64) -> Option<&E::Inner> {
		self.0.get(&key)?.downcast_ref()
	}
	pub fn get_mut<State: ValidState, E: CustomElement<State>>(
		&mut self,
		key: u64,
	) -> Option<&mut E::Inner> {
		self.0.get_mut(&key)?.downcast_mut()
	}
	pub fn remove(&mut self, key: u64) {
		self.0.remove(&key);
	}
}
