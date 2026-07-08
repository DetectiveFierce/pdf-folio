//! PDF-Folio cloud integration crate.
//!
//! This crate consolidates the sync client, sync control-plane server, and
//! Raindrop.io import client into a single crate, keeping Google sign-in, CRDT
//! metadata sync, R2 blob transfer, Turso credential retrieval, and Raindrop
//! HTTP/OAuth/download logic out of the UI and core crates.

pub mod raindrop;
pub mod server;
pub mod sync;
