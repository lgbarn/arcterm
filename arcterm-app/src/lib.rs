//! Library target for arcterm-app integration tests.
//!
//! Re-exports a minimal public surface so that `tests/` can access the
//! `Terminal` and `PreFilter` types without duplicating module declarations.
//!
//! This file is compiled only when the `lib` target is built (e.g. during
//! `cargo test --test engine_migration`).  The binary entry point remains
//! `src/main.rs`.

pub mod prefilter;
pub mod terminal;

// Re-export the types integration tests need.
pub use prefilter::PreFilter;
pub use terminal::Terminal;

// Internal modules required by prefilter and terminal.
pub(crate) mod kitty_types;
pub(crate) mod osc7770;
pub(crate) mod proc;
