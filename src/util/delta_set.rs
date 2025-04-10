use rustc_hash::FxHashSet;
use std::{fmt::Debug, hash::Hash};

pub struct DeltaSet<T: Clone + Hash + Eq> {
	added: FxHashSet<T>,
	current: FxHashSet<T>,
	removed: FxHashSet<T>,
}
impl<T: Clone + Hash + Eq> Default for DeltaSet<T> {
	fn default() -> Self {
		DeltaSet {
			added: Default::default(),
			current: Default::default(),
			removed: Default::default(),
		}
	}
}
impl<T: Clone + Hash + Eq + Debug> Debug for DeltaSet<T> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("DeltaSet")
			.field("added", &self.added)
			.field("current", &self.current)
			.field("removed", &self.removed)
			.finish()
	}
}
impl<T: Clone + Hash + Eq> DeltaSet<T> {
	pub fn push_new(&mut self, new: impl Iterator<Item = T>) {
		let new = FxHashSet::from_iter(new);
		self.added = FxHashSet::from_iter(new.difference(&self.current).cloned());
		self.removed = FxHashSet::from_iter(self.current.difference(&new).cloned());
		self.current = new;
	}
	pub fn added(&self) -> &FxHashSet<T> {
		&self.added
	}
	pub fn current(&self) -> &FxHashSet<T> {
		&self.current
	}
	pub fn removed(&self) -> &FxHashSet<T> {
		&self.removed
	}
}
