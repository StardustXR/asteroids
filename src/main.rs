pub mod asteroids;
mod syntax;
use crate::syntax::AbstractSyntaxTree;

fn main() {
    let src = std::fs::read_to_string(std::env::args().nth(1).unwrap()).unwrap();

    println!("{:?}", AbstractSyntaxTree::parse(&src));
}
