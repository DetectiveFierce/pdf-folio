//! PDF viewer state and surface types for PDF-Folio.
//!
//! This crate holds the viewer-side data model that is independent of the
//! main application shell:
//!
//! - [`state`] defines scroll modes, spread modes, rendered-page views,
//!   text-selection anchors, and find-in-document state.
//! - [`Event`] is the (currently empty) event enum reserved for future
//!   viewer-originated messages consumed by the application update loop.

pub mod state;

#[derive(Debug, Clone)]
pub enum Event {}
