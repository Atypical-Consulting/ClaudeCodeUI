//! ccui-shared — the IPC contract between the Tauri backend (`ccui-core`) and the Leptos UI (`ccui-ui`), plus every
//! PURE helper both sides need. Compiles to wasm32 and native: no tokio, no fs, no process, no clock-dependent tests
//! without an explicit `now`.
//!
//! Contract modules (owned by unit `tauri-glue`, see docs/ARCHITECTURE.md): `model`, `session`, `push`, `api`.
//! Everything else is a port of one C# file; the header of each file names it.
#![allow(dead_code, unused_variables, unused_imports)] // skeleton phase: stubs are todo!()

pub mod i18n;
pub mod json;

// --- contract ---
pub mod api;
pub mod model;
pub mod push;
pub mod session;

// --- pure ports (spec 02) ---
pub mod api_errors;
pub mod ask_user;
pub mod elicit;
pub mod hooks;
pub mod permission_rules;
pub mod thread_blocks;
pub mod todo_list;
pub mod tool_kinds;
pub mod usage;

// --- pure ports (spec 05) ---
pub mod cli_edit;
pub mod fmt;
pub mod invisible;
pub mod list_nav;
pub mod mcp;
pub mod md;
pub mod memory;
pub mod plugins;
pub mod workflow;

// --- pure ports (spec 01 / 03 / 04) ---
pub mod bg_agents;
pub mod edit_diff;
pub mod file_index;
pub mod images;
pub mod notifications;
pub mod prompt_history;
pub mod session_rules;
pub mod status_command;
pub mod text;
pub mod worktree_rules;

pub use model::Time;
