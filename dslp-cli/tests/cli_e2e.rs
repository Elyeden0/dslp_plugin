//! This file used to hold all CLI integration tests. As of v0.6 the tests
//! are split into one file per plugin so each stays under the file-size
//! threshold:
//!
//! - `core_e2e.rs` — scan / next / findings / score, --json
//! - `lang_ts_e2e.rs` — TypeScript / JavaScript plugin
//! - `lang_go_e2e.rs` — Go plugin
//! - `fw_react_e2e.rs` — React framework plugin
//! - `fw_nextjs_e2e.rs` — Next.js framework plugin
//! - `fw_nuxt_e2e.rs` — Nuxt framework plugin
