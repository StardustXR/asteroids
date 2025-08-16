use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

#[derive(Default)]
pub(crate) struct ResourceRegistry(FxHashMap<TypeId, Box<dyn Any>>);
impl ResourceRegistry {
	pub fn get<R: Default + Send + Sync + 'static>(&mut self) -> &mut R {
		let type_id = TypeId::of::<R>();
		self.0
			.entry(type_id)
			.or_insert_with(|| Box::new(R::default()))
			.downcast_mut::<R>()
			.unwrap()
	}
}
