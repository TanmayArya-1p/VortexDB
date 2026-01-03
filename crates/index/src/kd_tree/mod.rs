use defs::Magic;

pub mod index;
mod serialize;
pub mod types;

#[cfg(test)]
mod tests;

pub const KD_TREE_MAGIC_BYTES: Magic = [0x00, 0x00, 0x00, 0x00];
