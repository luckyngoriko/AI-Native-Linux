//! CLI subcommand modules for the `aios` binary.
//!
//! Each module holds the clap subcommand definitions and dispatch logic for
//! one top-level `aios <noun>` subtree. The dispatch functions accept an
//! already-connected [`crate::AiosClient`].

pub mod apps;
