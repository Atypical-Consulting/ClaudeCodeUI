//! Git invocation contracts — spec 04 §1.12 (WorktreeService) and spec 01 §5.7 (LiveSession.Git / GitRoot).
//! Owner: core-worktrees-files.
//! Every call: program `git`, args `-C <dir> …`, stdout/stderr drained concurrently (a chatty diff must not block),
//! optional stdin written after the readers started, no window on Windows.

use std::process::Stdio;
use std::sync::LazyLock;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Semaphore;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GitOut {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Caps concurrent git processes across every scan.
static GATE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(std::thread::available_parallelism().map_or(4, |n| n.get())));

fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped()).stdin(Stdio::null()).kill_on_drop(true);
    #[cfg(windows)]
    cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    cmd
}

fn git_command(dir: &str, args: &[&str]) -> Command {
    let mut cmd = command("git");
    cmd.arg("-C").arg(dir).args(args);
    cmd
}

/// Spawn, feed stdin (after the readers started), collect. Dropping the future kills the child (kill_on_drop).
async fn collect(mut cmd: Command, stdin: Option<String>) -> std::io::Result<GitOut> {
    if stdin.is_some() {
        cmd.stdin(Stdio::piped());
    }
    let mut child = cmd.spawn()?;
    let feeder = match (stdin, child.stdin.take()) {
        (Some(input), Some(mut pipe)) => Some(tokio::spawn(async move {
            let _ = pipe.write_all(input.as_bytes()).await; // a reader that exits early just closes the pipe
            drop(pipe);
        })),
        _ => None,
    };
    let out = child.wait_with_output().await?;
    if let Some(f) = feeder {
        let _ = f.await;
    }
    Ok(GitOut {
        code: out.status.code().unwrap_or(-1),
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
    })
}

/// WorktreeService contract: env `GIT_OPTIONAL_LOCKS=0`, `GIT_TERMINAL_PROMPT=0`, `GIT_SSH_COMMAND` (BatchMode,
/// ConnectTimeout=8 unless set), under the global semaphore (available_parallelism permits), optional timeout
/// (kill tree → Err). Spawn failure → Err(`Wt.GitMissing(os error)`), distinct from a non-zero exit (Ok).
pub async fn run(dir: &str, args: &[&str], stdin: Option<String>, timeout: Option<Duration>) -> Result<GitOut, String> {
    let mut cmd = git_command(dir, args);
    cmd.env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0").env(
        "GIT_SSH_COMMAND",
        std::env::var("GIT_SSH_COMMAND").unwrap_or_else(|_| "ssh -o BatchMode=yes -o ConnectTimeout=8".into()),
    ); // ls-remote never prompts
    let _permit = GATE.acquire().await.map_err(|e| e.to_string())?;
    let work = collect(cmd, stdin);
    let res = match timeout {
        Some(t) => tokio::time::timeout(t, work).await.map_err(|_| format!("git {}: timeout", args.join(" ")))?,
        None => work.await,
    };
    res.map_err(|e| ccui_shared::t!("Wt.GitMissing", e))
}

/// LiveSession.Git: exit 0 → Some(stdout), anything else (incl. spawn failure) → None.
pub async fn output(dir: &str, args: &[&str]) -> Option<String> {
    match collect(git_command(dir, args), None).await {
        Ok(o) if o.code == 0 => Some(o.stdout),
        _ => None,
    }
}

/// `gh` with `GH_PROMPT_DISABLED=1`, 15 s timeout, cwd = dir: Some(stdout) on exit 0, None otherwise (gh missing
/// included).
pub async fn gh(dir: &str, args: &[&str]) -> Option<String> {
    let mut cmd = command("gh");
    cmd.args(args).current_dir(dir).env("GH_PROMPT_DISABLED", "1");
    match tokio::time::timeout(Duration::from_secs(15), collect(cmd, None)).await {
        Ok(Ok(o)) if o.code == 0 => Some(o.stdout),
        _ => None, // no gh, not logged in, too slow: GitHub is simply not consulted
    }
}

/// `--git-common-dir` output → the main checkout root (its name is `.git` → its parent), else None (bare repo).
pub(crate) fn common_dir_root(out: &str) -> Option<String> {
    let common = out.trim().trim_end_matches(['/', '\\']);
    let p = std::path::Path::new(common);
    if common.is_empty() || p.file_name()? != ".git" {
        return None;
    }
    Some(p.parent()?.to_string_lossy().into_owned())
}

/// `LiveSession.GitRoot(dir)`: the main checkout root (`--git-common-dir` whose name is `.git` → its parent), else None.
pub async fn root(dir: &str) -> Option<String> {
    common_dir_root(&output(dir, &["rev-parse", "--path-format=absolute", "--git-common-dir"]).await?)
}

/// `git -C dir branch --show-current`: empty output → "HEAD" (detached); failure → None (NewSession.Branch).
pub async fn branch(dir: &str) -> Option<String> {
    let b = output(dir, &["branch", "--show-current"]).await?.trim().to_string();
    Some(if b.is_empty() { "HEAD".into() } else { b })
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Run git synchronously in a test fixture (author set, quiet).
    pub(crate) fn g(dir: &std::path::Path, args: &[&str]) {
        let ok = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "init.defaultBranch=main"])
            .args(args)
            .output()
            .unwrap();
        assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
    }

    #[tokio::test]
    async fn run_root_branch() {
        let tmp = tempfile::tempdir().unwrap();
        let d = tmp.path();
        assert_eq!(branch(&d.to_string_lossy()).await, None);
        assert_eq!(root(&d.to_string_lossy()).await, None);
        g(d, &["init", "-q"]);
        g(d, &["commit", "-q", "--allow-empty", "-m", "i"]);
        let ds = d.to_string_lossy().to_string();
        assert_eq!(branch(&ds).await.as_deref(), Some("main"));
        let r = root(&ds).await.unwrap();
        assert!(r.ends_with(&*d.file_name().unwrap().to_string_lossy()), "{r}");
        // stdin reaches the child after the readers started.
        let o = run(&ds, &["hash-object", "--stdin"], Some("x".repeat(200_000)), None).await.unwrap();
        assert_eq!((o.code, o.stdout.trim().len()), (0, 40));
        let bad = run(&ds, &["rev-parse", "--verify", "-q", "nope"], None, None).await.unwrap();
        assert_ne!(bad.code, 0);
        assert_eq!(common_dir_root("/a/b/.git\n"), Some("/a/b".into()));
        assert_eq!(common_dir_root("/a/b.git"), None);
    }
}
