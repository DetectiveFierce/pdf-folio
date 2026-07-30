//! PDF-Folio cloud integration crate.
//!
//! This crate is the remote-services boundary for PDF-Folio. It consolidates
//! three products that talk to the network, keeping OAuth, CRDT metadata sync,
//! object storage, and third-party import logic out of the UI and core crates.
//!
//! # The three products
//!
//! | Product | Module / binary | Role |
//! | --- | --- | --- |
//! | Sync client | [`sync`] | Desktop library API and `pdf-folio sync` CLI: Google PKCE sign-in, session cache, Turso CRDT exchange, R2 blob transfer, automatic sync passes |
//! | Control-plane server | [`server`] + bin `pdf-folio-sync-server` | Single-user identity gate that verifies Google, enforces an allow-list, mints session JWTs, and returns short-lived Turso credentials / R2 presigned URLs |
//! | Raindrop import | [`raindrop`] | Raindrop.io OAuth, REST listing, ZIP bulk export matching, and PDF download/import into a local library |
//!
//! Maintenance binaries live under `src/bin/`:
//!
//! - `pdf-folio-sync-server` — starts [`server::run`]
//! - `crdt-sync-once` — one-shot CRDT + hydration pass against a local library DB
//! - `ensure-turso-schema` — applies `turso_schema.sql` with direct Turso credentials
//!
//! # Architecture (sync)
//!
//! Sync is scoped to **one user, several machines**, not multi-user collaboration.
//! The control plane sits **beside** the data path, not on it:
//!
//! 1. Desktop signs in via Google OAuth (PKCE) and posts the code to the control plane.
//! 2. The server checks the Google identity against an allow-list and returns a session JWT.
//! 3. The client uses that JWT to request Turso credentials and R2 presigned URLs.
//! 4. Metadata CRDT operations go **directly** to Turso (Hrana / SQL-over-HTTP).
//! 5. PDF bytes go **directly** to Cloudflare R2 at keys `blobs/<blake3>.pdf`.
//!
//! Local durable state (entries, folders, CRDT op log, Raindrop provenance tables)
//! remains in `pdf-folio-core`; this crate only orchestrates remote exchange.
//!
//! # Module map
//!
//! - [`sync`] — client-side sync coordinator, auth, session, CRDT, blobs, CLI
//! - [`server`] — axum control plane: config, auth, handlers, R2/Turso helpers
//! - [`raindrop`] — Raindrop HTTP client, OAuth, import pipeline, ZIP matching

pub mod raindrop;
pub mod server;
pub mod sync;
