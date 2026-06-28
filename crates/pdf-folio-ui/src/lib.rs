//! Native iced UI shell for PDF-Folio.

pub mod app;
pub mod messages;
pub mod style;
pub mod theme;
pub mod views;

pub use app::{run, AppMode, PDFolioApp, Settings};
