//! `aval` — ask what the current architecture decision is, and get a typed
//! answer.
//!
//! Exit codes are the contract, and each command has its own: SEMANTICS
//! section 14. The rule that shapes all of them is that a code meaning "I could
//! not reach a verdict" never shares a range with a verdict.

use aval::{add, heads, hook, links, load, migrate, packfile, render, status};

use aval_core::graph::Verdict;
use aval_core::json::Json;
use aval_core::model::{Finding, Layer, Slot, DEFAULT_SCOPE};
use aval_core::project;
use load::Loaded;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "\
aval — the current architecture decision, as a typed answer

USAGE
    aval resolve <key> [--scope <scope>]   what is decided, and nothing else
    aval keys                              the vocabulary: every key, where it
                                           is answerable, where it is decided
    aval check                             every invariant; the gate runs this
    aval heads [--write | --check]         the projection
    aval show <ADR-NNNN>                   derived status of one document
    aval history <key> [--scope <scope>]   the chain, which is history not authority
    aval migrate <dir>                     report what converting a legacy corpus needs
    aval hook install [--check]            put the heads in front of an agent
    aval pack [--write | --check]          this corpus's declarations, for others to read
    aval add <source>… [--dry-run]         vendor another repository's declarations
    aval add --check                       are the vendored packs still current
    aval mcp                               serve the corpus as MCP tools on
                                           stdio, read-only, until stdin closes

SOURCES
    github:owner/repo   forgejo:host/owner/repo   <git-url>   <path>
    each optionally @<rev>; the commit id is what gets recorded

OPTIONS
    --json        machine output on stdout, warnings suppressed
    --as <name>   name one vendored pack (add only); it becomes the id prefix
    -C <dir>      run as if started in <dir>
    -h, --help    this
    -V, --version version

EXIT
    resolve  0 active · 4 undecided · 5 contradiction · 6 retired · 7 unknown
    others   0 ok · 1 findings or stale
    always   1 tool failure · 2 usage · 3 unreadable or invalid corpus
    mcp      0 stdin closed · 1 transport failure · 2 usage. Never 3: a
             corpus that will not load is reported in the tool result.
";

/// Failures, disjoint from every verdict.
const E_FAIL: i32 = 1;
const E_USAGE: i32 = 2;
const E_INVALID: i32 = 3;

struct Args {
    command: String,
    positional: Vec<String>,
    scope: Option<String>,
    as_name: Option<String>,
    json: bool,
    write: bool,
    check: bool,
    dry_run: bool,
    dir: PathBuf,
}

fn parse_args(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        command: String::new(),
        positional: Vec::new(),
        scope: None,
        as_name: None,
        json: false,
        write: false,
        check: false,
        dry_run: false,
        dir: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
    };
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        match arg {
            "--json" => a.json = true,
            "--write" => a.write = true,
            "--check" => a.check = true,
            "--dry-run" => a.dry_run = true,
            "--scope" => {
                i += 1;
                a.scope = Some(argv.get(i).ok_or("`--scope` needs a value")?.clone());
            }
            "--as" => {
                i += 1;
                a.as_name = Some(argv.get(i).ok_or("`--as` needs a name")?.clone());
            }
            s if s.starts_with("--as=") => a.as_name = Some(s[5..].to_string()),
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
        Ok(args) => run(&args),
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
    load::load(&args.dir).map_err(|e| {
        emit_error(args, E_INVALID, &e.to_string(), e.findings());
        E_INVALID
    })
}

fn emit_error(args: &Args, exit: i32, message: &str, findings: &[Finding]) {
    if args.json {
        println!("{}", render::error_json(exit, message, findings));
    } else {
        eprintln!("aval: {}", message);
        for f in findings {
            eprintln!("  {}", f);
        }
    }
}

fn run(args: &Args) -> i32 {
    match args.command.as_str() {
        "resolve" => cmd_resolve(args),
        "keys" => cmd_keys(args),
        "mcp" => cmd_mcp(args),
        "check" => cmd_check(args),
        "heads" => cmd_heads(args),
        "show" => cmd_show(args),
        "history" => cmd_history(args),
        "migrate" => cmd_migrate(args),
        "hook" => cmd_hook(args),
        "pack" => cmd_pack(args),
        "add" => cmd_add(args),
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
    let a = render::answer(&l.graph, &l.root, &key, &scope);
    if args.json {
        println!("{}", a.json());
    } else {
        print!("{}", a.text());
        let _ = std::io::stdout().flush();
    }
    a.exit()
}

/// The decision vocabulary.
///
/// Discovery, not authority. §12.1 rules out finding a decision by similarity,
/// so asking about a key means knowing its exact name — and until this verb
/// there was no way to learn one from the tool itself.
fn cmd_keys(args: &Args) -> i32 {
    if !args.positional.is_empty() {
        eprintln!("aval: `keys` takes no arguments");
        return E_USAGE;
    }
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    if args.json {
        println!("{}", render::keys_json(&l.graph, &l.packs));
    } else {
        print!("{}", render::keys_text(&l.graph, &l.packs));
        let _ = std::io::stdout().flush();
    }
    0
}

/// Serve the corpus as MCP tools on stdio.
///
/// Reads no corpus here: a registry mid-edit must not take the surface away,
/// and each call loads the working tree fresh anyway.
fn cmd_mcp(args: &Args) -> i32 {
    if !args.positional.is_empty() {
        eprintln!("aval: `mcp` takes no arguments");
        return E_USAGE;
    }
    aval::mcp::serve(&args.dir)
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
    findings.extend(packfile::findings(&l));
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
    // No `dir` means `adr_dir` is the repository root, and the repository's own
    // README is not an ADR index. Checking it would report a link table in
    // somebody's project readme as a corpus defect.
    if !l.graph.registry().has_dir() {
        return Vec::new();
    }
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
    if (args.write || args.check) && !heads::applies(&l) {
        // A registry that only vendors keeps no records and declares no `dir`,
        // so there is nowhere to put the file. `aval heads` on its own still
        // prints the borrowed heads, which is what the session hook reads.
        eprintln!(
            "aval: this registry declares no `dir`, so there is nowhere to write \
             {}; `aval heads` prints them instead",
            load::HEADS
        );
        return E_USAGE;
    }
    if args.write {
        // Content-idempotent: a file that already states the projection keeps
        // its bytes, so a formatter's padding is not undone on every run and
        // then reapplied on every commit.
        match heads::write(&l) {
            // Section 12 says `--json` writes the result object to stdout and
            // nothing else. These three printed nothing at all under `--json`,
            // and `--check` printed its findings to stderr, so the row-level
            // detail was unreachable from a machine caller.
            Ok(heads::Wrote::Unchanged) => {
                heads_json_or_text(args, "unchanged", &path, &[]);
                0
            }
            Ok(heads::Wrote::Written) => {
                heads_json_or_text(args, "written", &path, &[]);
                0
            }
            Err(e) => {
                emit_error(args, E_FAIL, &format!("{}: {}", path.display(), e), &[]);
                E_FAIL
            }
        }
    } else if args.check {
        let f = heads::findings(&l);
        if f.is_empty() {
            heads_json_or_text(args, "current", &path, &[]);
            0
        } else {
            heads_json_or_text(args, "stale", &path, &f);
            E_FAIL
        }
    } else if args.json {
        // Section 12: `--json` writes the result object and nothing else. Bare
        // `heads` used to ignore the flag and print the markdown table, so a
        // machine caller asking for JSON silently got a document instead.
        println!("{}", render::heads_json(&l.graph));
        0
    } else {
        print!("{}", text);
        0
    }
}

/// One place that decides where `heads` output goes, because section 12 says
/// `--json` writes the result object to stdout and nothing else.
fn heads_json_or_text(args: &Args, state: &str, path: &std::path::Path, findings: &[Finding]) {
    if args.json {
        let j = Json::obj()
            .set("state", state)
            .set("file", path.display().to_string())
            .set(
                "findings",
                findings
                    .iter()
                    .map(render::finding_json)
                    .collect::<Vec<_>>(),
            );
        println!("{}", j);
        return;
    }
    match state {
        "current" => println!("aval: HEADS.md is current"),
        "unchanged" => println!("aval: {} already current", path.display()),
        "written" => println!("aval: wrote {}", path.display()),
        _ => {}
    }
    for f in findings {
        eprintln!("{}", f);
    }
}

/// `aval hook install` — the session-start hook.
///
/// The install is the whole command: there is no uninstall, because removing
/// the entry from `.claude/settings.json` and deleting one script is a thing a
/// person can do correctly without a subcommand that might do it wrongly.
fn cmd_hook(args: &Args) -> i32 {
    let what = match one_positional(args, "`install`") {
        Ok(k) => k,
        Err(c) => return c,
    };
    if what != "install" {
        eprintln!(
            "aval: unknown hook command `{}`; the only one is `install`",
            what
        );
        return E_USAGE;
    }
    //  already resolved this, defaulting to the working directory. The
    // hook installs where the person is, not where the registry happens to be:
    // a repository may hold a corpus in a subdirectory and still want the hook
    // at its own root.
    let root = args.dir.clone();
    let report = match hook::install(&root, args.check) {
        Ok(r) => r,
        Err(hook::Error::SettingsUnparseable(e)) => {
            // Stop rather than replace it. A settings file this cannot read is
            // one somebody wrote, and overwriting it would lose whatever else
            // it says.
            eprintln!(
                "aval: {} is not valid JSON ({}) — fix or remove it, then run this again",
                hook::SETTINGS_PATH,
                e
            );
            return E_INVALID;
        }
        Err(hook::Error::Io(e)) => {
            eprintln!("aval: {}", e);
            return E_FAIL;
        }
    };

    for c in &report.changes {
        let word = match c.status {
            hook::Status::Written => "wrote",
            hook::Status::Unchanged => "ok",
            hook::Status::Stale => "stale",
        };
        println!("  {:<6} {}  ({})", word, c.path, c.detail);
    }

    if args.check {
        let n = report.stale();
        if n == 0 {
            println!("aval hook install --check: up to date");
            return 0;
        }
        println!(
            "aval hook install --check: {} file(s) out of date; run `aval hook install`",
            n
        );
        return E_FAIL;
    }

    println!();
    println!(
        "Repo-specific caveats go in {} — the hook appends that file after the",
        hook::NOTES_PATH
    );
    println!("heads, and installing again leaves it alone.");

    if !report.ignored.is_empty() {
        println!();
        println!("WARNING: git ignores these, so the hook would work here and ship to nobody.");
        println!("Add a negation for each:");
        for p in &report.ignored {
            println!("  !{}", p);
        }
    }
    0
}

/// `aval pack` — publish this corpus's declarations.
fn cmd_pack(args: &Args) -> i32 {
    if args.write && args.check {
        eprintln!("aval: `--write` and `--check` are mutually exclusive");
        return E_USAGE;
    }
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let path = packfile::path(&l);
    if args.write {
        match packfile::write(&l) {
            Ok(packfile::Wrote::Unchanged) => {
                println!("aval: {} already current", path.display());
                0
            }
            Ok(packfile::Wrote::Written) => {
                println!("aval: wrote {}", path.display());
                0
            }
            Err(e) => {
                emit_error(args, E_FAIL, &format!("{}: {}", path.display(), e), &[]);
                E_FAIL
            }
        }
    } else if args.check {
        let f = packfile::findings(&l);
        if f.is_empty() {
            if !packfile::publishes(&l) {
                println!("aval: this corpus publishes no pack");
            } else {
                println!("aval: {} is current", aval_core::pack::FILE);
            }
            0
        } else {
            for x in &f {
                println!("{}", x);
            }
            E_FAIL
        }
    } else {
        print!("{}", packfile::render(&l));
        0
    }
}

/// `aval add --check` — are the vendored packs still what their revisions name?
///
/// This is the one thing `amont` deliberately does not have, and the reason
/// for the difference is the payload. Its vendored rows are commands, so being
/// behind is safe and updating is the risk. A vendored *decision* is the other
/// way round: being behind means answering `active` with something that was
/// superseded, which is precisely the failure the whole tool exists to
/// prevent.
///
/// It reaches the network, so it is human-run and nothing calls it. The gate
/// does not, the hook does not, and CI cannot — the corpus this was built for
/// is private on a forge the consumers' CI has no credential for. That is a
/// real cost, and it is stated rather than papered over: a fleet decision that
/// changes has to be re-added in each consumer, by a person, on purpose.
fn cmd_add_check(args: &Args) -> i32 {
    let l = match loaded(args) {
        Ok(l) => l,
        Err(c) => return c,
    };
    let packs = &l.graph.registry().packs;
    if packs.is_empty() {
        println!("aval: this repository vendors no packs");
        return 0;
    }

    let mut behind = 0;
    let mut unknown = 0;
    for o in add::origins(&l.root, packs) {
        let o = match o {
            Ok(o) => o,
            Err(e) => {
                println!("  {:<9} {}", "unmarked", e);
                unknown += 1;
                continue;
            }
        };
        match add::standing(&o) {
            add::Standing::Current => println!(
                "  {:<9} {} @ {}  ({})",
                "current",
                o.name,
                aval::fetch::short(&o.commit),
                o.source
            ),
            add::Standing::Behind(now) => {
                behind += 1;
                println!(
                    "  {:<9} {} has {}, {} now names {}",
                    "behind",
                    o.name,
                    aval::fetch::short(&o.commit),
                    o.rev,
                    aval::fetch::short(&now)
                );
            }
            add::Standing::Unknown(e) => {
                unknown += 1;
                println!("  {:<9} {}: {}", "unknown", o.name, first_line(&e));
            }
            add::Standing::Unmarked => {
                unknown += 1;
                println!(
                    "  {:<9} {}: `{}` is not a source this version understands",
                    "unknown", o.name, o.source
                );
            }
        }
    }

    if behind == 0 {
        if unknown > 0 {
            println!(
                "\n{} pack(s) could not be checked. Being unable to ask is not an answer.",
                unknown
            );
        }
        return 0;
    }
    println!(
        "\n{} pack(s) behind. Run `aval add <source>` for each, read the diff, \
         then `aval heads --write`.",
        behind
    );
    E_FAIL
}

fn first_line(s: &str) -> &str {
    s.lines().next().unwrap_or(s)
}

/// `aval add` — vendor another repository's declarations.
///
/// Every source is resolved, fetched and parsed before any of them is written,
/// so a second source that turns out to be unreachable does not leave the first
/// one half-installed.
fn cmd_add(args: &Args) -> i32 {
    if args.check {
        return cmd_add_check(args);
    }
    if args.positional.is_empty() {
        eprintln!(
            "aval: `add` needs a source (github:owner/repo, \
             forgejo:host/owner/repo, a git URL, or a path)"
        );
        return E_USAGE;
    }
    if args.as_name.is_some() && args.positional.len() > 1 {
        eprintln!("aval: `--as` names one pack; add them one at a time");
        return E_USAGE;
    }
    let root = match load::find_root(&args.dir) {
        Some(r) => r,
        None => {
            eprintln!(
                "aval: no {} in {} or any parent directory — a pack is vendored \
                 into a corpus, so there has to be one",
                load::REGISTRY,
                args.dir.display()
            );
            return E_INVALID;
        }
    };

    let mut vendored = Vec::new();
    for spec in &args.positional {
        match add::resolve_one(spec, args.as_name.as_deref()) {
            Ok(v) => vendored.push(v),
            Err(e) => {
                eprintln!("aval: {}", e);
                return E_FAIL;
            }
        }
    }

    // Plan everything, then print, then write. A collision must stop the whole
    // command rather than the one source that hit it.
    let mut plans = Vec::new();
    for v in &vendored {
        match add::plan(&root, v) {
            Ok(add::Plan::Collision(other)) => {
                eprintln!(
                    "aval: {} already holds {}, which came from {} — name this \
                     one with `--as <name>`",
                    v.rel, v.name, other
                );
                return E_FAIL;
            }
            Ok(p) => plans.push(p),
            Err(e) => {
                eprintln!("aval: {}", e);
                return E_FAIL;
            }
        }
    }

    for (v, p) in vendored.iter().zip(&plans) {
        // An owned String rather than a borrow of one arm's temporary: the
        // MSRV does not extend a temporary's lifetime out of a match arm, and
        // the newer compiler that does would have let this reach a release.
        let state: String = match p {
            add::Plan::New => "new".into(),
            add::Plan::Unchanged => "unchanged".into(),
            add::Plan::Update(old) => format!("was {}", aval::fetch::short(old)),
            add::Plan::Collision(_) => unreachable!("returned above"),
        };
        println!(
            "{} @ {} ({}) declares {} record(s), {} key(s):",
            v.source.label,
            aval::fetch::short(&v.id),
            state,
            v.records,
            v.keys.len()
        );
        for k in &v.keys {
            println!("    {}", k);
        }
    }

    if args.dry_run {
        println!("\n--dry-run: nothing written");
        return 0;
    }

    let mut registered = Vec::new();
    for v in &vendored {
        match add::write(&root, v) {
            Ok(newly) => {
                if newly {
                    registered.push(v.rel.clone());
                }
            }
            Err(e) => {
                eprintln!("aval: {}", e);
                return E_FAIL;
            }
        }
    }

    println!();
    for v in &vendored {
        println!("  wrote {}", v.rel);
    }
    for r in &registered {
        println!("  listed {} in {}", r, load::REGISTRY);
    }

    // A vendored decision is decided here, so it belongs in the projection —
    // which means the projection is now out of date, and the gate will say so
    // at the least convenient moment. Say it here instead.
    if let Ok(l) = load::load(&root) {
        if heads::applies(&l) && !heads::findings(&l).is_empty() {
            println!("  stale  {}/{}", l.graph.registry().dir, load::HEADS);
            println!("\nThe projection now has rows it did not have. Run `aval heads --write`.");
        }
    }

    println!();
    println!(
        "These decisions are not yours to edit. To change one, change it in the \
         repository\nit came from and run `aval add` again; a local record \
         deciding the same slot is a\ncontradiction, which is what makes one \
         decision one decision."
    );
    0
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
                render::show_unknown_json(&id, sug.map(|s| s.to_string()))
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
    // Both rejections used to print to stderr and return 7 with nothing on
    // stdout, so `history --json` answered a machine caller with silence.
    if !l.graph.registry().has_key(&key) {
        let names: Vec<&str> = l
            .graph
            .registry()
            .keys
            .iter()
            .map(|k| k.name.as_str())
            .collect();
        let sug = aval_core::model::suggest(&key, names).map(|s| s.to_string());
        if args.json {
            println!("{}", render::history_unknown_json("key", &key, &scope, sug));
        } else {
            eprintln!("aval: `{}` is not a registered decision key", key);
            if let Some(g) = &sug {
                eprintln!("  did you mean `{}`? A suggestion is advisory.", g);
            }
        }
        return 7;
    }
    if !l.graph.registry().has_scope(&scope) {
        let names: Vec<&str> = l
            .graph
            .registry()
            .scopes
            .iter()
            .map(|s| s.as_str())
            .collect();
        let sug = aval_core::model::suggest(&scope, names).map(|s| s.to_string());
        if args.json {
            println!(
                "{}",
                render::history_unknown_json("scope", &key, &scope, sug)
            );
        } else {
            eprintln!("aval: `{}` is not a declared scope", scope);
            if let Some(g) = &sug {
                eprintln!("  did you mean `{}`? A suggestion is advisory.", g);
            }
        }
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
            emit_error(args, E_INVALID, &e, &[]);
            E_INVALID
        }
    }
}

/// Silences the unused-import warning when `Verdict` is only matched in render.
#[allow(dead_code)]
fn _assert_verdict_in_scope(v: &Verdict) -> i32 {
    v.exit()
}
