mod numeric_parameter;
// pub mod glob;
// pub mod path;
// pub mod path_list;
// pub mod text;
pub mod parameter;

// Re-export
pub use numeric_parameter::{NumericKind, NumericParameter};
pub use parameter::{DynamicParameter, FixedParameter};
