//! Transport- and database-free NeonDojo rules and shared configuration options.
//!
//! Both the headless backend (`neondojo`) and the native GPUI client
//! (`neondojo-desktop`) depend on this crate so backgammon legality, GNUbg IDs,
//! and shared option limits have a single Rust implementation.

pub mod backgammon;
pub mod options;
