use crate::{
	CustomElement, ValidState,
	inner::ElementInnerKey,
	mapped::Mapped,
	tree::{ElementDiffer, FlatElement},
};
use bumpalo::{Bump, boxed::Box};
use std::{
	hash::{DefaultHasher, Hash, Hasher},
	marker::PhantomData,
};

#[allow(private_bounds)]
pub trait Element<State: ValidState>: ElementFlattener<State> {
	fn map<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut State> + Send + Sync + 'static,
	>(
		self,
		mapper: F,
	) -> Mapped<SuperState, State, F, Self> {
		Mapped::new(self, mapper)
	}
}
pub(crate) trait ElementFlattener<State: ValidState>: Sized + Send + Sync + 'static {
	// return a vector of this element and all its known siblings
	fn flatten(self, bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>>;
}

macro_rules! tuple_impls {
	() => {
		impl<State: ValidState> ElementFlattener<State> for () {
			fn flatten(self, _bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>> {
				vec![]
			}
		}
		impl<State: ValidState> Element<State> for () {}
	};
	($($t:ident)*) => {
		impl<State: ValidState, $($t: Element<State>),*> ElementFlattener<State> for ($($t),*) {
			#[allow(non_snake_case)]
			fn flatten(self, bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>> {
				let ($($t),*) = self;
				let mut result = Vec::new();
				$(
					result.extend($t.flatten(bump));
				)*
				result
			}
		}
		impl<State: ValidState, $($t: Element<State>),*> Element<State> for ($($t),*) {}
	};
}
tuple_impls!();
tuple_impls!(A B);
tuple_impls!(A B C);
tuple_impls!(A B C D);
tuple_impls!(A B C D E);
tuple_impls!(A B C D E F);
tuple_impls!(A B C D E F G);
tuple_impls!(A B C D E F G H);
tuple_impls!(A B C D E F G H I);
tuple_impls!(A B C D E F G H I J);
tuple_impls!(A B C D E F G H I J K);
tuple_impls!(A B C D E F G H I J K L);

impl<State: ValidState, E: Element<State>> ElementFlattener<State> for Vec<E> {
	fn flatten(self, bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>> {
		self.into_iter().flat_map(|e| e.flatten(bump)).collect()
	}
}
impl<State: ValidState, E: Element<State>> Element<State> for Vec<E> {}
impl<State: ValidState, E: Element<State>> ElementFlattener<State> for Option<E> {
	fn flatten(self, bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>> {
		self.into_iter().flat_map(|e| e.flatten(bump)).collect()
	}
}
impl<State: ValidState, E: Element<State>> Element<State> for Option<E> {}

pub struct ElementWrapper<State: ValidState, E: CustomElement<State>, C: Element<State>> {
	pub custom_element: E,
	children: C,
	pub id: Option<ElementInnerKey>,
	state_phantom: PhantomData<State>,
}

impl<State: ValidState, E: CustomElement<State>, C: Element<State>> ElementWrapper<State, E, C> {
	pub(crate) fn new(custom_element: E) -> ElementWrapper<State, E, ()> {
		ElementWrapper {
			custom_element,
			children: (),
			id: None,
			state_phantom: PhantomData,
		}
	}
	pub fn child<NC: Element<State>>(self, child: NC) -> ElementWrapper<State, E, (C, NC)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, child),
			id: self.id,
			state_phantom: PhantomData,
		}
	}
	pub fn children<NC: Element<State>>(
		self,
		children: impl IntoIterator<Item = NC>,
	) -> ElementWrapper<State, E, (C, Vec<NC>)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, children.into_iter().collect()),
			id: self.id,
			state_phantom: PhantomData,
		}
	}
	pub fn maybe_child<NC: Element<State>>(
		self,
		child: Option<NC>,
	) -> ElementWrapper<State, E, (C, Option<NC>)> {
		ElementWrapper {
			custom_element: self.custom_element,
			children: (self.children, child),
			id: self.id,
			state_phantom: PhantomData,
		}
	}
	pub fn identify<H: Hash>(mut self, h: &H) -> Self {
		let mut hasher = DefaultHasher::new();
		h.hash(&mut hasher);
		let key = ElementInnerKey(hasher.finish());
		self.id.replace(key);
		self
	}
}
impl<State: ValidState, E: CustomElement<State>, C: Element<State>> ElementFlattener<State>
	for ElementWrapper<State, E, C>
{
	fn flatten(self, bump: &Bump) -> Vec<Box<dyn ElementDiffer<State>>> {
		let children =
			bumpalo::collections::vec::Vec::from_iter_in(self.children.flatten(bump), bump);
		let flat_element = Box::new_in(
			FlatElement {
				element: self.custom_element,
				children,
				id: self.id,
				phantom: PhantomData,
			},
			bump,
		);
		// rust doesn't let third party crates like bumpalo do proper trait object coercion so this is manual
		let flat_element = unsafe {
			let raw = Box::into_raw(flat_element);
			let trait_obj: *mut dyn ElementDiffer<State> = raw as *mut dyn ElementDiffer<State>;
			Box::from_raw(trait_obj)
		};
		vec![flat_element]
	}
}

impl<State: ValidState, E: CustomElement<State>, C: Element<State>> Element<State>
	for ElementWrapper<State, E, C>
{
}
