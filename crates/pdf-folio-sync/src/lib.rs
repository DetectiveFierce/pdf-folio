//! Transitional shim crate.
//!
//! The sync implementation has moved to `pdf_folio_cloud::sync` during crate
//! consolidation. This crate remains temporarily so existing consumers can be
//! rewired in a later phase without breaking intermediate builds.

pub use pdf_folio_cloud::sync::*;
