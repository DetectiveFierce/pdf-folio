# PDF-Folio Repo Restructure — Migration Checklist

Source inspected: `github.com/DetectiveFierce/pdf-folio` (branch `main`).

## 0. Current vs. target shape, in one picture

Current workspace has **12 members**:

```
iced-widget-patch  pdf-folio-core  pdf-folio-db  pdf-folio-main
pdf-folio-raindrop  pdf-folio-style  pdf-folio-sync  pdf-folio-sync-server
pdf-folio-ui  pdf-folio-ui-components  pdf-folio-viewer
```

Target has **6 members**:

```
iced-widget-patch  pdf-folio-main  pdf-folio-core  pdf-folio-cloud
pdf-folio-style  pdf-folio-ui
```

So this is not a pure "move files" job — it's a **crate consolidation**:

| Target crate | Absorbs |
|---|---|
| `iced-widget-patch` | itself, unchanged |
| `pdf-folio-main` | itself (renames `sync_cli.rs` → `cli.rs`) |
| `pdf-folio-core` | `pdf-folio-core` + `pdf-folio-db` + the *DB-mapping* half of `pdf-folio-raindrop` |
| `pdf-folio-cloud` | `pdf-folio-sync` + `pdf-folio-sync-server` + the *API/import* half of `pdf-folio-raindrop` |
| `pdf-folio-style` | itself, internally re-split into finer files |
| `pdf-folio-ui` | `pdf-folio-ui` + `pdf-folio-ui-components` + `pdf-folio-viewer` |

`pdf-folio-db`, `pdf-folio-raindrop`, `pdf-folio-sync`, `pdf-folio-sync-server`, `pdf-folio-ui-components`, and `pdf-folio-viewer` all disappear as separate crates.

Because of that, this migration is riskiest in the dependency-graph and visibility changes, not in the `git mv`s. The checklist below front-loads the mechanical scaffolding, then tackles each merge as its own phase so the build stays green (or close to it) between phases.

**Ground rule for the whole migration:** do this on a branch, in small commits, running `cargo check --workspace` (and `cargo test --workspace` once it compiles) after every sub-step. Several of the files being split are enormous (`app/update.rs` is 3,411 lines, `db/mod.rs` is 3,510 lines, `pdf-folio-raindrop/src/lib.rs` is 1,820 lines) — don't try to split-and-move in one shot; move the whole file first, get it compiling, *then* split it.

---

## Phase 1 — Scaffolding (no logic moves yet)

- [ ] Create branch `restructure/crate-consolidation`.
- [ ] Create the new crate directories with empty `Cargo.toml` + `src/lib.rs` (or `src/main.rs`):
  - `crates/pdf-folio-core` (already exists — will be edited in place)
  - `crates/pdf-folio-cloud` (new)
  - `crates/pdf-folio-main`, `crates/pdf-folio-style`, `crates/pdf-folio-ui` (already exist — edited in place)
- [ ] Update root `Cargo.toml` `[workspace.members]` to the final 6 entries, removing `pdf-folio-db`, `pdf-folio-raindrop`, `pdf-folio-sync`, `pdf-folio-sync-server`, `pdf-folio-ui-components`, `pdf-folio-viewer`, and adding `pdf-folio-cloud`. Do this at the *end* of each dependent phase, not all at once — see per-phase notes.
- [ ] Draft `pdf-folio-cloud/Cargo.toml` merging the dependency lists of `pdf-folio-sync` + `pdf-folio-raindrop` + `pdf-folio-sync-server`:
  - `anyhow`, `axum`, `base64`, `blake3`, `chrono`, `directories`, `hex`, `hmac`, `jsonwebtoken`, `rand`, `reqwest`, `serde`, `serde_json`, `sha2`, `tokio`, `tracing`, `tracing-subscriber`, `url`, `webbrowser`, `zip`
  - path dependency on `pdf-folio-core` (replaces old `pdf-folio-db` + `pdf-folio-core` path deps)
  - keep the four `[[bin]]` targets: `pdf-folio-sync-server`, `crdt-sync-once`, `ensure-turso-schema` (three already exist) — no 4th bin needed, `run.rs`/`cli.rs` are library modules, not new bins.
- [ ] Update `pdf-folio-core/Cargo.toml` to add the dependencies currently only in `pdf-folio-db`: `chrono`, `directories`, `notify`, `rusqlite`, `serde`, `serde_json`, `tantivy` (core already has `anyhow`, `blake3`, `image`, `lru`, `pdfium-render`, `rayon`, `thiserror`, `tokio`, `tracing`).
- [ ] Update `pdf-folio-style/Cargo.toml` — no dependency changes expected, this phase is a pure internal file re-split.
- [ ] Update `pdf-folio-ui/Cargo.toml`:
  - remove path deps on `pdf-folio-raindrop`, `pdf-folio-sync`, `pdf-folio-ui-components`, `pdf-folio-viewer`, `pdf-folio-db`
  - add path deps on `pdf-folio-core` (replaces `pdf-folio-core`+`pdf-folio-db`) and `pdf-folio-cloud` (replaces `pdf-folio-sync`+`pdf-folio-raindrop`)
  - keep `pdf-folio-style`, `iced`, and the rest of its existing deps (`rfd`, `zip`, `reqwest`, `webbrowser`, `notify`, etc.)
- [ ] Update `pdf-folio-main/Cargo.toml`:
  - remove path deps on `pdf-folio-db`, `pdf-folio-sync`
  - add path dep on `pdf-folio-core` (for any direct DB/CLI use) — confirm after inspecting `main.rs`/`cli.rs` which symbols they actually import (mechanically: `grep -n "pdf_folio_" crates/pdf-folio-main/src/*.rs` after the rename)
  - `pdf-folio-ui` path dep stays

---

## Phase 2 — Build `pdf-folio-core` (merge core + db + raindrop's DB half)

Target shape:
```
pdf-folio-core/src/
  lib.rs
  pdf/{mod.rs, document.rs, renderer.rs, geometry.rs, tests.rs}
  db/{mod.rs, types.rs, schema.rs, library.rs, organization.rs,
      metadata.rs, import.rs, search.rs, raindrop.rs, tests.rs}
```

### 2a. `pdf/` subtree (from current `pdf-folio-core`)

| Current | New |
|---|---|
| `src/document.rs` + `src/document/tests.rs` | `pdf/document.rs` (+ tests merged into `pdf/tests.rs`, or kept as submodule and re-exported from `pdf/tests.rs`) |
| `src/renderer.rs` + `src/renderer/tests.rs` | `pdf/renderer.rs` |
| `src/annotations.rs` | **split** — see decision below |
| `src/lib.rs` | becomes `pdf/mod.rs` (module wiring + doc comments), with the crate's new top-level `lib.rs` just doing `pub mod pdf; pub mod db;` + re-exports |

- [ ] `git mv crates/pdf-folio-core/src/document.rs crates/pdf-folio-core/src/pdf/document.rs` (and its `document/tests.rs` alongside)
- [ ] `git mv crates/pdf-folio-core/src/renderer.rs crates/pdf-folio-core/src/pdf/renderer.rs` (and `renderer/tests.rs`)
- [ ] Delete `src/annotations.rs` and remove `Annotation`, `AnnotationId`, and `AnnotationKind` from the public API. Annotations are not currently implemented, so they should not be preserved as dead model types. Move only still-used generic geometry primitives such as `ColorRgba`, `PagePoint`, and `PageRect` into `pdf/geometry.rs`; if any are annotation-only and unused, delete those too.
- [ ] Consolidate `document/tests.rs` and `renderer/tests.rs` into `pdf/tests.rs`, or leave them as `#[path]` submodules and have `pdf/tests.rs` just be a thin aggregator — match whichever convention `db/tests.rs` ends up using for consistency.
- [ ] Rewrite `pdf-folio-core/src/lib.rs` to declare `pub mod pdf; pub mod db;` and re-export the same active public surface (`PdfDoc`, `RenderedPage`, `TileCache`, `TileKey`, `OutlineNode`, `PageTextChar`, `PageTextLayer`, `TextRect`, plus still-used geometry primitives and everything from `db`). Do not re-export annotation types.

### 2b. `db/` subtree (from current `pdf-folio-db`, 3,510-line `db/mod.rs`)

This is the highest-effort single step in the whole migration: `db/mod.rs` needs to be decomposed by feature area into `types.rs`, `schema.rs`, `library.rs`, `organization.rs`, `metadata.rs`, `import.rs`, `search.rs`, `raindrop.rs`. Recommended approach:

- [ ] First, move the whole crate wholesale and get it compiling as a submodule before touching internal structure:
  - `git mv crates/pdf-folio-db/src/db crates/pdf-folio-core/src/db`
  - `git mv crates/pdf-folio-db/src/importer.rs crates/pdf-folio-core/src/db/import.rs` (+ `importer/tests.rs` → fold into `db/tests.rs` or a `db/import/tests.rs` submodule)
  - `git mv crates/pdf-folio-db/src/indexer.rs crates/pdf-folio-core/src/db/search.rs` (+ `indexer/tests.rs`)
  - `git mv crates/pdf-folio-db/src/watcher.rs crates/pdf-folio-core/src/db/import.rs` — fold `LibraryWatcher` / `LibraryWatchEvent` and the filesystem-watching logic into `db/import.rs`.
  - `git mv crates/pdf-folio-db/src/lib.rs` content → merge into `pdf-folio-core/src/db/mod.rs`'s module doc comment, then delete the old `lib.rs`.
  - At this point `cargo check -p pdf-folio-core` should pass (module paths inside the crate are unaffected since `db/mod.rs` still has everything in one file).
- [ ] Now split `db/mod.rs` itself. Use `grep -n "^pub fn\|^fn \|^pub struct\|^struct \|^impl "` on the current file to inventory it, then bucket by area:
  - `types.rs` — already exists (577 lines) as a separate file; leave as-is, just confirm nothing in `mod.rs` duplicates a type that belongs there.
  - `schema.rs` (new) — `Db::open`, `PRAGMA`/migration setup, `CREATE TABLE` strings, schema-version bookkeeping.
  - `library.rs` (new) — core `LibraryEntry`/`EntryId` CRUD: insert/lookup/update/delete entries, trash state.
  - `organization.rs` (new) — folders, tags, ordering/gap-preserving position logic, `Folder`/`FolderId`, membership tables. (`naming.rs`, 41 lines, likely belongs here too — fold it in or keep as a private helper submodule.)
  - `metadata.rs` (new) — title/author/page-count editing, attribution flags, library preferences (`LibraryPreferences`, sort/layout modes).
  - `import.rs` — importer + watcher functions (see above), `ImportSource`, `ImportSummary`/`ImportedEntry`.
  - `search.rs` — tantivy wrapper (`SearchIndex`, `SearchHit`, `IndexDocument`).
  - `raindrop.rs` (new) — the raindrop-*mapping* functions currently living in `db/mod.rs`: `RaindropCollectionMapping`, `RaindropEntryMapping`, `upsert_raindrop_collection_mapping`, `upsert_raindrop_entry_mapping`, `raindrop_collection_folder`. This is deliberately *not* the raindrop API client — that goes to `pdf-folio-cloud/raindrop/` in Phase 3. Keep the split narrow: only DB-table-facing code stays here.
  - `mod.rs` shrinks to the `Db` struct definition, `pub mod` wiring, and any glue that doesn't cleanly belong to one bucket.
  - `tests.rs` (697 lines) — split alongside the above by area if practical, or leave as one file re-declared under `db/tests.rs` with `#[path]` includes per area; match whatever precedent Phase 2a's `pdf/tests.rs` sets.
- [ ] Re-run `cargo check -p pdf-folio-core` after every file you carve out, not at the end — `db/mod.rs` has enough internal cross-references between these areas (e.g. metadata edits touching library rows) that big-bang splitting invites silent visibility bugs (`pub(crate)` items becoming unreachable across new module boundaries).
- [ ] Update `pdf-folio-core/src/lib.rs`'s re-exports to keep the existing public API surface (`Db, EntryFolderMembership, EntryId, ..., SyncSeedSummary`) working, now sourced from the new submodules.

### 2c. Fold in raindrop's DB-mapping half

- [ ] From `pdf-folio-raindrop`, nothing physically moves into `pdf-folio-core` as a *file* — `RaindropCollectionMapping`/`RaindropEntryMapping` and their `upsert_*`/`raindrop_collection_folder` functions already live in `pdf-folio-db` today (confirmed via its `lib.rs` re-exports), so they land in `db/raindrop.rs` automatically as part of 2b. Just double check nothing raindrop-API-specific (OAuth, HTTP client, ZIP handling) accidentally lives in `pdf-folio-db` before finishing this phase — it doesn't, per current source, but verify with `grep -rn "reqwest\|zip::" crates/pdf-folio-db/src`.
- [ ] Do **not** yet delete `crates/pdf-folio-raindrop` — its import/client logic still needs to land in `pdf-folio-cloud` in Phase 3, which depends on `pdf-folio-core`. Delete it at the end of Phase 3 instead.

### 2d. Wire up and validate

- [ ] Remove `crates/pdf-folio-db` from the workspace members list and delete the directory once `cargo check -p pdf-folio-core` is green and nothing else references it.
- [ ] `cargo check -p pdf-folio-core` and `cargo test -p pdf-folio-core`.
- [ ] Do **not** update `pdf-folio-ui`/`pdf-folio-main` `use` paths yet if you can avoid it — they'll break anyway until Phases 3–5 land; batch those edits into Phase 6 to avoid churn.

---

## Phase 3 — Build `pdf-folio-cloud` (merge sync + sync-server + raindrop's API half)

Target shape:
```
pdf-folio-cloud/src/
  lib.rs
  sync/{mod.rs, client.rs, session.rs, auth.rs, remote.rs, blobs.rs,
        crdt.rs, status.rs, run.rs, cli.rs, tests.rs}
  raindrop/{mod.rs, auth.rs, client.rs, types.rs, import.rs, matching.rs, tests.rs}
  server/{mod.rs, config.rs, auth.rs, handlers.rs, storage.rs, tests.rs}
  bin/{pdf-folio-sync-server.rs, crdt-sync-once.rs, ensure-turso-schema.rs}
```

### 3a. `sync/` subtree (from current `pdf-folio-sync`)

| Current | New | Notes |
|---|---|---|
| `google_auth.rs` | `sync/auth.rs` | direct rename |
| `session.rs` | `sync/session.rs` | direct rename |
| `r2_client.rs` | `sync/blobs.rs` | R2 object storage client |
| `blob_cache.rs` | fold into `sync/blobs.rs` | consolidate with the R2/blob-storage code; do not keep a standalone `blob_cache.rs` file |
| `turso_client.rs` | `sync/remote.rs` | Turso/libSQL metadata client |
| `sync.rs` | split across `sync/client.rs`, `sync/crdt.rs`, `sync/status.rs`, `sync/run.rs` | inspect this file's exports first (`SyncClient`, `SyncPlan`, `SyncCrdtPreflight`, `SyncCrdtReport`, `SyncCheckpoint`, `SyncRunReport`, `SyncHydrationReport`, `SyncBlobUploadReport`, `SyncLibraryRow`, `REGISTRY_LIBRARY_ID`) and bucket by name: `SyncClient` + connection/plan setup → `client.rs`; CRDT operation types/merge logic → `crdt.rs`; checkpoint/report/status types → `status.rs`; the actual run loop that drives a full sync pass → `run.rs` |
| `bin/crdt_sync_once.rs` | `bin/crdt-sync-once.rs` | filename gets a dash (matches target); update `[[bin]] path` in Cargo.toml |
| `bin/ensure_turso_schema.rs` | `bin/ensure-turso-schema.rs` | same |
| — | `sync/cli.rs` | CLI-facing sync helpers live here. `pdf-folio-main/src/cli.rs` should import these helpers and remain a thin argument-parsing/dispatch layer. |

- [ ] `git mv crates/pdf-folio-sync crates/pdf-folio-cloud/src/sync` as a first pass (whole-directory move), get it compiling as a flat module, *then* do the client/crdt/status/run split described above.
- [ ] Move `turso_schema.sql` alongside (`crates/pdf-folio-cloud/turso_schema.sql`), update the `include_str!`/path reference in `remote.rs`/`ensure-turso-schema.rs`.

### 3b. `raindrop/` subtree (from current `pdf-folio-raindrop`, 1,820-line `lib.rs`)

- [ ] `git mv crates/pdf-folio-raindrop crates/pdf-folio-cloud/src/raindrop` wholesale first; rename its `lib.rs` to `raindrop/mod.rs`.
- [ ] `auth.rs` → `raindrop/auth.rs` (no change needed beyond path)
- [ ] `types.rs` → `raindrop/types.rs` (no change needed)
- [ ] `tests.rs` → `raindrop/tests.rs`
- [ ] Split the current `lib.rs`/`mod.rs` body by function group (grep confirms these exist today):
  - `raindrop/client.rs` — the `RaindropClient` struct and its HTTP methods (`user`, `collections`, `raindrop`, `pdf_raindrops`, `download_pdf*`, `get_json`), plus the small response-shape structs (`UserResponse`, `CollectionsResponse`, `RaindropsResponse`, `RaindropResponse`) if not already in `types.rs`.
  - `raindrop/import.rs` — the public entry points (`import_all_pdfs`, `import_selected_pdfs*`, `import_preview*`, `import_prepared_raindrops`, `import_raindrop_pdf*`, `mirror_collections`, `import_pdf_with_metadata`, progress-percentage helpers) and the private `ImportedRaindropPdf` type.
  - `raindrop/matching.rs` — `ZipMatchIndex`, `unique_remaining_match`, `normalized_zip_file_name`/`normalized_zip_file_stem`, `extract_selected_pdfs_from_zip`, `choose_import_strategy`.
  - `raindrop/mod.rs` — left with module wiring, top-level doc comment, `can_import_without_prompt`, and re-exports.
- [ ] This file has a lot of cross-references between the three new files (import.rs calls into client.rs and matching.rs constantly) — expect several rounds of `pub(crate)`/visibility fixes. Do this split in its own commit, isolated from the `db/mod.rs` split in Phase 2, so a bad split is easy to `git revert` independently.
- [ ] Confirm no lingering dependency on `pdf-folio-db`'s raindrop mapping functions breaks — `raindrop/import.rs` will now call `pdf_folio_core::db::raindrop::*` (or wherever 2b landed them) instead of `pdf_folio_db::*`. Update those `use` statements as part of this phase, since `pdf-folio-cloud` depends on `pdf-folio-core`.

### 3c. `server/` subtree (from current `pdf-folio-sync-server`, 635-line `main.rs`)

Inventoried via the current file's function/struct list:

| Goes to | Current pieces |
|---|---|
| `server/config.rs` | `Config` struct + `Config::load`, `load_google_credentials`, `load_turso_credentials`, `load_r2_credentials`, `parse_labeled_secret`, `parse_labeled_url`, `env_nonempty`, the `GoogleCredentials`/`GoogleCredentialFile`/`GoogleInstalledCredentials`/`TursoCredentials`/`R2Credentials` structs |
| `server/auth.rs` | `verify_google_identity`, `require_session`, `SessionClaims`, `exchange_google_code`, `google_userinfo`, `GoogleTokenResponse`, `GoogleUserInfo` |
| `server/handlers.rs` | `health`, `google_callback`, `turso_token`, `r2_upload_token`, `r2_download_token`, the axum `Router` construction, `AppState`, request/response DTOs (`GoogleCallbackRequest`, `R2UploadRequest`, `R2DownloadQuery`, `HealthResponse`, `SessionResponse`, `TursoTokenResponse`, `R2UploadResponse`, `R2DownloadResponse`, `ErrorResponse`), `ApiError` + its `IntoResponse` impl |
| `server/storage.rs` | `presigned_r2_url`, `sigv4_signing_key`, `hmac_sha256`, `percent_encode_path`, `percent_encode`, `r2_blob_key`, `validate_hash` |
| `server/mod.rs` | module wiring + a `pub async fn run() -> Result<()>` that the thin bin wraps |
| `bin/pdf-folio-sync-server.rs` | just `#[tokio::main] async fn main() { tracing setup; pdf_folio_cloud::server::run().await }` |

- [ ] `git mv crates/pdf-folio-sync-server crates/pdf-folio-cloud` — move `main.rs` content into `server/{config,auth,handlers,storage}.rs` as above, and reduce the old `main.rs` to the thin `bin/pdf-folio-sync-server.rs` shown above.
- [ ] Note `R2UploadResponse`/`R2DownloadResponse` currently exist as struct names *both* in `pdf-folio-sync-server` (server-side response DTOs) and `pdf-folio-sync` (client-side, re-exported from `pdf-folio-sync`'s `lib.rs` per its module doc). These are two distinct types serving two sides of the same HTTP contract — once both crates merge into `pdf-folio-cloud`, they'll collide by name in the same crate unless disambiguated (they're in different modules — `sync::` vs `server::` — so this is fine as long as nothing does a globbed `pub use` of both into the crate root). Flag this explicitly and check `pdf-folio-cloud/src/lib.rs`'s top-level re-exports don't re-export both under the same name.

### 3d. Wire up and validate

- [ ] Write `pdf-folio-cloud/src/lib.rs`: `pub mod sync; pub mod raindrop; pub mod server;` plus re-exports mirroring the union of what `pdf-folio-sync`, `pdf-folio-raindrop`, and `pdf-folio-sync-server` (which had none — it's a binary) exported.
- [ ] Remove `pdf-folio-sync`, `pdf-folio-raindrop`, `pdf-folio-sync-server` from workspace members; delete the three directories.
- [ ] `cargo check -p pdf-folio-cloud` and `cargo test -p pdf-folio-cloud`.

---

## Phase 4 — Re-split `pdf-folio-style` (no crate merge, just internal reorg)

Target shape adds structure the current crate doesn't have yet:
```
pdf-folio-style/src/
  lib.rs, theme.rs, tokens.rs
  book/{mod.rs, parser.rs, sources.rs, tests.rs}
  classes/{mod.rs, core.rs, library.rs, viewer.rs}   (tests.rs already exists)
  components/{mod.rs, core.rs, library.rs, viewer.rs}
  borders/{mod.rs, side.rs}
```

- [ ] `book.rs` (1,678 lines, currently one file, no `book/` subdir exists yet) → create `book/mod.rs`, `book/parser.rs`, `book/sources.rs`, `book/tests.rs`. Grep the file for KDL-parsing functions vs. fallback/bundled-source functions (`fallback_dark_tokens`, `fallback_light_tokens`, `StyleBook::bundled`) to decide the parser/sources boundary — parsing (`StyleBook::load`, KDL document walking) → `parser.rs`; hardcoded fallback token sets and embedded default source strings → `sources.rs`; `StyleBook` struct itself + public API → `mod.rs`.
- [ ] `classes.rs` (990 lines) + existing `classes/tests.rs` → split into `classes/mod.rs` (shared `Class`, `ComponentState`, `VisualOverride` enums), `classes/core.rs` (generic `button_style`/`container_style`/`text_input_style`/etc.), `classes/library.rs` (library-card/sidebar-specific style fns), `classes/viewer.rs` (`viewer_primitives`, `ViewerPrimitiveStyle`). Match against the component split below for consistency — a style class and its component builder for "library" should live in parallel files.
- [ ] `components.rs` (364 lines) → `components/mod.rs` + `components/core.rs` (generic: `icon_button`, `toolbar_button`, `search_input`, `tag_pill`, `progress_bar`, `section_heading`) + `components/library.rs` (`library_card`, `library_row`, `selection_checkbox`, `master_checkbox`) + `components/viewer.rs` (`annotation_popover`, `annotation_toolbar`, `toc_entry`).
- [ ] `side_border.rs` (229 lines) + `side_border/tests.rs` → `git mv` to `borders/side.rs` + `borders/tests.rs`... **wait**, target only lists `borders/{mod.rs, side.rs}` (no `tests.rs` under borders) — keep the existing test module attached to `side.rs` via `#[cfg(test)] mod tests;` inline, or as `side/tests.rs`, rather than inventing a `borders/tests.rs` the spec didn't ask for. `borders/mod.rs` just does `pub mod side; pub use side::side_border;`.
- [ ] `theme.rs`, `tokens.rs`, `lib.rs` stay at the top level unchanged, aside from updating `pub mod` declarations and re-export paths to point at the new submodules.
- [ ] Move `styles/*.kdl` assets — **no change needed**, they already match the target layout (`styles/application.kdl`, `styles/themes/{espresso,light}.kdl`, `styles/components/core.kdl`, `styles/components/library/{library,sidebar}.kdl`, `styles/components/viewer/viewer.kdl`) and `assets/fonts/*` also already match. Just double-check `include_str!`/`include_bytes!` paths in the moved Rust files still resolve (they're relative to the crate, so a source-file move to a subdirectory doesn't affect them, but double-check any `../../styles/...`-style relative literal in code).
- [ ] `cargo check -p pdf-folio-style` and `cargo test -p pdf-folio-style` (the crate's only test today, `bundled_fonts_include_display_family_weights` in `lib.rs`, plus whatever moves with `book`/`classes`/`side_border`).

---

## Phase 5 — Rebuild `pdf-folio-ui` (merge ui-components + viewer, reorganize shell/components/library/viewer)

This is the largest and highest-risk phase. Target shape:
```
pdf-folio-ui/src/
  lib.rs, tests.rs
  shell/{mod.rs, app.rs, messages.rs, update.rs, tasks.rs, subscriptions.rs,
         platform.rs, session.rs, shortcuts.rs, constants.rs}
  components/
    shared/{mod, root_surface, overlays, loading, empty_state, toolbar, sidebar,
             menus, context_menu, command_palette, icons, buttons, inputs,
             metadata, selection, drag, error_banner, sync_status}.rs
    library/{mod, filters, metadata, selection, drag, state, view, cards,
              folder_tree, inspector, dialogs, import_status}.rs
    viewer/{mod, canvas, controls, outline, toolbar, sidebar, page_controls,
             zoom, find_bar}.rs
  library/{mod, state, update, data, actions, layout, tasks, thumbnails,
            registry/{mod, state, session, preview, tasks}.rs,
            view/{mod, root, entries, folders, sidebar}.rs}
  viewer/{mod, document, state, update, navigation, layout, rendering, tasks,
           view/{mod, root, document}.rs}
```

### 5a. First, absorb the two satellite crates wholesale (no internal reorg yet)

- [ ] `git mv crates/pdf-folio-ui-components/src/library crates/pdf-folio-ui/src/ui_components_library` (temporary staging name to avoid colliding with the existing `pdf-folio-ui/src/library/`), drop the crate's `lib.rs`/`events` module — the empty `Event` enum can just disappear if nothing constructs it (grep for `ui_components::events::Event` / `pdf_folio_ui_components::events` usage first).
- [ ] `git mv crates/pdf-folio-viewer/src/state.rs crates/pdf-folio-ui/src/viewer_crate_state.rs` (temporary staging name), drop the crate's empty `Event` enum the same way.
- [ ] Update `pdf-folio-ui/Cargo.toml` to drop the two path deps, delete both crate directories once nothing references them, remove them from workspace members.
- [ ] Fix up `pdf-folio-ui/src/library/mod.rs` (currently `pub(crate) use pdf_folio_ui_components::library::{drag, filters, metadata, selection, state};`) and `pdf-folio-ui/src/viewer/mod.rs` (currently `pub(crate) use pdf_folio_viewer::state;`) to point at the staged in-crate modules instead. Get `cargo check -p pdf-folio-ui` green here before doing any further reorganizing — this confirms the two crate-merges didn't break anything before you also start moving files around internally.

### 5b. `shell/` (from `app/*` top-level files + free functions currently in `lib.rs`)

| Current | New | Notes |
|---|---|---|
| `app/mod.rs` (30 lines, module wiring) | `shell/mod.rs` | plus the `run()` fn and its helpers currently at the top of `lib.rs` (`initial_window_size`) |
| `app/state.rs` (500 lines, `PDFolioApp` struct) | `shell/app.rs` | target has no separate `shell/state.rs`, so the app-state definition itself becomes `app.rs`'s content |
| `app/messages.rs` (906 lines) | `shell/messages.rs` | direct rename |
| `app/update.rs` (3,411 lines!) | `shell/update.rs` **only for shell-scoped routing** | see decision point below — most of this needs to fan out to `library/update.rs` and `viewer/update.rs` |
| `app/update/shortcuts.rs` (236 lines) | fold into `shell/shortcuts.rs` | |
| `app/update/tasks.rs` (658 lines) | fan out to `shell/tasks.rs` / `library/tasks.rs` / `viewer/tasks.rs` by which subsystem the task belongs to | |
| `app/subscriptions.rs` (495 lines) | `shell/subscriptions.rs` | direct rename |
| `app/platform.rs` (73 lines) | `shell/platform.rs` | direct rename |
| `app/session.rs` (527 lines) | `shell/session.rs` | direct rename; **also** fold `app/sync_auth.rs` (185 lines) in here, since target has no separate shell-level auth file and this is app-session/sign-in state |
| `app/shortcuts.rs` (287 lines) | `shell/shortcuts.rs` | merge with `app/update/shortcuts.rs` above |
| `app/constants.rs` (39 lines) | `shell/constants.rs` | direct rename |
| free functions in `lib.rs`: `save_app_session_task`, `with_session_save`, `open_file_manager_task`, `open_file_dialog_task`, `import_folder_dialog_task`, `import_pdf_dialog_task`, `export_destination_dialog_task`, `relink_file_dialog_task`, `save_library_preferences_task`, `schedule_search` | `shell/tasks.rs` | these are exactly the kind of top-level "kick off a `Task<Message>`" helpers `shell/tasks.rs` is for |
| free functions in `lib.rs`: `library_search_match_label`, `truncated_title`, `truncate_for_width`, `truncate_for_width_with_font`, `file_tree_label`, `file_tree_font` | resolved split | Card truncation helpers go to `components/library/cards.rs` when used by library cards; viewer toolbar truncation goes to `components/viewer/toolbar.rs`; file-tree label helpers go to `components/library/folder_tree.rs`; shared font helpers go to `components/shared/typography.rs` only if used broadly. |
| `app/icons.rs` (11 lines) | `components/shared/icons.rs` | not shell — moves to components (see 5c) |
| `app/context_menu.rs` (702 lines) | `components/shared/context_menu.rs` | not shell — moves to components |
| `app/view/mod.rs` (778 lines) | mostly `components/shared/root_surface.rs` | Move the entire `view()` function plus private overlay-stacking helpers into `components/shared/root_surface.rs`: `command_palette_capture_layer`, `view_command_palette`, `view_signed_out`, `view_library_name_dialog`, loading/spinner layers, and `class_text_color`. `shell/app.rs` should not own rendering logic; at most it has `PDFolioApp::view()` delegating to `components::shared::root_surface::view(self)`. Move `viewer_find_anchor`, `view_viewer_find_bar`, and `viewer_find_icon_button` to `components/viewer/find_bar.rs`; move `dismissible_error_banner` to `components/shared/error_banner.rs`. |
| `app/view/library_switcher.rs` (581 lines) | `components/shared/menus.rs` | a library-switching dropdown is a menu component; confirm this is the best fit vs. `sidebar.rs` by reading the file, since "switcher" could also be sidebar-adjacent |
| `app/view/viewer_toolbar.rs` (362 lines) | `components/viewer/toolbar.rs` | direct conceptual match |
| `app/viewer_layout.rs` (161 lines) | `viewer/layout.rs` | not shell — moves to the `viewer/` module (see 5e) |
| `app/viewer_navigation.rs` (269 lines) | `viewer/navigation.rs` | not shell |
| `app/viewer_state.rs` (1,100 lines) | `viewer/state.rs` (merge with the staged `pdf-folio-viewer` state from 5a) | not shell |
| `app/libraries.rs` (642 lines) | `library/registry/*` | not shell — moves to the library registry (see 5d) |
| `app/library_clipboard.rs`, `library_data.rs`, `library_drag.rs`, `library_folders.rs`, `library_layout.rs`, `library_selection.rs`, `library_view_state.rs` | `library/*` | not shell (see 5d) |

- [ ] Split `app/update.rs` using this boundary:

  - `shell/update.rs` keeps arms that touch app-mode, window state, global chrome, sync/auth, library switching, and global input:
    - Startup: `StartupResponsivenessProbe`, `StartupBackgroundReady`
    - Sync/auth: `SyncSignInRequested`, `SyncSignInFinished`, `AutoSyncTick`, `RemoteSyncAvailable`, `LibraryRegistryRemoteAvailable`, `AutoSyncFinished`, `LibraryRegistrySyncFinished`, `LibraryPreviewRefreshed`, `PendingRaindropRollbackChecked`, `PendingRaindropRollbackFinished`
    - Global chrome: `CursorMoved`, `ContextMenuOpened*`, `ContextMenuClosed`, `OpenCommandPalette`, `CloseCommandPalette`, `CommandPalette*`, `ContextMenuActionSelected`
    - App-mode/navigation: `BackToLibrary`, `BackToViewer`, `DocumentOpened`, `LibraryDocumentOpened`, `ThemeToggled`, `ReloadStyles`, `StylesReloaded`, `ToggleSidebar`, `ToggleTocPanel`, `ToggleViewMode`
    - Library switcher: `OpenLibrarySwitcher`, `CloseLibrarySwitcher`, `SelectLibrary`, `ToggleLibraryCardMenu`, `CloseLibraryCardMenu`, `OpenLibraryNameDialog`, `CancelLibraryNameDialog`, `ConfirmLibraryNameDialog`, `NewLibraryNameChanged`, `CreateLibrary`, `LibraryRegistryUpdated`, `LibraryRenameInputChanged`, `RenameLibrary`, `RequestDeleteLibrary`, `DeleteLibrary`
    - Global input: `AnimationFrame`, `ModifiersChanged`, `WindowResized`, `ShortcutPressed`, `FileDialogCanceled`, `FileSelected`, `OpenFileDialog`
    - Confirmation flow: `RequestConfirmation`, `CancelConfirmation`, `ConfirmPendingAction`. Keep the generic dispatcher in `shell/update.rs`, but move the actual library delete / permanently-delete / folder action execution into `library::actions`.

  - `library/update.rs` takes everything with a `Library*`, `Folder*`, `Tag*`, `Import*`, `Raindrop*`, `Entry*`, `Export*`, `Bulk*`, `Search*`, `Move*`, `Details*`, `Inspector*`, or `Trash*` prefix, excluding the library-switcher subset listed above. This includes `LibrarySortChanged` through `ToggleLibraryTreeFolder` / `LibraryWatchEvent*`, all folder/tag/import/raindrop messages, selection and drag messages, clipboard/history messages, export flow, metadata edit flow, bulk operations, thumbnails, progress, and saved-progress handling.

  - `viewer/update.rs` takes viewer rendering and interaction messages: `PageRendered`, jump dialog/page navigation, outline toggling, text-layer and text-selection messages, canvas clicks, selection copy/clear, scrolling, viewport changes, wheel scrolling, zoom-settle and zoom commands, viewer mode selection, viewer find open/close/update commands, `CloseOverlay` if it only closes viewer overlays after verification, and `ViewerSidebarTabSelected`.

- [ ] Mechanics for the split:
  - Move the whole file to `shell/update.rs` first, unsplit, and get it compiling/passing tests.
  - Add stub functions:
    - `library::update::update(app, message) -> Option<Task<Message>>`
    - `viewer::update::update(app, message) -> Option<Task<Message>>`
  - Return `None` for unhandled messages at first.
  - In `shell/update.rs`, replace relocated arms with a fallthrough:
    `other => library::update::update(app, other).or_else(|| viewer::update::update(app, other)).unwrap_or_else(Task::none)`.
  - Move arms in small batches by prefix so `cargo test -p pdf-folio-ui` stays green between commits.
  - Split `app/update/tasks.rs` along the same boundaries by grepping which `Message` variant each task function feeds.

### 5c. `components/` (mostly the absorbed `pdf-folio-ui-components` + parts of `app/`)

| Current | New |
|---|---|
| staged `ui_components_library/{drag, filters, metadata, selection, state}.rs` | `components/library/{drag, filters, metadata, selection, state}.rs` |
| staged `ui_components_library/view.rs` (410 lines) | `components/library/view.rs` + `components/library/cards.rs` — target adds a `cards.rs` not present today; grep `view.rs` for card-rendering functions vs. general view-composition functions and split accordingly |
| `library/view/inspector.rs` (155 lines, currently under `pdf-folio-ui/src/library/view/`) | `components/library/inspector.rs` |
| `library/view/dialogs.rs` (1,614 lines) | `components/library/dialogs.rs` |
| `library/view/sidebar.rs` (1,509 lines) | split: generic reusable sidebar chrome → `components/shared/sidebar.rs`; folder-tree rendering → `components/library/folder_tree.rs`; anything left that's specifically the library-mode sidebar composition → stays as `library/view/sidebar.rs` (see 5d) |
| — | `components/library/import_status.rs` (new) — grep `library/tasks.rs` and `app/libraries.rs` for import-progress UI rendering (raindrop import progress bars, etc.) that currently lives inline; extract into this file |
| `app/icons.rs` | `components/shared/icons.rs` |
| `app/context_menu.rs` | `components/shared/context_menu.rs` |
| `viewer/canvas.rs` (597 lines) | split: the actual iced `Canvas`/widget drawing → `components/viewer/canvas.rs`; render-scheduling/zoom-policy logic (`ZoomRenderPolicy`) → `viewer/rendering.rs` (see 5e) |
| `viewer/outline.rs` (304 lines) | `components/viewer/outline.rs` |
| `viewer/zoom.rs` (377 lines) | split: zoom control widgets → `components/viewer/zoom.rs`; zoom state/percent math used by non-view code → stays in `viewer/state.rs` |
| `viewer/outline.rs` top half | `components/viewer/sidebar.rs` | `view_sidebar`, `viewer_sidebar_tab_button`, `view_outline_body`, `view_thumbnails_body`, `thumbnail_button`, `sidebar_scroll_direction` |
| `viewer/outline.rs` bottom half | `components/viewer/outline.rs` | `outline_list`, `outline_button`; keep actual TOC tree rendering separate from the sidebar shell |
| `app/view/viewer_toolbar.rs` toolbar functions | `components/viewer/toolbar.rs` | `view_viewer_toolbar`, `viewer_library_back_button`, `viewer_toolbar_title`, `viewer_toolbar_status_label`, `viewer_toolbar_title_width`, `viewer_floating_sidebar_toggle` |
| `app/view/viewer_toolbar.rs` page-nav functions + jump dialog | `components/viewer/page_controls.rs` | `viewer_page_control`, `viewer_page_chevron_button`, plus `view_jump_dialog` from `viewer/outline.rs` |
| `app/view/mod.rs` find-bar block | `components/viewer/find_bar.rs` | `viewer_find_anchor`, `view_viewer_find_bar`, `viewer_find_icon_button` |
| `viewer/zoom.rs` UI half | `components/viewer/zoom.rs` | `zoom_control`, `zoom_menu`, `zoom_chevron_button`, `zoom_menu_row`, plus `view_zoom_menu_dropdown` and `zoom_menu_capture_layer` from `app/view/mod.rs` |
| `viewer/zoom.rs` math half | `viewer/rendering.rs` | `automatic_zoom_width`, `page_width_zoom`, `page_fit_width`, `percent_width`, `SpreadZoomMetrics`, `current_spread_metrics`, `current_spread_pages`, `page_width_for_group`, `available_page_width`, `available_page_height` |
| `viewer/canvas.rs` `ZoomRenderPolicy` enum | `viewer/rendering.rs` | scheduling/render policy, not a widget component |
| rest of `viewer/canvas.rs` | `components/viewer/canvas.rs` | `ViewerCanvas`, `ViewerSelectionOverlay`, `HistoryRestoreSpinner`, drawing helpers, geometry helpers, and `scroll_delta_pixels` |
| — | `components/viewer/controls.rs` | optional thin `pub use` aggregator only. Do not force unique content into it if toolbar/page-controls/zoom/find-bar already cover the real widgets. |
| — | `components/shared/{overlays, loading, empty_state, toolbar, menus, command_palette, buttons, inputs, metadata, selection, drag, error_banner, sync_status}.rs` — several of these (loading, empty_state, error_banner already exist as named functions in `pdf-folio-style`'s `components.rs` re-exports: `empty_state`, `error_banner`). Distinguish **style-layer builders** (which stay in `pdf-folio-style`, already covered in Phase 4) from **UI-layer components that call those builders** (which is what these new `pdf-folio-ui` files are for). Don't duplicate logic — these files should be thin wrappers wiring `pdf_folio_style::{empty_state, error_banner, ...}` into actual `PDFolioApp`/`Message`-typed views. `sync_status.rs` in particular is new UI surfacing the sync state from `pdf-folio-cloud`; confirm whether this already exists inline somewhere in `app/view/mod.rs` before writing it from scratch. |
| — | `components/mod.rs`, `components/shared/mod.rs`, `components/library/mod.rs`, `components/viewer/mod.rs` — new wiring files, straightforward `pub mod ...;` + re-exports |

### 5d. `library/` (top-level library subsystem, distinct from `components/library/`)

| Current | New |
|---|---|
| `library/mod.rs` | `library/mod.rs` (rewritten to declare the new submodules instead of re-exporting from the now-gone `pdf-folio-ui-components`) |
| `app/library_view_state.rs` (228 lines) | `library/state.rs` |
| portion of `app/update.rs` / `app/update/tasks.rs` handling library messages | `library/update.rs` (new — see the decision point in 5b) |
| `app/library_data.rs` (157 lines) | `library/data.rs` |
| `app/library_clipboard.rs` (137 lines) + `app/library_folders.rs` (328 lines, folder CRUD) | `library/actions.rs` (new — copy/paste/duplicate + folder create/rename/delete/move are all "user-triggered library actions") |
| `app/library_layout.rs` (247 lines) | `library/layout.rs` |
| `library/tasks.rs` (1,706 lines, stays largely as-is) | `library/tasks.rs` |
| `library/thumbnails.rs` (175 lines) | `library/thumbnails.rs` (unchanged) |
| `app/library_drag.rs` (839 lines) | split: app-state mutation/drag-session logic → `library/actions.rs` or `library/state.rs`; anything purely about rendering the drag ghost/preview stays with the already-relocated `components/library/drag.rs` (5c) |
| `app/libraries.rs` (642 lines, `LibraryProfile`/`LibraryRegistryRuntime`/`LibraryNameDialog`/`load_library_registry`) | `library/registry/{state.rs, session.rs, preview.rs, tasks.rs}` — split by: registry struct/state → `registry/state.rs`; load/save of the registry file → `registry/session.rs`; any "preview a library before switching" logic → `registry/preview.rs`; async loading tasks → `registry/tasks.rs` |
| `library/view/mod.rs` (933 lines) | `library/view/root.rs` + trimmed `library/view/mod.rs` | `library/view/root.rs` gets `view_library(app)` only. `library/view/mod.rs` keeps reusable cross-cutting helpers: `view_library_header`, `library_header_title`, `view_library_selection_toolbar`, `library_header_button`, `library_sync_indicator`, `last_sync_tooltip_label`, `format_local_time`, `view_library_breadcrumb_row`, `library_search_input`, `library_toolbar_available_width`, `library_new_folder_button`, `library_history_icon_button`, `library_quick_filter_chips`, `library_filter_summary`, `breadcrumb_button`, and `library_scrollable`. |
| `library/view/entries.rs` (526 lines) | `library/view/entries.rs` (unchanged) |
| `library/view/folders.rs` (615 lines) | `library/view/folders.rs` (unchanged) |
| `library/view/sidebar.rs` (1,509 lines) | whatever remains after extracting `components/shared/sidebar.rs` and `components/library/folder_tree.rs` (5c) stays here as the library-mode sidebar composition |
| `library/view/inspector.rs`, `library/view/dialogs.rs` | fully relocate to `components/library/{inspector,dialogs}.rs` (5c) — nothing stays behind in `library/view/` for these two |

### 5e. `viewer/` (top-level viewer subsystem, distinct from `components/viewer/`)

| Current | New |
|---|---|
| `viewer/mod.rs` (7 lines) | `viewer/mod.rs` (rewritten to declare the new submodules) |
| staged `viewer_crate_state.rs` (from `pdf-folio-viewer`, 447 lines) + `app/viewer_state.rs` (1,100 lines) | merged into `viewer/state.rs` — these two need reconciling since they may already overlap (both currently deal with viewer state; the crate one is scroll/spread/selection/find state per its module doc, the app one is the runtime `ViewerRuntime`-style state holding the open document). Read both fully before merging to avoid duplicate type definitions. |
| — | `viewer/document.rs` (new) — likely a slice of the merged state above: the `PdfDoc` handle + outline/text-layer wrapper specific to "what document is open", separated out from scroll/zoom/selection state |
| portion of `app/update.rs` / `app/update/tasks.rs` handling viewer messages | `viewer/update.rs` (new — see decision point in 5b) |
| `app/viewer_navigation.rs` (269 lines) | `viewer/navigation.rs` |
| `app/viewer_layout.rs` (161 lines) | `viewer/layout.rs` |
| `viewer/canvas.rs` rendering-scheduling half (see 5c) | `viewer/rendering.rs` (new) |
| `viewer/tasks.rs` (59 lines) | `viewer/tasks.rs` (unchanged) |
| viewer branch currently inline in `app/view/mod.rs` | `viewer/view/root.rs` + `viewer/view/document.rs` | `viewer/view/root.rs` owns the viewer-mode composition: toolbar/sidebar chrome around the document viewport. `viewer/view/document.rs` owns the page-content-only portion: canvas + text-selection overlay + scrollable wrapping. `viewer/view/mod.rs` is thin module wiring/re-exports. |

### 5f. Dead weight to remove

- [ ] `views/{mod.rs, library.rs, settings.rs, sidebar.rs, viewer.rs}` — confirmed via inspection to be vestigial scaffold stubs (`LibraryView` marker struct, etc., 5–6 lines each). The target tree has no top-level `views/` module at all. Grep for any real usage (`grep -rn "views::" crates/pdf-folio-ui/src`) before deleting — if genuinely unused beyond the module declarations, delete the whole `views/` directory rather than migrating it.
- [ ] `pdf-folio-ui-components`'s `events::Event` and `pdf-folio-viewer`'s `Event` — both are empty placeholder enums (`pub enum Event {}`). Confirm nothing constructs or matches on either (they can't be constructed since they're empty, so this is likely dead code) and drop them rather than inventing a merged `Event` type the target tree doesn't ask for.

### 5g. Validate

- [ ] `cargo check -p pdf-folio-ui` after each of 5b/5c/5d/5e lands (not just once at the end) — the shell/library/viewer split has three-way circular-looking references (shell dispatches to library and viewer; library and viewer both reference shared components; components reference style) so get the module tree compiling incrementally, ideally shell → components → library → viewer in that order since later ones depend on earlier ones existing.
- [ ] `cargo test -p pdf-folio-ui` — this crate has the most existing tests (`tests.rs` at the crate root, plus the `#[cfg(test)]` blocks scattered through `app/*`), and behavior preservation should be checked here most carefully since this phase is the most invasive.

---

## Phase 6 — `pdf-folio-main`

- [ ] `git mv crates/pdf-folio-main/src/sync_cli.rs crates/pdf-folio-main/src/cli.rs`.
- [ ] Update `use` paths in `main.rs`/`cli.rs`: anything importing from `pdf_folio_db`/`pdf_folio_sync` now imports from `pdf_folio_core`/`pdf_folio_cloud`.
- [ ] Update `Cargo.toml` path deps per Phase 1.
- [ ] `cargo check -p pdf-folio-main` and a manual smoke run of the `pdf-folio` binary.

---

## Phase 7 — Full-workspace cleanup and validation

- [ ] Global search for now-stale crate names across the whole tree: `grep -rn "pdf_folio_db\|pdf_folio_raindrop\|pdf_folio_sync::\|pdf_folio_sync_server\|pdf_folio_ui_components\|pdf_folio_viewer" --include=*.rs crates/` — anything left should be zero once Phases 2–6 are done.
- [ ] Confirm `Cargo.toml` `[workspace.members]` lists exactly the 6 target crates.
- [ ] Regenerate `Cargo.lock` (`cargo generate-lockfile` or just let `cargo check` do it) and confirm no unexpected extra crates got pulled in.
- [ ] `cargo check --workspace`, `cargo clippy --workspace --all-targets`, `cargo test --workspace`, `cargo fmt --check` (or run `cargo fmt` if the repo doesn't gate on it).
- [ ] Check the packaging files (`packaging/folio-sync-server.Dockerfile`, `.compose.yml`, `.service`) for hardcoded binary paths or crate names referencing `pdf-folio-sync-server` as a standalone crate — update to build from `pdf-folio-cloud`'s `pdf-folio-sync-server` bin target instead.
- [ ] Re-check any repo-root docs (`docs/docs.html`) or READMEs that describe the old crate layout, and update them to describe the new 6-crate layout.
- [ ] Diff the public API surface of each crate's `lib.rs` re-exports before/after (a quick `cargo doc --no-deps -p <crate>` before and after the whole migration, diffed, is a good sanity check) to make sure nothing was silently dropped.

---

## Ordering summary (do these top-to-bottom; each is its own PR/commit set)

1. Phase 1 — scaffolding
2. Phase 2 — `pdf-folio-core` (core + db + raindrop's DB half)
3. Phase 3 — `pdf-folio-cloud` (sync + sync-server + raindrop's API half) — depends on Phase 2 for the DB-mapping types
4. Phase 4 — `pdf-folio-style` internal split (independent of 2/3, can run in parallel)
5. Phase 5 — `pdf-folio-ui` (ui-components + viewer + shell/library/viewer reorg) — depends on Phases 2, 3, 4 all being in place
6. Phase 6 — `pdf-folio-main` — depends on Phases 2, 3, 5
7. Phase 7 — cleanup and full-workspace validation

## Resolved implementation decisions

1. `Annotation` / `AnnotationId` / `AnnotationKind` should be removed from the codebase. Annotations are not currently implemented, so do not preserve or relocate the annotation model. Keep only generic PDF geometry primitives in `pdf/geometry.rs` if they are still used.
2. `db/watcher.rs` should be folded into `db/import.rs`.
3. `blob_cache.rs` should be consolidated into `sync/blobs.rs`.
4. CLI-facing sync helpers should live in the sync crate under `pdf-folio-cloud/src/sync/cli.rs`. `pdf-folio-main/src/cli.rs` should stay focused on argument parsing and command dispatch, importing helper functions from `pdf_folio_cloud::sync::cli`.
5. `app/update.rs` should split into:
   - `shell/update.rs` for app-mode, window, global chrome, sync/auth, library-switcher, global input, and generic confirmation dispatch.
   - `library/update.rs` for all library-content operations.
   - `viewer/update.rs` for viewer rendering, navigation, zoom, find, text-selection, and viewer-sidebar operations.
6. Text helper placement:
   - Card title truncation helpers go in `components/library/cards.rs` when used by library cards.
   - Viewer toolbar truncation helpers go in `components/viewer/toolbar.rs` when used by viewer toolbar/title code.
   - File-tree label helpers go in `components/library/folder_tree.rs`.
   - Shared font helpers go in `components/shared/typography.rs` only if used broadly across multiple component areas.
7. `app/view/mod.rs` should not split between shell rendering and root-surface rendering. The rendering logic belongs in `components/shared/root_surface.rs`. `shell/app.rs` may contain only a thin `PDFolioApp::view()` wrapper delegating to `components::shared::root_surface::view(self)`.
8. `library/view/mod.rs` and viewer view code split as:
   - `library/view/root.rs`: `view_library(app)` only.
   - `library/view/mod.rs`: shared library-view helpers such as header, toolbar, breadcrumbs, search, filters, sync indicator, and scrollable helpers.
   - `viewer/view/root.rs`: viewer-mode page composition with toolbar/sidebar chrome.
   - `viewer/view/document.rs`: page-content-only document viewport, canvas/selection stack, and scrollable wrapper.
   - `viewer/view/mod.rs`: thin module wiring/re-exports.
9. Viewer component boundaries:
   - `components/viewer/sidebar.rs`: viewer sidebar shell, tabs, thumbnail/outline body selection.
   - `components/viewer/outline.rs`: actual TOC tree rendering.
   - `components/viewer/toolbar.rs`: viewer toolbar and title/status/sidebar-toggle widgets.
   - `components/viewer/page_controls.rs`: page navigation controls and jump dialog.
   - `components/viewer/find_bar.rs`: viewer find bar and find icon button.
   - `components/viewer/zoom.rs`: zoom UI controls/dropdown only.
   - `viewer/rendering.rs`: zoom math, spread metrics, render policy, and other non-widget rendering/layout math.
   - `components/viewer/canvas.rs`: iced canvas programs and drawing helpers.
   - `components/viewer/controls.rs`: optional thin re-export shim only; do not force real content into it if the specific files above cover everything.
