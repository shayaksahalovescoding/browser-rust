//! Custom JavaScript lexer, recursive-descent/Pratt parser, runtime, and DOM bindings.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod runtime;

pub use runtime::{JsEngine, JsValue};
