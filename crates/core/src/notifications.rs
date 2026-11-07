//! Port of the notification observer of `MainLayout.razor` + `Core/Notifications.cs` — spec 03 §8.2, 06 §2.
//! Owner: core-store-sessions. Runs in the backend on every session change (synchronously, so Waiting → answered →
//! Waiting inside one burst is still seen). Posts via `UiSink::notify` only when the `notify-on` flag is set and the
//! window is not focused. Pure decisions: `ccui_shared::notifications`.

use crate::live::LiveSession;
use crate::UiSink;
use ccui_shared::model::{SessionStatus, Time};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct NotifyObserver {
    /// id → (status, remembered result time, pushes). Empty until `prime`.
    seen: Mutex<HashMap<String, (SessionStatus, Option<Time>, i32)>>,
}

impl NotifyObserver {
    /// First observation: record without notifying (what is already waiting is on screen, not news).
    pub fn prime(&self, sessions: &[Arc<LiveSession>]) {
        todo!("port: MainLayout first observation")
    }

    /// Forget closed sessions, then per session: Decision / Done / PushNotification → post.
    pub fn observe(&self, sessions: &[Arc<LiveSession>], sink: &dyn UiSink) {
        todo!("port: MainLayout.OnSessions / Due / Post")
    }
}

/// `Notifications.ShellOn` = AppFlag `notify-on`.
pub fn shell_on() -> bool {
    todo!("port: Notifications.ShellOn")
}
