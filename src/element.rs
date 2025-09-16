#![allow(private_bounds)]

use crate::{
	CustomElement, ValidState,
	mapped::Mapped,
	tree::{ElementDiffer, FlatElement},
};
use bumpalo::{Bump, boxed::Box};
use std::{
	hash::{DefaultHasher, Hash, Hasher},
	marker::PhantomData,
};

pub trait Element<State: ValidState>: ElementFlattener<State> + Sized {
	fn map<
		SuperState: ValidState,
		F: Fn(&mut SuperState) -> Option<&mut State> + Send + Sync + 'static,
	>(
		self,
		mapper: F,
	) -> Mapped<SuperState, State, F, Self> {
		Mapped::new(self, mapper)
	}
	fn heap(self) -> HeapElement<State> {
		HeapElement(std::boxed::Box::new(self))
	}
}
pub(crate) trait ElementFlattener<State: ValidState>: Send + Sync + 'static {
	// return a vector of this element and all its known siblings
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>>;
}
pub trait Identifiable {
	fn identify<H: Hash>(self, h: &H) -> Self;
}

pub struct HeapElement<State: ValidState>(std::boxed::Box<dyn ElementFlattener<State>>);
impl<State: ValidState> ElementFlattener<State> for HeapElement<State> {
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		self.0.flatten(bump)
	}
}
impl<State: ValidState> Element<State> for HeapElement<State> {}

macro_rules! tuple_impls {
	() => {
		impl<State: ValidState> ElementFlattener<State> for () {
			fn flatten<'a>(&mut self, _bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
				vec![]
			}
		}
	};
	($($t:ident)*) => {
		impl<State: ValidState, $($t: ElementFlattener<State>),*> ElementFlattener<State> for ($($t),*) {
			#[allow(non_snake_case)]
			fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
				let ($($t),*) = self;
				let mut result = Vec::new();
				$(
					result.extend($t.flatten(bump));
				)*
				result
			}
		}
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

impl<State: ValidState> ElementFlattener<State> for std::boxed::Box<dyn ElementFlattener<State>> {
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		self.as_mut().flatten(bump)
	}
}

impl<State: ValidState, E: Element<State>> ElementFlattener<State> for Vec<E> {
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		self.drain(..).flat_map(|mut e| e.flatten(bump)).collect()
	}
}
impl<State: ValidState, E: Element<State>> ElementFlattener<State> for Option<E> {
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		self.take()
			.into_iter()
			.flat_map(|mut e| e.flatten(bump))
			.collect()
	}
}

pub struct ElementWrapper<State: ValidState, E: CustomElement<State>, C: ElementFlattener<State>> {
	pub custom_element: Option<E>,
	children: C,
	pub id: Option<u64>,
	state_phantom: PhantomData<State>,
}

impl<State: ValidState, E: CustomElement<State>, C: ElementFlattener<State>>
	ElementWrapper<State, E, C>
{
	pub(crate) fn new(custom_element: E) -> ElementWrapper<State, E, ()> {
		ElementWrapper {
			custom_element: Some(custom_element),
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
}
impl<State: ValidState, E: CustomElement<State>, C: ElementFlattener<State>> ElementFlattener<State>
	for ElementWrapper<State, E, C>
{
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		let children =
			bumpalo::collections::vec::Vec::from_iter_in(self.children.flatten(bump), bump);
		let flat_element = Box::new_in(
			FlatElement {
				element: self.custom_element.take().unwrap(),
				children,
				id: self.id.map(|id| id.into()).unwrap_or_default(),
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

impl<State: ValidState, E: CustomElement<State>, C: ElementFlattener<State>> Element<State>
	for ElementWrapper<State, E, C>
{
}

impl<State: ValidState, E: CustomElement<State>, C: ElementFlattener<State>> Identifiable
	for ElementWrapper<State, E, C>
{
	fn identify<H: Hash>(mut self, h: &H) -> Self {
		let mut hasher = DefaultHasher::new();
		h.hash(&mut hasher);
		let key = hasher.finish();
		self.id.replace(key);
		self
	}
}
