//! `aval mcp install|uninstall` — register aval's MCP server with Claude Code.
//!
//! Registration is `claude`'s to do and to answer for: this module only runs
//! `claude mcp add|remove|get` and reads its exit status, never its config
//! files. An absent `claude` is a gap worth one line, not a failure: the
//! machine simply has no Claude Code to register with.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The name the server is registered under.
pub const SERVER_NAME: &str = "aval";

/// How long `claude` may take to answer before it is reported as unable to.
const CLAUDE_DEADLINE: Duration = Duration::from_secs(30);

/// What running `claude` came to.
#[derive(Debug)]
pub enum Ran {
    /// No `claude` on PATH.
    Absent,
    /// It ran and exited; `None` is death by signal.
    Exited { code: Option<i32>, output: String },
    /// It could not be run, or did not answer within the deadline.
    Unable(String),
}

/// `claude mcp add --scope user aval -- <exe> mcp`, as argv after `claude`.
pub fn add_args(exe: &Path) -> Vec<String> {
    ["mcp", "add", "--scope", "user", SERVER_NAME, "--"]
        .iter()
        .map(|s| s.to_string())
        .chain([exe.display().to_string(), "mcp".to_string()])
        .collect()
}

/// `claude mcp remove --scope user aval`, as argv after `claude`.
pub fn remove_args() -> Vec<String> {
    ["mcp", "remove", "--scope", "user", SERVER_NAME]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// `claude mcp get aval`, as argv after `claude`.
pub fn get_args() -> Vec<String> {
    ["mcp", "get", SERVER_NAME]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// Run `claude` with `args`, bounded by [`CLAUDE_DEADLINE`].
///
/// holds-until: `claude mcp add|remove|get` prints more than a pipe buffer
/// (64 KiB) before exiting. Both pipes are read only after it exits, so a
/// larger answer would block it and read as "did not answer" at the deadline.
pub fn run_claude(args: &[String]) -> Ran {
    let mut child = match Command::new("claude")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ran::Absent,
        Err(e) => return Ran::Unable(e.to_string()),
    };
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() >= CLAUDE_DEADLINE => {
                let _ = child.kill(); // already past its deadline; a failed kill changes nothing we report
                let _ = child.wait(); // reaping only
                return Ran::Unable(format!(
                    "`claude` did not answer within {} s",
                    CLAUDE_DEADLINE.as_secs()
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            Err(e) => return Ran::Unable(e.to_string()),
        }
    }
    match child.wait_with_output() {
        Ok(o) => Ran::Exited {
            code: o.status.code(),
            output: format!(
                "{}{}",
                String::from_utf8_lossy(&o.stdout),
                String::from_utf8_lossy(&o.stderr)
            ),
        },
        Err(e) => Ran::Unable(e.to_string()),
    }
}

/// Is aval registered? `Unknown` when `claude` cannot say, `NoClaude` when it
/// is not on PATH.
#[derive(Debug, PartialEq, Eq)]
pub enum Registered {
    Yes,
    No,
    Unknown(String),
    NoClaude,
}

/// The exit status `claude mcp get` gives for a server that is not there;
/// any other non-zero status is a failure to answer.
const NOT_REGISTERED_EXIT: i32 = 1;

/// Ask `claude mcp get aval`. Exit 0 is registered, exit 1 is not registered,
/// anything else is unknown.
pub fn registered() -> Registered {
    match run_claude(&get_args()) {
        Ran::Absent => Registered::NoClaude,
        Ran::Unable(why) => Registered::Unknown(why),
        Ran::Exited { code: Some(0), .. } => Registered::Yes,
        Ran::Exited {
            code: Some(NOT_REGISTERED_EXIT),
            ..
        } => Registered::No,
        Ran::Exited { code, output } => Registered::Unknown(format!(
            "`claude mcp get` exited {}: {}",
            code.map_or("by signal".to_string(), |c| c.to_string()),
            output.trim()
        )),
    }
}
