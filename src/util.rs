use serde::de::DeserializeOwned;
use std::any::{Any, TypeId};

pub trait DeserializeAny<'de, E> {
	fn deserialize<T: DeserializeOwned>(&self) -> Result<T, E>;
}
pub trait Migrate: Any + Sized + DeserializeOwned {
	/// If this type is set to `Self` then it'll be treated as the earliest version
	type Old: Migrate + Into<Self>;

	fn chain_end() -> bool {
		TypeId::of::<Self>() == TypeId::of::<Self::Old>()
	}

	fn deserialize_with_migrate<'de, E, D: DeserializeAny<'de, E>>(
		deserializer: &D,
	) -> Result<Self, E> {
		match deserializer.deserialize::<Self>() {
			Ok(current) => Ok(current),
			Err(e) => {
				if Self::chain_end() {
					Err(e)
				} else {
					let old = Self::Old::deserialize_with_migrate(deserializer)?;
					Ok(old.into())
				}
			}
		}
	}
}

pub(crate) struct RonFile(pub String);
impl DeserializeAny<'_, ron::Error> for RonFile {
	fn deserialize<T: DeserializeOwned>(&self) -> Result<T, ron::Error> {
		Ok(ron::from_str(&self.0)?)
	}
}
