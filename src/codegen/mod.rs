//! TokenStream codegen (structs, enums, methods, action descriptors).

pub mod actions;
pub mod enums;
pub mod methods;
pub mod naming;
pub mod structs;

pub use actions::{emit_action_descriptor_types, emit_method_action};
pub use enums::emit_enum;
pub use methods::emit_method_param_structs;
pub use structs::emit_struct;
