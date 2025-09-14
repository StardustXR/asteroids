use bumpalo::{Bump, boxed::Box};

use crate::{
	Element, Identifiable, ValidState,
	element::ElementFlattener,
	tree::{ElementDiffer, FlatmapElement},
};
use std::marker::PhantomData;

pub struct Mapped<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> {
	wrapped: E,
	mapper: Option<F>,
	phantom: PhantomData<State>,
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Mapped<State, WrappedState, F, E>
{
	pub fn new(wrapped: E, mapper: F) -> Self {
		Self {
			wrapped,
			mapper: Some(mapper),
			phantom: PhantomData,
		}
	}
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> ElementFlattener<State> for Mapped<State, WrappedState, F, E>
{
	fn flatten<'a>(&mut self, bump: &'a Bump) -> Vec<Box<'a, dyn ElementDiffer<State>>> {
		let wrapped = self.wrapped.flatten(bump).into_iter().next().unwrap();
		let flatmap_element = Box::new_in(
			FlatmapElement {
				wrapped,
				mapper: self.mapper.take().unwrap(),
				phantom: PhantomData,
			},
			bump,
		);

		// rust doesn't let third party crates like bumpalo do proper trait object coercion so this is manual
		let flatmap_element = unsafe {
			let raw = Box::into_raw(flatmap_element);
			let trait_obj: *mut dyn ElementDiffer<State> = raw as *mut dyn ElementDiffer<State>;
			Box::from_raw(trait_obj)
		};
		vec![flatmap_element]
	}
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Element<State> for Mapped<State, WrappedState, F, E>
{
}

impl<
	State: ValidState,
	WrappedState: ValidState,
	F: Fn(&mut State) -> Option<&mut WrappedState> + Send + Sync + 'static,
	E: Element<WrappedState>,
> Identifiable for Mapped<State, WrappedState, F, E>
where
	E: Identifiable,
{
	fn identify<H: std::hash::Hash>(self, h: &H) -> Self {
		Mapped {
			wrapped: self.wrapped.identify(h),
			mapper: self.mapper,
			phantom: PhantomData,
		}
	}
}
