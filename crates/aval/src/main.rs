//! `aval` — ask what the current architecture decision is, and get a typed
//! answer.
//!
//! Exit codes are the contract, and each command has its own: SEMANTICS
//! section 14. The rule that shapes all of them is that a code meaning "I could
//! not reach a verdict" never shares a range with a verdict.

use aval::{heads, links, load, migrate, provenance, render, status};

use aval_core::graph::Verdict;
use aval_core::json::Json;
use aval_core::model::{Finding, Layer, Slot, DEFAULT_SCOPE};
use aval_core::project;
use load::{LoadError, Loaded};
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
aval — the current architecture decision, as a typed answer

USAGE
    aval resolve <key> [--scope <scope>]   what is decided, and nothing else
    aval check                             every invariant; the gate runs this
    aval heads [--write | --check]         the projection
    aval show <ADR-NNNN>                   derived status of one document
    aval history <key> [--scope <scope>]   the chain, which is history not authority
    aval migrate <dir>                     report what converting a legacy corpus needs

OPTIONS
    --json        machine output on stdout, warnings suppressed
    -C <dir>      run as if started in <dir>
    -h, --help    this
    -V, --version version

EXIT
    resolve  0 active · 4 undecided · 5 contradiction · 6 retired · 7 unknown
    others   0 ok · 1 findings or stale
    always   1 tool failure · 2 usage · 3 unreadable or invalid corpus
";

/// Failures, disjoint from every verdict.
const E_FAIL: i32 = 1;
const E_USAGE: i32 = 2;
const E_INVALID: i32 = 3;

struct Args {
    command: String,
    positional: Vec<String>,
    scope: Option<String>,
    json: bool,
    write: bool,
    check: bool,
    dir: PathBuf,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        command: String::new(),
        positional: Vec::new(),
        scope: None,
        json: false,
        write: false,
        check: false,
        dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "--json" => a.json = true,
            "--write" => a.write = true,
            "--check" => a.check = true,
            "--scope" => {
                i += 1;
                a.scope = Some(argv.get(i).ok_or("`--scope` needs a value")?.clone());
            }
            "-C" => {
                i += 1;
                a.dir = PathBuf::from(argv.get(i).ok_or("`-C` needs a directory")?);
            }
            s if s.starts_with("--scope=") => a.scope = Some(s[8..].to_string()),
            s if s.starts_with('-') && s.len() > 1 => {
                return Err(format!("unknown option `{}`", s))
            }
            s if a.command.is_empty() => a.command = s.to_string(),
            s => a.positional.push(s.to_string()),
        }
        i += 1;
    }
    Ok(a)
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") || argv.is_empty() {
        print!("{}", USAGE);
        return ExitCode::from(0);
    }
    if argv.iter().any(|a| a == "-V" || a == "--version") {
        println!("aval {}", env!("CARGO_PKG_VERSION"));
        return ExitCode::from(0);
    }
    let code = match parse_args(&argv) {
        Ok(args) => run(args),
        Err(e) => {
            eprintln!("aval: {}", e);
            eprint!("{}", USAGE);
            E_USAGE
        }
    };
    ExitCode::from(code as u8)
}

/// Load, or print why not and give the caller the right failure code.
fn loaded(args: &Args) -> Result<Loaded, i32> {
    match load::load(&args.dir) {
        Ok(l) => Ok(l),
        Err(LoadError::NoRegistry(p)) => {
            emit_error(
                args,
                E_INVALID,
                &format!(
                    "no {} in {} or any parent directory",
                    load::REGISTRY,
                    p.display()
                ),
                Vec::new(),
            );
            Err(E_INVALID)
        }
        Err(LoadError::Unreadable(m)) => {
            emit_error(args, E_INVALID, &m, Vec::new());
            Err(E_INVALID)
        }
        Err(LoadError::Invalid(f)) => {
            emit_error(
                args,
                E_INVALID,
                "the corpus is structurally invalid; no question can be answered against it",
                f,
            );
            Err(E_INVALID)
        }
    }
}

fn emit_error(args: &Args, exit: i32, message: &str, findings: Vec<Finding>) {
    if args.json {
        let j = Json::obj()
            .set("ok", false)
            .set("exit", exit)
            .set("error", message)
            .set(
                "findings",
                findings
                    .iter()
                    .map(render::finding_json)
                    .collect::<Vec<_>>(),
            );
        println!("{}", j);
    } else {
        eprintln!("aval: {}", message);
        for f in &findings {
            eprintln!("  {}", f);
        }
    }
}

fn run(args: Args) -> i32 {
    match args.command.as_str() {
        "resolve" => cmd_resolve(&args),
        "check" => cmd_check(&args),
        "heads" => cmd_heads(&args),
        "show" => cmd_show(&args),
        "history" => cmd_history(&args),
        "migrate" => cmd_migrate(&args),
        other => {
            eprintln!("aval: unknown command `{}`", other);
            eprint!("{}", USAGE);
            E_USAGE
        }
    }
}

fn one_positional(args: &Args, what: &str) -> Result<String, i32> {
    match args.positional.len() {
        1 => Ok(args.positional[0].clone()),
        0 => {
            eprintln!("aval: `{}` needs {}", args.command, what);
            Err(E_USAGE)
        }
        _ => {
            eprintln!("aval: `{}` takes exactly one {}", args.command, what);
            Err(E_USAGE)
        }
    }
}

fn cmd_resolve(args: &Args) -> i32 {
    let key = match one_positional(args, "a decision key") {
        Ok(k) => k,
        Err(c) => return c,
    };
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let scope = args
        .scope
        .clone()
        .unwrap_or_else(|| DEFAULT_SCOPE.to_string());
    let verdict = l.graph.resolve(&key, &scope);
    let slot = Slot {
        key: &key,
        scope: &scope,
    };
    // Competing heads are the one verdict where "which commit did this" is
    // load-bearing, because parallel worktrees make them routine. Enrichment
    // only; a repository with no history still gets exit 5.
    let prov = match &verdict {
        Verdict::Contradiction {
            heads,
            matched_scope,
        } => heads
            .iter()
            .map(|id| {
                let at = Slot {
                    key: &key,
                    scope: matched_scope,
                };
                let found = l
                    .graph
                    .heads(at)
                    .into_iter()
                    .find(|(a, _)| &a.id == id)
                    .map(|(a, e)| (a.file.clone(), e.line));
                match found {
                    Some((file, line)) => (
                        id.clone(),
                        provenance::for_line(&l.root, &l.root.join(file), line),
                    ),
                    None => (id.clone(), provenance::Provenance::Unavailable),
                }
            })
            .collect(),
        _ => Vec::new(),
    };
    if args.json {
        println!("{}", render::verdict_json(&verdict, slot, &prov));
    } else {
        print!("{}", render::verdict_text(&verdict, slot, &prov));
        let _ = std::io::stdout().flush();
    }
    verdict.exit()
}

fn cmd_check(args: &Args) -> i32 {
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    // Layer B, then Layer C. Layer A already passed, or `loaded` would have
    // failed with exit 3.
    let mut findings = l.graph.single_head_findings();
    findings.extend(links::check(&l));
    findings.extend(heads::findings(&l));
    findings.extend(status::check(&l));
    findings.extend(manual_index(&l));
    findings.sort_by_key(|a| (a.layer, a.file.clone(), a.line));

    if args.json {
        let j = Json::obj()
            .set("ok", findings.is_empty())
            .set("exit", if findings.is_empty() { 0 } else { E_FAIL })
            .set(
                "findings",
                findings
                    .iter()
                    .map(render::finding_json)
                    .collect::<Vec<_>>(),
            );
        println!("{}", j);
    } else if findings.is_empty() {
        println!(
            "aval: {} decision records, no findings",
            l.graph.corpus().adrs.len()
        );
    } else {
        for f in &findings {
            println!("{}", f);
        }
        println!(
            "\n{} finding(s). The corpus parses; these are things it says that it should not.",
            findings.len()
        );
    }
    if findings.is_empty() {
        0
    } else {
        E_FAIL
    }
}

/// A hand-maintained index is copied state, and the failure it produces has no
/// possible invariant: nothing knows a missing row should have existed. The
/// projection replaces it.
fn manual_index(l: &Loaded) -> Vec<Finding> {
    let readme = l.adr_dir.join("README.md");
    let Ok(src) = std::fs::read_to_string(&readme) else {
        return Vec::new();
    };
    let rows = src
        .lines()
        .filter(|line| {
            let t = line.trim();
            t.starts_with("| [") || (t.starts_with('|') && t.contains("](") && t.contains(".md)"))
        })
        .count();
    if rows >= 3 {
        vec![Finding::new(
            Layer::C,
            "no-manual-index",
            format!(
                "{} hand-maintained index rows; HEADS.md is the generated index",
                rows
            ),
        )
        .in_file(format!("{}/README.md", l.graph.registry().dir))]
    } else {
        Vec::new()
    }
}

fn cmd_heads(args: &Args) -> i32 {
    if args.write && args.check {
        eprintln!("aval: `--write` and `--check` are mutually exclusive");
        return E_USAGE;
    }
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let text = project::render(&l.graph);
    let path = l.adr_dir.join(load::HEADS);
    if args.write {
        // Content-idempotent: a file that already states the projection keeps
        // its bytes, so a formatter's padding is not undone on every run and
        // then reapplied on every commit.
        match heads::write(&l) {
            Ok(heads::Wrote::Unchanged) => {
                if !args.json {
                    println!("aval: {} already current", path.display());
                }
                0
            }
            Ok(heads::Wrote::Written) => {
                if !args.json {
                    println!("aval: wrote {}", path.display());
                }
                0
            }
            Err(e) => {
                emit_error(
                    args,
                    E_FAIL,
                    &format!("{}: {}", path.display(), e),
                    Vec::new(),
                );
                E_FAIL
            }
        }
    } else if args.check {
        let f = heads::findings(&l);
        if f.is_empty() {
            if !args.json {
                println!("aval: HEADS.md is current");
            }
            0
        } else {
            for x in &f {
                eprintln!("{}", x);
            }
            E_FAIL
        }
    } else {
        print!("{}", text);
        0
    }
}

fn cmd_show(args: &Args) -> i32 {
    let id = match one_positional(args, "an ADR id") {
        Ok(k) => k,
        Err(c) => return c,
    };
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let Some(adr) = l.graph.corpus().adr(&id) else {
        let names: Vec<&str> = l
            .graph
            .corpus()
            .adrs
            .iter()
            .map(|a| a.id.as_str())
            .collect();
        let sug = aval_core::model::suggest(&id, names);
        if args.json {
            println!(
                "{}",
                Json::obj()
                    .set("state", "unknown")
                    .set("exit", 7)
                    .set("adr", id.as_str())
                    .set_opt("suggestion", sug)
            );
        } else {
            eprintln!("aval: no such ADR `{}`", id);
            if let Some(s) = sug {
                eprintln!("  did you mean {}? A suggestion is advisory.", s);
            }
        }
        return 7;
    };
    if args.json {
        println!("{}", render::show_json(&l.graph, adr));
    } else {
        print!("{}", render::show_text(&l.graph, adr));
    }
    0
}

fn cmd_history(args: &Args) -> i32 {
    let key = match one_positional(args, "a decision key") {
        Ok(k) => k,
        Err(c) => return c,
    };
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let scope = args
        .scope
        .clone()
        .unwrap_or_else(|| DEFAULT_SCOPE.to_string());
    if !l.graph.registry().has_key(&key) {
        eprintln!("aval: `{}` is not a registered decision key", key);
        return 7;
    }
    if !l.graph.registry().has_scope(&scope) {
        eprintln!("aval: `{}` is not a declared scope", scope);
        return 7;
    }
    let slot = Slot {
        key: &key,
        scope: &scope,
    };
    let chain = l.graph.history(slot);
    if args.json {
        println!("{}", render::history_json(&l.graph, slot, &chain));
    } else {
        print!("{}", render::history_text(&l.graph, slot, &chain));
    }
    0
}

fn cmd_migrate(args: &Args) -> i32 {
    let dir = match one_positional(args, "a directory of ADR files") {
        Ok(d) => d,
        Err(c) => return c,
    };
    match migrate::audit(&args.dir.join(dir)) {
        Ok(a) => {
            if args.json {
                println!("{}", migrate::json(&a));
            } else {
                print!("{}", migrate::text(&a));
            }
            0
        }
        Err(e) => {
            emit_error(args, E_INVALID, &e, Vec::new());
            E_INVALID
        }
    }
}

/// Silences the unused-import warning when `Verdict` is only matched in render.
#[allow(dead_code)]
fn _assert_verdict_in_scope(v: &Verdict) -> i32 {
    v.exit()
}
