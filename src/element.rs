use crate::{ValidState, inner::ElementInnerKey, scenegraph::GenericElement};
use std::{
	collections::hash_map::DefaultHasher,
	hash::{Hash, Hasher},
};

#[derive_where::derive_where(Debug)]
pub struct Element<State: ValidState>(pub(crate) Box<dyn GenericElement<State>>);
impl<State: ValidState> Element<State> {
	pub fn child(mut self, child: Element<State>) -> Self {
		self.0.add_child(child);
		self
	}
	pub fn children(mut self, children: impl IntoIterator<Item = Element<State>>) -> Self {
		for child in children {
			self.0.add_child(child);
		}
		self
	}
	pub fn maybe_child(mut self, child: Option<Element<State>>) -> Self {
		if let Some(child) = child {
			self.0.add_child(child);
		}
		self
	}
	pub fn map<
		NewState: ValidState,
		F: Fn(&mut NewState) -> Option<&mut State> + Send + Sync + 'static,
	>(
		self,
		mapper: F,
	) -> Element<NewState> {
		Element(Box::new(crate::MappedElement::new(self, mapper)))
	}
	pub fn identify<H: Hash>(mut self, h: &H) -> Self {
		let mut hasher = DefaultHasher::new();
		h.hash(&mut hasher);
		let key = ElementInnerKey(hasher.finish());
		self.0.identify(key);
		self
	}
}

impl<State: ValidState> Hash for Element<State> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		self.0.inner_key().hash(state);
	}
}
impl<State: ValidState> PartialEq for Element<State> {
	fn eq(&self, other: &Self) -> bool {
		self.0.inner_key() == other.0.inner_key()
	}
}
impl<State: ValidState> Eq for Element<State> {}
