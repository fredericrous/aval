//! `aval mcp` — the decision corpus as MCP tools.
//!
//! An agent could already run `aval resolve`; the session hook prints the
//! commands. What this adds is **discoverable** access: tools a client lists
//! without being told they exist, arguments checked as a schema rather than
//! assembled into a command line, results as data rather than parsed back out
//! of stdout — and tool descriptions that carry the obligations `SEMANTICS.md`
//! places on a caller, in front of the model at the moment it calls.
//!
//! Three rules shape everything here.
//!
//! **A verdict is not an error.** Section 14: a code meaning "I could not reach
//! a verdict" never shares a range with a verdict. MCP offers one `isError`
//! flag and the reflex is to raise it whenever the exit code is non-zero, which
//! would report `contradiction` — the verdict that means *stop, do not pick
//! one* — as a malfunction, and teach a caller to retry or route around it. All
//! five verdicts are `isError: false`.
//!
//! **The corpus is the working tree, read fresh for every call.** An agent
//! edits records during the session it is asking questions in, so a graph
//! cached at startup would answer with decisions that session already changed.
//!
//! **Startup reads nothing.** A registry mid-edit must not take the surface
//! away; a corpus that will not load is reported in the tool result, where the
//! caller can see why.

use crate::load;
use crate::render;
use aval_core::json::Json;
use aval_core::model::DEFAULT_SCOPE;
use std::io::{BufRead, Write};
use std::path::Path;

/// Newest first. `initialize` echoes the client's version when we speak it.
pub const PROTOCOL_VERSIONS: &[&str] = &["2025-06-18", "2025-03-26", "2024-11-05"];

const PARSE_ERROR: i32 = -32700;
const INVALID_REQUEST: i32 = -32600;
const METHOD_NOT_FOUND: i32 = -32601;
const INVALID_PARAMS: i32 = -32602;

const INSTRUCTIONS: &str = "\
aval answers what is currently decided, from a corpus of architecture decision
records. It is READ-ONLY: nothing here writes a decision, because deciding is
not a thing to do on a caller's behalf.

Resolve before writing code a decision governs, and before proposing an
alternative to one. A verdict of `contradiction` means the corpus disagrees
with itself: stop and raise it, do not pick a side. A `suggestion` is advisory
and resolves nothing — never substitute it and call again.";

// --------------------------------------------------------------- transport

/// Serve until stdin closes.
///
/// MCP's stdio transport is newline-delimited JSON: one message per line.
/// Every response is flushed as it is written — a server that buffered until
/// exit would look alive and answer nothing.
pub fn serve(root: &Path) -> i32 {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("aval mcp: reading stdin: {}", e);
                return 1;
            }
        };
        if line.trim().is_empty() {
            continue;
        }
        // A notification earns no reply at all, which is what `None` means.
        if let Some(reply) = handle(root, &line) {
            if writeln!(out, "{}", reply).is_err() || out.flush().is_err() {
                eprintln!("aval mcp: stdout closed");
                return 1;
            }
        }
    }
    0
}

/// One message in, at most one reply out.
///
/// Separated from the loop so the tests drive the protocol without a process.
pub fn handle(root: &Path, line: &str) -> Option<Json> {
    let msg = match aval_core::json::parse(line) {
        Ok(m) => m,
        Err(e) => return Some(error(Json::Null, PARSE_ERROR, &e)),
    };
    if !matches!(msg, Json::Obj(_)) {
        return Some(error(
            Json::Null,
            INVALID_REQUEST,
            "a request must be a JSON object",
        ));
    }
    if msg.get("jsonrpc").and_then(|j| j.as_str()) != Some("2.0") {
        return Some(error(
            msg.get("id").cloned().unwrap_or(Json::Null),
            INVALID_REQUEST,
            "`jsonrpc` must be \"2.0\"",
        ));
    }
    let Some(method) = msg.get("method").and_then(|j| j.as_str()) else {
        return Some(error(
            msg.get("id").cloned().unwrap_or(Json::Null),
            INVALID_REQUEST,
            "`method` must be a string",
        ));
    };

    // Only a WELL-FORMED request without an id is a notification. `{}` is not
    // one merely for lacking an id — it is invalid, and was answered above.
    // Getting this backwards leaves a client waiting forever for a reply that
    // was never going to come.
    let id = msg.get("id").cloned()?;

    Some(match method {
        "initialize" => result(id, initialize(&msg)),
        "notifications/initialized" => result(id, Json::obj()),
        "ping" => result(id, Json::obj()),
        "tools/list" => result(id, Json::obj().set("tools", tools())),
        "tools/call" => tools_call(root, id, &msg),
        other => error(id, METHOD_NOT_FOUND, &format!("no method `{}`", other)),
    })
}

fn initialize(msg: &Json) -> Json {
    let asked = msg
        .get("params")
        .and_then(|p| p.get("protocolVersion"))
        .and_then(|v| v.as_str());
    let version = match asked {
        Some(v) if PROTOCOL_VERSIONS.contains(&v) => v,
        // Answer with ours and let the client decide whether it can proceed.
        _ => PROTOCOL_VERSIONS[0],
    };
    Json::obj()
        .set("protocolVersion", version)
        .set("capabilities", Json::obj().set("tools", Json::obj()))
        .set(
            "serverInfo",
            Json::obj()
                .set("name", "aval")
                .set("version", env!("CARGO_PKG_VERSION")),
        )
        .set("instructions", INSTRUCTIONS)
}

fn result(id: Json, r: Json) -> Json {
    Json::obj()
        .set("jsonrpc", "2.0")
        .set("id", id)
        .set("result", r)
}

fn error(id: Json, code: i32, message: &str) -> Json {
    Json::obj().set("jsonrpc", "2.0").set("id", id).set(
        "error",
        Json::obj().set("code", code).set("message", message),
    )
}

// ------------------------------------------------------------------- tools

fn schema(props: Vec<(&str, Json)>, required: Vec<&str>) -> Json {
    let mut o = Json::obj();
    for (k, v) in props {
        o = o.set(k, v);
    }
    Json::obj()
        .set("type", "object")
        .set("properties", o)
        .set("required", required)
}

fn prop(kind: &str, description: &str) -> Json {
    Json::obj()
        .set("type", kind)
        .set("description", description)
}

fn tool(name: &str, description: &str, input: Json) -> Json {
    Json::obj()
        .set("name", name)
        .set("description", description)
        .set("inputSchema", input)
        // Read-only is declared, not merely true: there is no write handler to
        // call, and a client that surfaces the hint can say so to a person.
        .set(
            "annotations",
            Json::obj()
                .set("readOnlyHint", true)
                .set("destructiveHint", false),
        )
}

const KEY_ARG: &str = "The exact decision key, e.g. `stack.sql-layer`. Exact \
     only — there is no matching by similarity. Use aval_keys to find one.";
const SCOPE_ARG: &str = "The scope to ask at, e.g. `homelab`. Omit for the \
     default scope `*`. An undeclared scope is rejected, never absorbed into \
     the default.";

fn tools() -> Vec<Json> {
    vec![
        tool(
            "aval_resolve",
            "What is decided NOW for a decision key, at a scope. The authoritative \
             answer: call it before writing code a decision governs, and before \
             proposing an alternative to one.\n\n\
             Returns a TYPED verdict in `state`. `active` is a decision. \
             `undecided` (the key exists, nothing has decided it), `retired` (a \
             record deliberately retired it) and `unknown` (no such key or scope, \
             or the key is not decided on that axis) are ANSWERS, not failures — \
             none is a reason to retry or to guess. `contradiction` means two \
             records compete: STOP, do not pick one, and say so; the corpus \
             disagrees with itself and a person has to resolve it.\n\n\
             A `suggestion` on `unknown` is ADVISORY. Do not substitute it and \
             call again — ask, or call aval_keys. `inherited: true` means the \
             answer came from the default scope rather than the one you asked \
             about. `pack` names the repository a vendored decision came from; \
             change it there, not here.",
            schema(
                vec![("key", prop("string", KEY_ARG)), ("scope", prop("string", SCOPE_ARG))],
                vec!["key"],
            ),
        ),
        tool(
            "aval_keys",
            "Every decision key this repository can answer, with its description, \
             the scopes it is answerable at, and where it is already decided.\n\n\
             DISCOVERY, NOT AUTHORITY. Use it to find an exact key name, then call \
             aval_resolve for the answer. There is deliberately no search by \
             similarity: only an exact key resolves.\n\n\
             `scopes: null` means the key is answerable at every declared scope; \
             `scopes: []` means fleet-wide only. An empty `decided` list means the \
             key is declared and undecided — a real state, and often deliberate.",
            schema(vec![], vec![]),
        ),
        tool(
            "aval_heads",
            "Every slot that has a decision, with its state — the structured form \
             of HEADS.md, for orienting at the start of a task.\n\n\
             A SUPERSET of that file: a slot whose records compete appears here as \
             `contradiction`, where the markdown projection omits it entirely and \
             so reads as though nothing were decided. For one authoritative answer, \
             call aval_resolve.",
            schema(vec![], vec![]),
        ),
        tool(
            "aval_show",
            "One decision record, and whether it still holds.\n\n\
             `derived_status` is computed from supersession edges, never read from \
             a status line in the document: `active`, `superseded`, \
             `partially-superseded`, `draft` or `empty`. Each entry says whether it \
             is still the head for its slot. Accepts a vendored id such as \
             `decisions:ADR-0002`.",
            schema(
                vec![(
                    "record",
                    prop(
                        "string",
                        "The record id, e.g. `ADR-0002`, or `decisions:ADR-0002` for a vendored one.",
                    ),
                )],
                vec!["record"],
            ),
        ),
        tool(
            "aval_history",
            "How a slot reached its current answer, oldest first.\n\n\
             HISTORY, NOT AUTHORITY. A record in this chain that is not the head \
             has been replaced, and citing it as current is exactly the mistake \
             this corpus exists to prevent. For what holds now, call aval_resolve.",
            schema(
                vec![("key", prop("string", KEY_ARG)), ("scope", prop("string", SCOPE_ARG))],
                vec!["key"],
            ),
        ),
    ]
}

// -------------------------------------------------------------------- call

/// A tool's answer: the payload, and whether it reports a failure.
///
/// The split that matters: a **verdict** is never a failure, however non-zero
/// its exit code. `isError` is for a corpus that would not load or a name the
/// corpus does not carry — cases where no question was answered at all.
struct Out {
    payload: Json,
    is_error: bool,
    text: Option<String>,
}

impl Out {
    fn ok(payload: Json, text: Option<String>) -> Out {
        Out {
            payload,
            is_error: false,
            text,
        }
    }
    fn failed(payload: Json) -> Out {
        Out {
            payload,
            is_error: true,
            text: None,
        }
    }
}

fn tools_call(root: &Path, id: Json, msg: &Json) -> Json {
    let params = msg.get("params");
    let Some(name) = params.and_then(|p| p.get("name")).and_then(|n| n.as_str()) else {
        return error(id, INVALID_PARAMS, "`params.name` must be a string");
    };
    let args = params.and_then(|p| p.get("arguments"));

    // A missing or mistyped ARGUMENT is a protocol error: the call was
    // malformed, and no reading of a tool result would help a client fix it.
    // A well-formed argument naming something the corpus does not carry is a
    // tool result instead — that is an answer, and it carries the suggestion.
    let out = match name {
        "aval_resolve" => match req_str(args, "key") {
            Ok(key) => call_resolve(root, key, opt_str(args, "scope")),
            Err(e) => return error(id, INVALID_PARAMS, &e),
        },
        "aval_keys" => call_keys(root),
        "aval_heads" => call_heads(root),
        "aval_show" => match req_str(args, "record") {
            Ok(r) => call_show(root, r),
            Err(e) => return error(id, INVALID_PARAMS, &e),
        },
        "aval_history" => match req_str(args, "key") {
            Ok(key) => call_history(root, key, opt_str(args, "scope")),
            Err(e) => return error(id, INVALID_PARAMS, &e),
        },
        other => {
            let known: Vec<String> = tools()
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(str::to_string))
                .collect();
            return error(
                id,
                INVALID_PARAMS,
                &format!("no tool `{}`; this server has {}", other, known.join(", ")),
            );
        }
    };

    let mut content = vec![text_block(&out.payload.to_string())];
    if let Some(t) = &out.text {
        content.push(text_block(t));
    }
    result(
        id,
        Json::obj()
            .set("content", content)
            .set("structuredContent", out.payload)
            .set("isError", out.is_error),
    )
}

/// The complete payload goes in a text block too, not only in
/// `structuredContent`: a client that reads only text must still see every
/// field, the numeric `exit` included.
fn text_block(s: &str) -> Json {
    Json::obj().set("type", "text").set("text", s)
}

fn req_str<'a>(args: Option<&'a Json>, name: &str) -> Result<&'a str, String> {
    match args.and_then(|a| a.get(name)) {
        Some(Json::Str(s)) if !s.is_empty() => Ok(s),
        Some(Json::Str(_)) => Err(format!("`{}` must not be empty", name)),
        Some(_) => Err(format!("`{}` must be a string", name)),
        None => Err(format!("`{}` is required", name)),
    }
}

fn opt_str<'a>(args: Option<&'a Json>, name: &str) -> Option<&'a str> {
    args.and_then(|a| a.get(name)).and_then(|v| v.as_str())
}

/// Load the corpus, or describe why not in the shape the CLI uses for it.
///
/// `LoadError` renders itself, so this surface and the CLI cannot disagree
/// about what a load failure says.
fn corpus(root: &Path) -> Result<load::Loaded, Out> {
    load::load(root).map_err(|e| Out::failed(render::error_json(3, &e.to_string(), e.findings())))
}

fn call_resolve(root: &Path, key: &str, scope: Option<&str>) -> Out {
    let l = match corpus(root) {
        Ok(l) => l,
        Err(o) => return o,
    };
    let scope = scope.unwrap_or(DEFAULT_SCOPE);
    let a = render::answer(&l.graph, &l.root, key, scope);
    // Every verdict, including `contradiction` and `unknown`, is an answer.
    Out::ok(a.json(), Some(a.text()))
}

fn call_keys(root: &Path) -> Out {
    match corpus(root) {
        Ok(l) => Out::ok(
            render::keys_json(&l.graph, &l.packs),
            Some(render::keys_text(&l.graph, &l.packs)),
        ),
        Err(o) => o,
    }
}

fn call_heads(root: &Path) -> Out {
    match corpus(root) {
        Ok(l) => {
            let j = render::heads_json(&l.graph);
            let t = aval_core::project::render(&l.graph);
            Out::ok(j, Some(t))
        }
        Err(o) => o,
    }
}

fn call_show(root: &Path, id: &str) -> Out {
    let l = match corpus(root) {
        Ok(l) => l,
        Err(o) => return o,
    };
    match l.graph.corpus().adr(id) {
        Some(adr) => Out::ok(
            render::show_json(&l.graph, adr),
            Some(render::show_text(&l.graph, adr)),
        ),
        None => {
            let names: Vec<&str> = l
                .graph
                .corpus()
                .adrs
                .iter()
                .map(|a| a.id.as_str())
                .collect();
            let sug = aval_core::model::suggest(id, names).map(str::to_string);
            Out::failed(render::show_unknown_json(id, sug))
        }
    }
}

fn call_history(root: &Path, key: &str, scope: Option<&str>) -> Out {
    let l = match corpus(root) {
        Ok(l) => l,
        Err(o) => return o,
    };
    let scope = scope.unwrap_or(DEFAULT_SCOPE);
    let reg = l.graph.registry();
    if !reg.has_key(key) {
        let names: Vec<&str> = reg.keys.iter().map(|k| k.name.as_str()).collect();
        let sug = aval_core::model::suggest(key, names).map(str::to_string);
        return Out::failed(render::history_unknown_json("key", key, scope, sug));
    }
    if !reg.has_scope(scope) {
        let names: Vec<&str> = reg.scopes.iter().map(|s| s.as_str()).collect();
        let sug = aval_core::model::suggest(scope, names).map(str::to_string);
        return Out::failed(render::history_unknown_json("scope", key, scope, sug));
    }
    let slot = aval_core::model::Slot { key, scope };
    let chain = l.graph.history(slot);
    Out::ok(
        render::history_json(&l.graph, slot, &chain),
        Some(render::history_text(&l.graph, slot, &chain)),
    )
}
