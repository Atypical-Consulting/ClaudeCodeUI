//! ccui-core — every native behaviour of the old .NET server, without Tauri: claude processes (stream-json),
//! the event parser and session reducer, the session registry, transcripts, worktrees, git, plugins/MCP/memory,
//! workflow runs, background agents, settings/prefs/ledgers (same on-disk paths and formats as the C# app).
//!
//! It never depends on tauri: everything the UI must learn goes through [`UiSink`]. The Tauri shell
//! (desktop/src-tauri) implements the sink, holds an `Arc<Core>` in managed state, and exposes thin commands.
//! Errors cross IPC as `Result<T, String>` (`CoreResult`); nothing here panics on malformed input.
#![allow(dead_code, unused_variables, unused_imports)] // skeleton phase: stubs are todo!()

pub mod paths;

// protocol (core-events)
pub mod boot_probe;
pub mod events;
pub mod process;
pub mod process_job;

// session engine (core-live-session)
pub mod live;
pub mod status_command;

// store & registry (core-store-sessions)
pub mod app_flag;
pub mod background_agents;
pub mod bookmarks;
pub mod notifications;
pub mod replay;
pub mod session_manager;
pub mod transcript_store;
pub mod user_settings;

// worktrees & files (core-worktrees-files)
pub mod edit_diff;
pub mod file_index;
pub mod folder;
pub mod git;
pub mod new_session;
pub mod worktrees;

// extensions (core-extensions)
pub mod cli;
pub mod cli_edit;
pub mod memory_files;
pub mod permission_rules;
pub mod plugins;
pub mod workflow_runs;

use ccui_shared::push::Push;
use std::sync::Arc;

/// Error type over IPC: a message for the person (already localized when it comes from an i18n key).
pub type CoreResult<T> = Result<T, String>;

/// Where core sends what the UI must learn. Implemented by the Tauri shell (`desktop/src-tauri/src/sink.rs`),
/// by a recording fake in tests.
pub trait UiSink: Send + Sync + 'static {
    /// Broadcast one push to the webview (`ccui_shared::push::PUSH_EVENT`).
    fn emit(&self, push: Push);
    /// Post a native notification; a click brings the window up and navigates to `session/<id>`.
    fn notify(&self, session_id: &str, title: &str, body: &str);
    /// Whether the main window has focus (notifications are posted only when it does not).
    fn window_focused(&self) -> bool;
}

/// The app's backend state, held by the shell in `tauri::State<Arc<Core>>`.
pub struct Core {
    pub sink: Arc<dyn UiSink>,
    pub sessions: Arc<session_manager::SessionManager>,
    pub worktrees: Arc<worktrees::WorktreeService>,
    pub searches: transcript_store::SearchRegistry,
    pub file_scans: file_index::ScanRegistry,
    pub agent_logs: workflow_runs::AgentLogs,
}

impl Core {
    /// Build the state; sets the app-wide i18n language from the persisted culture (else `os_locale`).
    pub fn new(sink: Arc<dyn UiSink>, os_locale: Option<String>) -> Arc<Core> {
        ccui_shared::i18n::set_lang(app_flag::culture(os_locale.as_deref()));
        ccui_shared::i18n::set_mac(cfg!(target_os = "macos"));
        let sessions = session_manager::SessionManager::new(sink.clone());
        let worktrees = worktrees::WorktreeService::new(sessions.clone(), sink.clone());
        Arc::new(Core {
            sink,
            sessions,
            worktrees,
            searches: Default::default(),
            file_scans: Default::default(),
            agent_logs: Default::default(),
        })
    }

    /// App exit / before an update: dispose every session in parallel (each may wait 2 s for its process).
    pub async fn shutdown(&self) {
        self.sessions.dispose_all().await
    }
}
