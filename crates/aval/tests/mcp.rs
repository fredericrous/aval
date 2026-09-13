//! `aval mcp` at the protocol level: bytes in on stdin, bytes out on stdout.
//!
//! The server is driven as a process rather than through `mcp::handle`, because
//! the failures worth catching here are transport failures — a response that is
//! never flushed, a reply sent to a notification, a malformed line that kills
//! the loop — and none of those are visible to a caller that skips the pipe.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::{fs, thread, time::Duration};

fn bin() -> &'static str {
    env!("CARGO_BIN_EXE_aval")
}

fn scratch(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/mcp-tests")
        .join(name);
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(p.join("docs/adr")).expect("mkdir");
    p
}

fn write(root: &Path, rel: &str, body: &str) {
    let p = root.join(rel);
    if let Some(d) = p.parent() {
        fs::create_dir_all(d).expect("mkdir");
    }
    fs::write(p, body).expect("write");
}

const REGISTRY: &str = "dir: docs/adr\nscopes: [cloud]\nkeys:\n  a.b:\n  c.d:\n  gone.key:\n";

const ONE: &str = "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
                   - key: a.b\n    choice: One\n    first: true\n---\n# one\n";

/// A slot decided, then retired. A retirement with `first: true` must name
/// what it opts out of, so the decision it replaces comes first.
const DECIDED_THEN_RETIRED: &str = "---\nid: ADR-0006\nstatus: accepted\ndecisions:\n  \
                                    - key: gone.key\n    choice: Kept\n    first: true\n\
                                    ---\n# kept\n";

const RETIRE: &str = "---\nid: ADR-0007\nstatus: accepted\ndecisions:\n  \
                      - key: gone.key\n    retire: true\n    replaces: [ADR-0006]\n    \
                      reason: no longer ours\n---\n# gone\n";

/// A corpus exercising active, undecided, retired and unknown.
fn corpus(name: &str) -> PathBuf {
    let r = scratch(name);
    write(&r, ".adr.yaml", REGISTRY);
    write(&r, "docs/adr/0001-one.md", ONE);
    write(&r, "docs/adr/0006-kept.md", DECIDED_THEN_RETIRED);
    write(&r, "docs/adr/0007-gone.md", RETIRE);
    r
}

/// Two records answering one slot: a contradiction.
fn conflicted(name: &str) -> PathBuf {
    let r = scratch(name);
    write(
        &r,
        ".adr.yaml",
        "dir: docs/adr\nscopes: []\nkeys:\n  a.b:\n",
    );
    write(&r, "docs/adr/0001-one.md", ONE);
    write(
        &r,
        "docs/adr/0002-two.md",
        "---\nid: ADR-0002\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: Two\n    first: true\n---\n# two\n",
    );
    r
}

// --- driving the server ----------------------------------------------------

/// A live server with stdin held open, so a test can observe that a reply
/// arrives BEFORE the stream closes.
struct Server {
    child: Child,
    out: BufReader<std::process::ChildStdout>,
}

impl Server {
    fn start(dir: &Path) -> Server {
        let mut child = Command::new(bin())
            .args(["-C", dir.to_str().unwrap(), "mcp"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("spawn aval mcp");
        let out = BufReader::new(child.stdout.take().expect("stdout"));
        Server { child, out }
    }

    /// Write raw bytes to stdin WITHOUT closing it.
    fn send_raw(&mut self, s: &str) {
        let stdin = self.child.stdin.as_mut().expect("stdin");
        stdin.write_all(s.as_bytes()).expect("write");
        stdin.flush().expect("flush");
    }

    /// Read one reply line. Blocks, so it fails the test rather than hanging
    /// silently if the server never flushes.
    fn reply(&mut self) -> Json {
        let mut line = String::new();
        let n = self.out.read_line(&mut line).expect("read");
        assert!(n > 0, "server closed stdout without replying");
        parse(&line)
    }

    fn finish(mut self) -> String {
        drop(self.child.stdin.take());
        let o = self.child.wait_with_output().expect("wait");
        String::from_utf8_lossy(&o.stderr).to_string()
    }
}

/// One shot: feed every line, close stdin, collect the replies.
fn exchange(dir: &Path, lines: &[&str]) -> Vec<Json> {
    let mut child = Command::new(bin())
        .args(["-C", dir.to_str().unwrap(), "mcp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn");
    {
        let stdin = child.stdin.as_mut().expect("stdin");
        for l in lines {
            writeln!(stdin, "{}", l).expect("write");
        }
    }
    let o = child.wait_with_output().expect("wait");
    assert!(
        o.stderr.is_empty(),
        "stderr: {:?}",
        String::from_utf8_lossy(&o.stderr)
    );
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(parse)
        .collect()
}

fn call(tool: &str, args: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{{"name":"{}","arguments":{}}}}}"#,
        tool, args
    )
}

// A tiny reader, so the tests read the protocol the way a client would rather
// than by matching substrings in a line.
use aval_core::json::Json;

fn parse(s: &str) -> Json {
    aval_core::json::parse(s.trim()).unwrap_or_else(|e| panic!("reply is not JSON ({}): {}", e, s))
}

fn result(j: &Json) -> &Json {
    j.get("result")
        .unwrap_or_else(|| panic!("no result: {}", j))
}

fn tool_result(j: &Json) -> (&Json, bool) {
    let r = result(j);
    let is_err = matches!(r.get("isError"), Some(Json::Bool(true)));
    (
        r.get("structuredContent").expect("structuredContent"),
        is_err,
    )
}

fn err_code(j: &Json) -> i64 {
    j.get("error")
        .and_then(|e| e.get("code"))
        .and_then(|c| c.as_i64())
        .unwrap_or_else(|| panic!("no error code: {}", j))
}

fn state(j: &Json) -> &str {
    j.get("state").and_then(|s| s.as_str()).unwrap_or("")
}

// --- the isError rule ------------------------------------------------------

/// The regression this file exists to prevent.
///
/// MCP has one `isError` flag and the reflex is to raise it whenever the exit
/// code is non-zero. That would report `contradiction` — the verdict meaning
/// *stop, do not pick one* — as a malfunction, and a caller taught that the
/// tool is broken retries or routes around it. Section 14 keeps verdicts and
/// failures in separate ranges; this keeps them on separate sides of the flag.
#[test]
fn every_verdict_is_an_answer_not_an_error() {
    let r = corpus("verdicts");
    for (args, want_state, want_exit) in [
        (r#"{"key":"a.b"}"#, "active", 0),
        (r#"{"key":"c.d"}"#, "undecided", 4),
        (r#"{"key":"gone.key"}"#, "retired", 6),
        (r#"{"key":"no.such.key"}"#, "unknown", 7),
        (r#"{"key":"a.b","scope":"nope"}"#, "unknown", 7),
    ] {
        let reply = &exchange(&r, &[&call("aval_resolve", args)])[0];
        let (payload, is_err) = tool_result(reply);
        assert_eq!(state(payload), want_state, "{}", payload);
        assert_eq!(
            payload.get("exit").and_then(|e| e.as_i64()),
            Some(want_exit),
            "{}",
            payload
        );
        assert!(!is_err, "{} must not be an error: {}", want_state, payload);
    }

    let c = conflicted("verdict-contradiction");
    let reply = &exchange(&c, &[&call("aval_resolve", r#"{"key":"a.b"}"#)])[0];
    let (payload, is_err) = tool_result(reply);
    assert_eq!(state(payload), "contradiction");
    assert_eq!(payload.get("exit").and_then(|e| e.as_i64()), Some(5));
    assert!(!is_err, "a contradiction is an answer: {}", payload);
}

#[test]
fn a_question_that_could_not_be_asked_is_an_error() {
    let r = corpus("errors");
    // A name the corpus does not carry: no question was answered.
    let replies = exchange(&r, &[&call("aval_show", r#"{"record":"ADR-9999"}"#)]);
    let (p, is_err) = tool_result(&replies[0]);
    assert!(is_err, "{}", p);
    assert_eq!(state(p), "unknown");

    let replies = exchange(&r, &[&call("aval_history", r#"{"key":"no.such.key"}"#)]);
    let (p, is_err) = tool_result(&replies[0]);
    assert!(is_err, "{}", p);
    assert_eq!(state(p), "unknown");
}

#[test]
fn an_unreadable_corpus_is_reported_and_does_not_stop_the_server() {
    // A registry mid-edit must not take the surface away: tools/list still
    // answers, and the failure is in the tool result where a caller sees why.
    let r = scratch("broken");
    write(&r, ".adr.yaml", "dir: docs/adr\nscopes: [\nkeys:\n  a.b:\n");

    let replies = exchange(
        &r,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#,
            &call("aval_resolve", r#"{"key":"a.b"}"#),
        ],
    );
    assert_eq!(replies.len(), 2);
    let tools = result(&replies[0]).get("tools").and_then(|t| t.as_arr());
    assert_eq!(
        tools.map(<[Json]>::len),
        Some(5),
        "tools/list must still answer"
    );

    let (payload, is_err) = tool_result(&replies[1]);
    assert!(is_err, "{}", payload);
    assert_eq!(payload.get("exit").and_then(|e| e.as_i64()), Some(3));
}

// --- envelope and dispatch -------------------------------------------------

#[test]
fn an_empty_object_is_invalid_not_a_notification() {
    // `{}` has no id, but it is not a notification — it is an invalid request.
    // Treating it as one leaves a client waiting for a reply that never comes.
    let r = corpus("envelope");
    let replies = exchange(&r, &["{}"]);
    assert_eq!(replies.len(), 1, "an invalid request must be answered");
    assert_eq!(err_code(&replies[0]), -32600);
}

#[test]
fn a_well_formed_notification_draws_no_reply() {
    let r = corpus("notification");
    let replies = exchange(
        &r,
        &[
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            r#"{"jsonrpc":"2.0","method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":9,"method":"ping"}"#,
        ],
    );
    assert_eq!(replies.len(), 1, "only the request with an id is answered");
    assert_eq!(replies[0].get("id").and_then(|i| i.as_i64()), Some(9));
}

#[test]
fn the_envelope_is_validated_before_the_method() {
    let r = corpus("envelope-checks");
    for (msg, want) in [
        (r#"[1,2]"#, -32600),
        (r#"{"id":1,"method":"ping"}"#, -32600),
        (r#"{"jsonrpc":"1.0","id":1,"method":"ping"}"#, -32600),
        (r#"{"jsonrpc":"2.0","id":1,"method":7}"#, -32600),
        (r#"{"jsonrpc":"2.0","id":1,"method":"nope"}"#, -32601),
    ] {
        let replies = exchange(&r, &[msg]);
        assert_eq!(replies.len(), 1, "{}", msg);
        assert_eq!(err_code(&replies[0]), want, "{}", msg);
    }
}

#[test]
fn a_malformed_call_is_a_protocol_error() {
    // An unknown tool or a missing argument cannot be fixed by reading a tool
    // result, so it is -32602 rather than an in-band failure.
    let r = corpus("params");
    for msg in [
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"aval_nope"}}"#,
        r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":7}}"#,
        &call("aval_resolve", "{}"),
        &call("aval_resolve", r#"{"key":7}"#),
        &call("aval_resolve", r#"{"key":""}"#),
        &call("aval_show", "{}"),
    ] {
        let replies = exchange(&r, &[msg]);
        assert_eq!(replies.len(), 1, "{}", msg);
        assert_eq!(err_code(&replies[0]), -32602, "{}", msg);
    }
}

#[test]
fn an_unknown_tool_names_the_ones_that_exist() {
    let r = corpus("unknown-tool");
    let replies = exchange(
        &r,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"resolve"}}"#],
    );
    let m = replies[0]
        .get("error")
        .and_then(|e| e.get("message"))
        .and_then(|m| m.as_str())
        .unwrap_or("");
    assert!(m.contains("aval_resolve"), "{}", m);
}

// --- framing ---------------------------------------------------------------

#[test]
fn a_malformed_line_does_not_kill_the_loop() {
    let r = corpus("parse-error");
    let replies = exchange(
        &r,
        &[
            "not json at all",
            r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        ],
    );
    assert_eq!(replies.len(), 2, "the server must survive a bad line");
    assert_eq!(err_code(&replies[0]), -32700);
    assert!(replies[1].get("result").is_some(), "{}", replies[1]);
}

#[test]
fn two_messages_in_one_write_are_both_answered() {
    // Newline-delimited: two messages may arrive in a single write, and each
    // is its own line. (Two objects on ONE line is not valid framing.)
    let r = corpus("framing-batch");
    let mut s = Server::start(&r);
    s.send_raw(concat!(
        r#"{"jsonrpc":"2.0","id":1,"method":"ping"}"#,
        "\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#,
        "\n"
    ));
    assert_eq!(s.reply().get("id").and_then(|i| i.as_i64()), Some(1));
    assert_eq!(s.reply().get("id").and_then(|i| i.as_i64()), Some(2));
    assert!(s.finish().is_empty());
}

#[test]
fn a_message_split_across_writes_is_answered_once_its_newline_arrives() {
    let r = corpus("framing-split");
    let mut s = Server::start(&r);
    s.send_raw(r#"{"jsonrpc":"2.0","id":"#);
    s.send_raw(r#"42,"method":"pi"#);
    s.send_raw("ng\"}");
    // Still no newline: nothing should have been answered yet.
    thread::sleep(Duration::from_millis(50));
    s.send_raw("\n");
    assert_eq!(s.reply().get("id").and_then(|i| i.as_i64()), Some(42));
    assert!(s.finish().is_empty());
}

/// The failure an EOF-only test cannot see.
///
/// A server that buffers its replies until exit passes every one-shot test and
/// hangs every real client, which holds the pipe open for the whole session.
#[test]
fn a_reply_is_flushed_while_stdin_stays_open() {
    let r = corpus("framing-flush");
    let mut s = Server::start(&r);
    s.send_raw(&format!(
        "{}\n",
        r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#
    ));
    let reply = s.reply(); // blocks; stdin is still open
    assert_eq!(
        result(&reply)
            .get("protocolVersion")
            .and_then(|v| v.as_str()),
        Some("2025-06-18")
    );
    // And the connection is still usable afterwards.
    s.send_raw(&format!(
        "{}\n",
        r#"{"jsonrpc":"2.0","id":2,"method":"ping"}"#
    ));
    assert_eq!(s.reply().get("id").and_then(|i| i.as_i64()), Some(2));
    assert!(s.finish().is_empty());
}

#[test]
fn an_unsupported_protocol_version_gets_ours() {
    let r = corpus("protocol-version");
    let replies = exchange(
        &r,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"1999-01-01"}}"#,
        ],
    );
    let v = result(&replies[0])
        .get("protocolVersion")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    assert!(
        aval::mcp::PROTOCOL_VERSIONS.contains(&v),
        "answered with {}",
        v
    );
}

#[test]
fn a_request_id_survives_the_round_trip() {
    // Beyond 2^53, where f64 rounds. JSON-RPC says the response carries back
    // the same id, and "no client sends one that large" is not a contract.
    let r = corpus("ids");
    let replies = exchange(
        &r,
        &[
            r#"{"jsonrpc":"2.0","id":9007199254740993,"method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":"abc","method":"ping"}"#,
            r#"{"jsonrpc":"2.0","id":null,"method":"ping"}"#,
        ],
    );
    assert_eq!(
        replies[0].get("id").and_then(|i| i.as_i64()),
        Some(9007199254740993)
    );
    assert_eq!(replies[1].get("id").and_then(|i| i.as_str()), Some("abc"));
    assert!(replies[2].get("id").map(Json::is_null).unwrap_or(false));
}

// --- parity with the CLI ---------------------------------------------------

fn cli(dir: &Path, args: &[&str]) -> String {
    let o = Command::new(bin())
        .args(args)
        .current_dir(dir)
        .output()
        .expect("spawn");
    String::from_utf8_lossy(&o.stdout).to_string()
}

fn content(j: &Json, i: usize) -> String {
    result(j)
        .get("content")
        .and_then(|c| c.as_arr())
        .and_then(|a| a.get(i))
        .and_then(|b| b.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or_else(|| panic!("no content[{}]", i))
        .to_string()
}

/// The anti-drift guarantee.
///
/// Both surfaces render from one producer, and this is what says so. If the
/// MCP path ever grows its own copy of the enrichment, these stop matching.
#[test]
fn the_tool_result_matches_the_cli_byte_for_byte() {
    let r = corpus("parity");
    for (args, cli_args) in [
        (r#"{"key":"a.b"}"#, vec!["resolve", "a.b"]),
        (r#"{"key":"c.d"}"#, vec!["resolve", "c.d"]),
        (r#"{"key":"gone.key"}"#, vec!["resolve", "gone.key"]),
        (r#"{"key":"nope"}"#, vec!["resolve", "nope"]),
        (
            r#"{"key":"a.b","scope":"cloud"}"#,
            vec!["resolve", "a.b", "--scope", "cloud"],
        ),
    ] {
        let reply = &exchange(&r, &[&call("aval_resolve", args)])[0];
        let mut json_args = cli_args.clone();
        json_args.push("--json");
        assert_eq!(
            content(reply, 0).trim(),
            cli(&r, &json_args).trim(),
            "json differs for {:?}",
            cli_args
        );
    }
}

#[test]
fn an_inherited_answer_and_a_contradiction_match_the_cli_too() {
    // The two payloads that carry extra work: scope fallback, and per-head
    // provenance. Both are where a second copy of the logic would show up.
    let r = corpus("parity-inherited");
    let reply = &exchange(
        &r,
        &[&call("aval_resolve", r#"{"key":"a.b","scope":"cloud"}"#)],
    )[0];
    let (p, _) = tool_result(reply);
    assert_eq!(p.get("inherited"), Some(&Json::Bool(true)), "{}", p);
    assert_eq!(
        content(reply, 0).trim(),
        cli(&r, &["resolve", "a.b", "--scope", "cloud", "--json"]).trim()
    );

    let c = conflicted("parity-contradiction");
    let reply = &exchange(&c, &[&call("aval_resolve", r#"{"key":"a.b"}"#)])[0];
    assert_eq!(
        content(reply, 0).trim(),
        cli(&c, &["resolve", "a.b", "--json"]).trim()
    );
}

#[test]
fn keys_and_heads_match_the_cli() {
    let r = corpus("parity-keys");
    // Both detail levels have a CLI counterpart, which is why `--names`
    // exists: a tool the CLI cannot answer is a tool that can drift.
    let reply = &exchange(&r, &[&call("aval_keys", "{}")])[0];
    assert_eq!(
        content(reply, 0).trim(),
        cli(&r, &["keys", "--names", "--json"]).trim()
    );

    let reply = &exchange(&r, &[&call("aval_keys", r#"{"detail":"full"}"#)])[0];
    assert_eq!(
        content(reply, 0).trim(),
        cli(&r, &["keys", "--json"]).trim()
    );

    let reply = &exchange(&r, &[&call("aval_heads", "{}")])[0];
    assert_eq!(
        content(reply, 0).trim(),
        cli(&r, &["heads", "--json"]).trim()
    );
}

// --- the surface itself ----------------------------------------------------

#[test]
fn every_tool_is_declared_read_only() {
    let r = corpus("read-only");
    let replies = exchange(&r, &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    let tools = result(&replies[0])
        .get("tools")
        .and_then(|t| t.as_arr())
        .expect("tools");
    assert_eq!(tools.len(), 5);
    for t in tools {
        let name = t.get("name").and_then(|n| n.as_str()).unwrap_or("");
        assert!(name.starts_with("aval_"), "{} needs the prefix", name);
        assert_eq!(
            t.get("annotations").and_then(|a| a.get("readOnlyHint")),
            Some(&Json::Bool(true)),
            "{} is not declared read-only",
            name
        );
        assert!(
            t.get("inputSchema").and_then(|s| s.get("type")).is_some(),
            "{} has no input schema",
            name
        );
    }
}

/// The obligations `SEMANTICS.md` puts on a caller only bind if the caller is
/// told about them, and a tool description is where an agent reads them.
#[test]
fn the_descriptions_carry_the_contract() {
    let r = corpus("descriptions");
    let replies = exchange(&r, &[r#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#]);
    let tools = result(&replies[0])
        .get("tools")
        .and_then(|t| t.as_arr())
        .expect("tools");
    let by = |n: &str| -> String {
        tools
            .iter()
            .find(|t| t.get("name").and_then(|x| x.as_str()) == Some(n))
            .and_then(|t| t.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string()
    };
    let resolve = by("aval_resolve");
    assert!(resolve.contains("STOP"), "contradiction guidance missing");
    assert!(resolve.contains("ADVISORY"), "suggestion guidance missing");
    assert!(by("aval_keys").contains("NOT AUTHORITY"));
    assert!(by("aval_history").contains("NOT AUTHORITY"));
}

#[test]
fn stdout_carries_nothing_but_protocol() {
    // Anything else on stdout corrupts the stream for every client.
    let r = corpus("stdout-purity");
    let replies = exchange(
        &r,
        &[
            "garbage",
            "{}",
            &call("aval_resolve", r#"{"key":"a.b"}"#),
            &call("aval_nope", "{}"),
        ],
    );
    assert_eq!(replies.len(), 4);
    for j in &replies {
        assert_eq!(
            j.get("jsonrpc").and_then(|v| v.as_str()),
            Some("2.0"),
            "{}",
            j
        );
    }
}

#[test]
fn the_corpus_is_reread_for_every_call() {
    // An agent edits records during the session it is asking questions in, so
    // a graph cached at startup would answer with decisions already changed.
    let r = corpus("freshness");
    let mut s = Server::start(&r);
    s.send_raw(&format!("{}\n", call("aval_resolve", r#"{"key":"a.b"}"#)));
    let first = s.reply();
    let (p, _) = tool_result(&first);
    assert_eq!(p.get("choice").and_then(|c| c.as_str()), Some("One"));

    write(
        &r,
        "docs/adr/0001-one.md",
        "---\nid: ADR-0001\nstatus: accepted\ndecisions:\n  \
         - key: a.b\n    choice: Rewritten\n    first: true\n---\n# one\n",
    );

    s.send_raw(&format!("{}\n", call("aval_resolve", r#"{"key":"a.b"}"#)));
    let second = s.reply();
    let (p, _) = tool_result(&second);
    assert_eq!(
        p.get("choice").and_then(|c| c.as_str()),
        Some("Rewritten"),
        "the second answer came from a cache"
    );
    assert!(s.finish().is_empty());
}

// --- context economics -----------------------------------------------------

/// A tool result carries the payload ONCE as text, plus `structuredContent`.
///
/// An earlier shape added the human rendering as a second text block. A model
/// already has every field from the first one, and on the fleet's largest
/// corpus that duplicate was 27% of the bytes.
#[test]
fn a_result_carries_one_text_block() {
    let r = corpus("one-block");
    let replies = exchange(&r, &[&call("aval_resolve", r#"{"key":"a.b"}"#)]);
    let content = result(&replies[0])
        .get("content")
        .and_then(|c| c.as_arr())
        .expect("content");
    assert_eq!(
        content.len(),
        1,
        "expected one text block, got {}",
        content.len()
    );
    // And it is the complete payload, not a summary of it.
    let text = content[0]
        .get("text")
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(text.contains(r#""exit":0"#), "{}", text);
}

/// Discovery defaults to what a caller asking "which keys exist" needs.
#[test]
fn keys_defaults_to_names_and_grows_on_request() {
    let r = corpus("keys-detail");
    let names = &exchange(&r, &[&call("aval_keys", "{}")])[0];
    let (p, _) = tool_result(names);
    let first = p.get("keys").and_then(|k| k.as_arr()).expect("keys");
    assert!(!first.is_empty());
    assert!(first[0].get("key").is_some());
    assert!(
        first[0].get("decided").is_none(),
        "names detail must omit `decided`: {}",
        p
    );

    let full = &exchange(&r, &[&call("aval_keys", r#"{"detail":"full"}"#)])[0];
    let (p, _) = tool_result(full);
    let keys = p.get("keys").and_then(|k| k.as_arr()).expect("keys");
    assert!(
        keys[0].get("decided").is_some(),
        "full detail must carry it: {}",
        p
    );

    // And an unrecognised level is a protocol error, not a silent default.
    let bad = exchange(&r, &[&call("aval_keys", r#"{"detail":"everything"}"#)]);
    assert_eq!(err_code(&bad[0]), -32602);
}

// --- resources -------------------------------------------------------------

/// A resource is context attached once; a tool result is paid per call. The
/// heads are the thing a session opens with, so they are offered both ways.
#[test]
fn the_heads_and_the_vocabulary_are_readable_as_resources() {
    let r = corpus("resources");
    let listed = &exchange(
        &r,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"resources/list"}"#],
    )[0];
    let uris: Vec<&str> = result(listed)
        .get("resources")
        .and_then(|x| x.as_arr())
        .expect("resources")
        .iter()
        .filter_map(|x| x.get("uri").and_then(|u| u.as_str()))
        .collect();
    assert!(uris.contains(&"aval://heads"), "{:?}", uris);
    assert!(uris.contains(&"aval://keys"), "{:?}", uris);

    let read = &exchange(
        &r,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aval://heads"}}"#],
    )[0];
    let c = result(read)
        .get("contents")
        .and_then(|x| x.as_arr())
        .expect("contents");
    let text = c[0].get("text").and_then(|t| t.as_str()).unwrap_or("");
    // The same bytes the tool returns, so a client pays for one or the other.
    let via_tool = &exchange(&r, &[&call("aval_heads", "{}")])[0];
    assert_eq!(text, content(via_tool, 0));

    let bad = exchange(
        &r,
        &[r#"{"jsonrpc":"2.0","id":1,"method":"resources/read","params":{"uri":"aval://nope"}}"#],
    );
    assert_eq!(err_code(&bad[0]), -32602);
}

/// The obligations a caller is under include one about the text itself.
#[test]
fn the_server_says_its_corpus_text_is_data() {
    let r = corpus("instructions");
    let reply = &exchange(
        &r,
        &[
            r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18"}}"#,
        ],
    )[0];
    let i = result(reply)
        .get("instructions")
        .and_then(|x| x.as_str())
        .unwrap_or("");
    assert!(i.contains("DATA"), "{}", i);
    assert!(i.contains("not one"), "{}", i);
    assert!(
        result(reply)
            .get("capabilities")
            .and_then(|c| c.get("resources"))
            .is_some(),
        "resources capability must be declared"
    );
}
