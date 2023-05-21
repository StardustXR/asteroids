use crate::syntax::{AbstractSyntaxTree, AstPropertyValue};
use rustc_hash::FxHashMap;

pub trait Asteroid: Sized {
    fn from_properties(properties: &FxHashMap<String, AstPropertyValue>) -> Result<Self, String>;
}

pub struct AsteroidField {
    ast: AbstractSyntaxTree,
    nodes: Vec<Box<dyn Asteroid>>,
}
impl AsteroidField {
    pub fn create(
        ast: AbstractSyntaxTree,
        node_init_fn: fn(&FxHashMap<String, AstPropertyValue>) -> Result<dyn Asteroid, String>,
    ) -> Result<Self, String> {
    }
}
