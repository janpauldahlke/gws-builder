//! Intermediate representation: Discovery → IR → codegen.

pub mod filter;
pub mod resolve;
pub mod transform;
pub mod types;

pub use filter::apply_filter;
pub use resolve::resolve_service;
pub use transform::discovery_to_ir;
pub use types::*;
