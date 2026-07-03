//! Reusable UI component logic for PDF-Folio's library surface.
//!
//! This crate collects stateless helpers shared between the application shell
//! and the library view:
//!
//! - [`library`] exposes drag-and-drop state, filtering predicates, metadata
//!   formatting, selection helpers, and view builders for library entries.
//! - [`events::Event`] is the (currently empty) event enum reserved for
//!   future component-originated messages.

pub mod library;

pub mod events {
    #[derive(Debug, Clone)]
    pub enum Event {}
}
