use std::{collections::HashMap, path::PathBuf};

use chumsky::{
    error::Simple,
    prelude::*,
    text::{newline, whitespace},
};

fn parser() -> impl Parser<char, AbstractSyntaxTree, Error = Simple<char>> {
    // let whitespace_no_newline = filter(|c: &char| c.is_inline_whitespace())
    //     .ignored()
    //     .repeated();
    // let newline_separator = whitespace_no_newline
    //     .then(newline())
    //     .ignore_then(whitespace_no_newline);

    let newline_separator = whitespace();

    let import_path = filter(|c: &char| c.is_ascii_alphanumeric() || *c == '/' || *c == ' ')
        .repeated()
        .map(|s| PathBuf::from(s.into_iter().collect::<String>()));
    let import = just("import ")
        .ignore_then(import_path)
        .then_ignore(newline().then(whitespace()))
        .labelled("import");
    let imports = import.repeated().labelled("imports");

    let ast_struct = recursive(|ast_struct| {
        let r#type = filter(|c: &char| c.is_ascii_alphabetic() && c.is_ascii_uppercase())
            .chain(filter(|c: &char| c.is_ascii_alphanumeric() || *c == '_').repeated())
            .collect()
            .labelled("struct type");

        let digits = text::int(10).labelled("digits");
        let int_property = digits
            .map(|s: String| s.parse().unwrap())
            .labelled("integer");
        let float_property = text::int(10)
            .then_ignore(just("."))
            .then(text::int(10))
            .map(|(a, b): (String, String)| a + "." + &b)
            .map(|s: String| s.parse().unwrap())
            .labelled("float");
        let vector_property = float_property
            .padded()
            .separated_by(just(","))
            .delimited_by(just("<"), just(">"))
            .labelled("vector");
        let string_property = just::<char, char, Simple<char>>('"')
            .ignore_then(take_until(just('"')))
            .map(|(a, _)| a)
            .map(|chars: Vec<char>| chars.into_iter().collect::<String>())
            .labelled("string");
        let struct_property = ast_struct.clone().labelled("struct (property)");
        let other_property = take_until(whitespace())
            .map(|(a, _)| a)
            .map(|chars: Vec<char>| chars.into_iter().collect::<String>())
            .labelled("other");

        let property_value = choice((
            vector_property.map(AstPropertyValue::Vector),
            float_property.map(AstPropertyValue::Float),
            int_property.map(AstPropertyValue::Int),
            struct_property.map(AstPropertyValue::Struct),
            string_property.map(AstPropertyValue::String),
            other_property.map(AstPropertyValue::Other),
        ))
        .labelled("property value");
        let id = newline_separator
            .ignore_then(just("id"))
            .then_ignore(just(':').padded())
            .ignore_then(text::ident())
            .then_ignore(whitespace())
            .or_not()
            .labelled("id");
        let property = text::ident()
            .then_ignore(just(':').padded())
            .then(property_value)
            .labelled("property");
        let properties = property
            .then_ignore(whitespace())
            .repeated()
            .collect::<HashMap<_, _>>()
            .labelled("properties");

        let children = ast_struct
            .then_ignore(whitespace())
            .repeated()
            .labelled("children");

        r#type
            .then_ignore(whitespace())
            .then_ignore(just('{'))
            .then(id)
            .then(properties)
            .then(children)
            .then_ignore(whitespace())
            .then_ignore(just('}'))
            .map(|(((r#type, id), properties), children)| AstStruct {
                r#type,
                id,
                properties,
                children,
            })
            .labelled("struct")
    });

    imports
        .then(ast_struct)
        .then_ignore(whitespace())
        .map(|(imports, root_struct)| AbstractSyntaxTree {
            imports,
            root_struct,
        })
        .then_ignore(end())
}

#[derive(Debug)]
pub enum AstPropertyValue {
    Int(i32),
    Float(f32),
    String(String),
    Struct(AstStruct),
    Vector(Vec<f32>),
    Other(String),
}

#[derive(Debug)]
pub struct AstStruct {
    pub(crate) r#type: String,
    pub(crate) id: Option<String>,
    pub(crate) properties: HashMap<String, AstPropertyValue>,
    pub(crate) children: Vec<AstStruct>,
}

#[derive(Debug)]
pub struct AbstractSyntaxTree {
    pub(crate) imports: Vec<PathBuf>,
    pub(crate) root_struct: AstStruct,
}
impl AbstractSyntaxTree {
    pub fn parse(src: &str) -> Result<Self, Vec<Simple<char>>> {
        parser().parse(src)
    }
}
