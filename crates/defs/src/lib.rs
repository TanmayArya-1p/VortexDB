pub mod error;
pub mod types;

// Without re-exports, users would need to write defs::types::SomeType instead of just defs::SomeType. Re-exports simplify the API by flattening the module hierarchy. The * means "everything public" from that module.
pub use error::*;
pub use types::*;
