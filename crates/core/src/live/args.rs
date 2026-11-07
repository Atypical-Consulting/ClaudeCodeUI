//! Port of `LiveSession.Args(resume)` — exact argv order, spec 01 §4. Owner: core-live-session.
//! Pure (the Thinking app flag is passed in) so the Self-check expectations port as unit tests.

use super::state::SessionState;
use ccui_shared::todo_list::ALLOWED_TOOLS_ARG;

/// `[--permission-mode M ("default" → "manual")] --include-partial-messages --forward-subagent-text
/// --allowedTools=… --include-hook-events --prompt-suggestions true` then resume / fork / fresh, then model,
/// effort, fallback, add-dirs, restricted, safe-mode, agent, `--thinking-display summarized` (iff thinking_flag).
pub fn args(s: &SessionState, resume: bool, thinking_flag: bool) -> Vec<String> {
    let mut a: Vec<String> = Vec::new();
    let mut add = |xs: &[&str]| a.extend(xs.iter().map(|x| x.to_string()));
    // No mode chosen (""): the CLI applies permissions.defaultMode, as in the terminal; initialize says which.
    if !s.mode.is_empty() {
        add(&["--permission-mode", if s.mode == "default" { "manual" } else { &s.mode }]);
    }
    // --prompt-suggestions: a prompt_suggestion after each result, as the terminal's ghost text.
    add(&["--include-partial-messages", "--forward-subagent-text", ALLOWED_TOOLS_ARG, "--include-hook-events", "--prompt-suggestions", "true"]);
    if resume {
        add(&["--resume", &s.id]);
    } else if let Some(src) = &s.fork_of {
        add(&["--resume", src, "--fork-session", "--session-id", &s.id, "--name", &s.name]);
        if let Some(at) = &s.fork_at {
            add(&["--resume-session-at", at]);
        }
    } else {
        add(&["--session-id", &s.id, "--name", &s.name]);
        if let (Some(w), None) = (&s.worktree, &s.heavy.init) {
            add(&["-w", w]);
        }
    }
    if let Some(m) = &s.model {
        add(&["--model", m]);
    }
    if let Some(e) = &s.effort {
        add(&["--effort", e]);
    }
    if let Some(fb) = &s.fallback_model {
        add(&["--fallback-model", fb]);
    }
    for d in &s.add_dirs {
        add(&["--add-dir", d]); // one flag each
    }
    if s.restricted {
        add(&["--restricted"]);
    }
    if s.safe_mode {
        add(&["--safe-mode"]);
    }
    // stream-json ignores an unknown name instead of exiting: a wrong one starts a plain session.
    if let Some(ag) = &s.agent {
        add(&["--agent", ag]);
    }
    // Without it stream-json sends every thinking block empty; read at each launch.
    if thinking_flag {
        add(&["--thinking-display", "summarized"]);
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: &str, name: &str, mode: &str) -> SessionState {
        SessionState::new(id, name, "/w", mode, chrono::Utc::now())
    }

    fn v(xs: &[&str]) -> Vec<String> {
        xs.iter().map(|x| x.to_string()).collect()
    }

    const PRE: [&str; 6] = ["--include-partial-messages", "--forward-subagent-text", ALLOWED_TOOLS_ARG, "--include-hook-events", "--prompt-suggestions", "true"];

    #[test]
    fn fresh_resume_fork() {
        let fresh = args(&st("id", "n", "default"), false, false);
        let mut want = v(&["--permission-mode", "manual"]);
        want.extend(v(&PRE));
        want.extend(v(&["--session-id", "id", "--name", "n"]));
        assert_eq!(fresh, want, "fresh args");

        let mut r = st("o", "o", "plan");
        r.model = Some("haiku".into());
        let resumed = args(&r, true, false);
        assert_eq!(&resumed[..2], &v(&["--permission-mode", "plan"])[..]);
        assert_eq!(&resumed[8..], &v(&["--resume", "o", "--model", "haiku"])[..], "resume args");

        let mut f = st("f", "x (fork)", "default");
        f.fork_of = Some("o".into());
        f.fork_at = Some("u1".into());
        assert_eq!(
            &args(&f, false, false)[8..],
            &v(&["--resume", "o", "--fork-session", "--session-id", "f", "--name", "x (fork)", "--resume-session-at", "u1"])[..],
            "fork args"
        );
        assert_eq!(&args(&f, true, false)[8..], &v(&["--resume", "f"])[..], "fork resumes its own transcript once written");

        let c = args(&st("c", "c", ""), false, false);
        assert_eq!(&c[..6], &v(&PRE)[..], "no mode chosen: no --permission-mode");
        assert_eq!(&c[6..8], &v(&["--session-id", "c"])[..]);
    }

    #[test]
    fn dirs_flags_thinking() {
        let mut d = st("d", "d", "default");
        d.add_dirs = v(&["/a", "/b c"]);
        assert!(args(&d, true, false).ends_with(&v(&["--resume", "d", "--add-dir", "/a", "--add-dir", "/b c"])), "resume keeps --add-dir");
        assert!(args(&st("t", "t", "default"), false, true).ends_with(&v(&["--name", "t", "--thinking-display", "summarized"])));
        let mut fb = st("fb", "fb", "default");
        fb.model = Some("opus".into());
        fb.fallback_model = Some("sonnet".into());
        fb.thinking = false;
        assert!(args(&fb, false, false).ends_with(&v(&["--model", "opus", "--fallback-model", "sonnet"])), "thinking off not anchored at launch");
        let mut k = st("k", "k", "default");
        k.agent = Some("pirate".into());
        let a = args(&k, false, false);
        let ai = a.iter().position(|x| x == "--agent").unwrap();
        assert_eq!(a[ai + 1], "pirate");
        k.restricted = true;
        k.safe_mode = true;
        let lock = args(&k, false, false);
        assert!(lock.contains(&"--restricted".into()) && lock.contains(&"--safe-mode".into()));
        assert!(!args(&st("k", "k", "default"), false, false).contains(&"--restricted".into()));
        let mut w = st("w", "w", "default");
        w.worktree = Some("x".into());
        assert!(args(&w, false, false).ends_with(&v(&["--name", "w", "-w", "x"])), "-w before the first init");
    }
}
