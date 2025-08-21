use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

use crate::{CustomElement, ValidState};

#[derive(Default)]
pub(crate) struct ResourceRegistry(FxHashMap<TypeId, Box<dyn Any>>);
impl ResourceRegistry {
	pub fn get<State: ValidState, E: CustomElement<State>>(&mut self) -> &mut E::Resource {
		let type_id = TypeId::of::<E::Resource>();
		self.0
			.entry(type_id)
			.or_insert_with(|| Box::new(E::Resource::default()))
			.downcast_mut::<E::Resource>()
			.unwrap()
	}
}
