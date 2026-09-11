//! `aval migrate` — read a LEGACY corpus and report what converting it needs.
//!
//! Deliberately a report and not a rewrite. A decision record is the record of
//! a judgment, and the one thing an automated pass cannot do is decide which
//! decision keys a document actually carries or whether it was ever approved.
//! A half-correct rewrite of sixteen decision records is worse than none,
//! because the result looks converted.
//!
//! So this reads the prose-bullet format that predates frontmatter, extracts
//! what is mechanically there, and says per file what a human still has to
//! supply. It never writes.

use aval_core::json::Json;
use aval_core::yaml;
use std::path::Path;

/// The bullet fields the legacy format uses, spec and off-spec alike.
const LEGACY_FIELDS: &[&str] = &[
    "Date",
    "Status",
    "Supersedes",
    "Superseded by",
    "Deciders",
    "Related",
    "Note",
];

/// The four status values the legacy template allowed.
const LEGACY_STATUSES: &[&str] = &["Proposed", "Accepted", "Superseded", "Deprecated"];

pub struct Legacy {
    pub file: String,
    /// A `Date` bullet the old template could not parse. Informational: the
    /// model never reads dates, so this needs no action per document.
    pub loose_date: bool,
    pub id: String,
    pub title: String,
    /// `(field, value)` in document order.
    pub fields: Vec<(String, String)>,
    pub has_frontmatter: bool,
    pub notes: Vec<String>,
}

fn id_from_filename(file: &str) -> Option<String> {
    let digits: String = file.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        Some(format!("ADR-{}", digits))
    }
}

/// `- **Field**: value`, between the H1 and the first `##`.
fn legacy_fields(src: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for line in src.lines() {
        let t = line.trim();
        if t.starts_with("## ") {
            break;
        }
        let Some(rest) = t.strip_prefix("- **") else {
            continue;
        };
        let Some(end) = rest.find("**") else { continue };
        let name = rest[..end].to_string();
        let value = rest[end + 2..].trim_start_matches(':').trim().to_string();
        out.push((name, value));
    }
    out
}

fn title_of(src: &str) -> String {
    src.lines()
        .find(|l| l.starts_with("# "))
        .map(|l| l[2..].trim().to_string())
        .unwrap_or_default()
}

fn read(file: &str, src: &str) -> Legacy {
    let has_frontmatter = yaml::split_frontmatter(src).is_some();
    let fields = legacy_fields(src);
    let mut notes = Vec::new();

    let id = match id_from_filename(file) {
        Some(i) => i,
        None => {
            notes.push("the filename has no leading ADR number".to_string());
            String::new()
        }
    };

    let get = |k: &str| fields.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str());

    match get("Status") {
        None => notes.push("no Status bullet; decide `draft` or `accepted`".to_string()),
        Some(s) => {
            let head = s.split_whitespace().next().unwrap_or("");
            if !LEGACY_STATUSES.contains(&head) {
                notes.push(format!("Status `{}` is off-template", s));
            }
            if s.contains('(') {
                notes.push(format!(
                    "Status carries a parenthetical (`{}`); the parenthetical is prose, \
                     the status is not",
                    s
                ));
            }
            if head == "Superseded" {
                notes.push(
                    "`Superseded` is DERIVED and must not be written. Move this into the \
                     successor's `replaces:`, or into a `retire:` entry if nothing replaced it"
                        .to_string(),
                );
            }
            if head == "Proposed" {
                notes.push(
                    "decide `draft` or `accepted` from evidence of APPROVAL, not from whether \
                     the work shipped"
                        .to_string(),
                );
            }
        }
    }

    if let Some(s) = get("Supersedes") {
        if !s.contains("ADR-") && !s.contains("](") {
            notes.push(format!(
                "`Supersedes: {}` names no ADR id; supersession must point at a document",
                s
            ));
        }
    }

    for (name, _) in &fields {
        if !LEGACY_FIELDS.contains(&name.as_str()) {
            notes.push(format!("unrecognised bullet `{}`; keep it as prose", name));
        }
    }

    notes.push(
        "decide which decision KEYS this document carries; one entry per key, per scope"
            .to_string(),
    );

    let loose_date = get("Date")
        .map(|d| d.len() != 10 || !d.starts_with(|c: char| c.is_ascii_digit()))
        .unwrap_or(false);

    Legacy {
        file: file.to_string(),
        loose_date,
        id,
        title: title_of(src),
        fields,
        has_frontmatter,
        notes,
    }
}

pub struct Audit {
    pub docs: Vec<Legacy>,
    pub global: Vec<String>,
}

pub fn audit(dir: &Path) -> Result<Audit, String> {
    let mut paths: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {}", dir.display(), e))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().map(|x| x == "md").unwrap_or(false))
        .collect();
    paths.sort();

    let mut docs = Vec::new();
    let mut global = Vec::new();
    for p in &paths {
        let file = p.file_name().unwrap().to_string_lossy().to_string();
        if !file
            .chars()
            .next()
            .map(|c| c.is_ascii_digit())
            .unwrap_or(false)
        {
            continue;
        }
        let src = std::fs::read_to_string(p).map_err(|e| format!("{}: {}", file, e))?;
        docs.push(read(&file, &src));
    }

    // Duplicate ids are the defect a numbered corpus produces on its own, and
    // nothing in a hand-maintained index can catch it.
    for (i, d) in docs.iter().enumerate() {
        if let Some(prev) = docs[..i].iter().find(|p| p.id == d.id && !d.id.is_empty()) {
            global.push(format!(
                "{} is claimed by both {} and {}; one must be renumbered",
                d.id, prev.file, d.file
            ));
        }
    }

    let loose = docs.iter().filter(|d| d.loose_date).count();
    if loose > 0 {
        global.push(format!(
            "{} of {} documents carry a Date the old template could not parse. \
             Nothing to do: ordering comes from edges, never dates, so the model \
             never reads the field. Leave them in prose",
            loose,
            docs.len()
        ));
    }

    let readme = dir.join("README.md");
    if let Ok(src) = std::fs::read_to_string(&readme) {
        let rows = src
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("| [")
                    || (t.starts_with('|') && t.contains("](") && t.contains(".md)"))
            })
            .count();
        if rows > 0 {
            global.push(format!(
                "README.md carries {} hand-maintained index rows for {} documents. \
                 The index is copied state; HEADS.md replaces it",
                rows,
                docs.len()
            ));
        }
    }

    Ok(Audit { docs, global })
}

pub fn text(a: &Audit) -> String {
    let mut s = String::new();
    let converted = a.docs.iter().filter(|d| d.has_frontmatter).count();
    s.push_str(&format!(
        "{} document(s), {} already converted\n\n",
        a.docs.len(),
        converted
    ));
    for d in &a.docs {
        if d.has_frontmatter {
            s.push_str(&format!("{}  {}  already converted\n\n", d.id, d.file));
            continue;
        }
        s.push_str(&format!("{}  {}\n", d.id, d.file));
        if !d.title.is_empty() {
            s.push_str(&format!("  {:<11} {}\n", "title", d.title));
        }
        for (k, v) in &d.fields {
            s.push_str(&format!("  {:<11} {}\n", k.to_lowercase(), v));
        }
        for n in &d.notes {
            s.push_str(&format!("  {:<11} {}\n", "todo", n));
        }
        s.push('\n');
    }
    if !a.global.is_empty() {
        s.push_str("corpus\n");
        for g in &a.global {
            s.push_str(&format!("  {:<11} {}\n", "todo", g));
        }
        s.push('\n');
    }
    s.push_str(
        "Nothing was written. Convert by hand: the keys a document carries, and whether it \n\
         was approved, are judgments this cannot make for you.\n",
    );
    s
}

pub fn json(a: &Audit) -> Json {
    let docs: Vec<Json> = a
        .docs
        .iter()
        .map(|d| {
            Json::obj()
                .set("adr", d.id.as_str())
                .set("file", d.file.as_str())
                .set("title", d.title.as_str())
                .set("converted", d.has_frontmatter)
                .set(
                    "legacy_fields",
                    Json::Obj(
                        d.fields
                            .iter()
                            .map(|(k, v)| (k.clone(), Json::Str(v.clone())))
                            .collect(),
                    ),
                )
                .set(
                    "todo",
                    d.notes.iter().map(|n| n.as_str()).collect::<Vec<_>>(),
                )
        })
        .collect();
    Json::obj()
        .set("documents", docs)
        .set(
            "corpus_todo",
            a.global.iter().map(|g| g.as_str()).collect::<Vec<_>>(),
        )
        .set("wrote_anything", false)
}

#[cfg(test)]
mod tests {
    use super::*;

    const LEGACY: &str = "\
# 0011 — Build ddns-updater-operator

- **Date**: predates the consolidation
- **Status**: Superseded (retired 2026-08-26)

## Context
";

    #[test]
    fn reads_the_prose_bullet_format() {
        let d = read("0011-ddns-updater-operator.md", LEGACY);
        assert_eq!(d.id, "ADR-0011");
        assert!(d.title.contains("ddns-updater-operator"));
        assert_eq!(d.fields.len(), 2);
        assert!(!d.has_frontmatter);
    }

    #[test]
    fn flags_a_derived_status_written_by_hand() {
        let d = read("0011-x.md", LEGACY);
        assert!(d.notes.iter().any(|n| n.contains("DERIVED")));
    }

    #[test]
    fn flags_a_parenthetical_status() {
        let d = read("0011-x.md", LEGACY);
        assert!(d.notes.iter().any(|n| n.contains("parenthetical")));
    }

    #[test]
    fn a_loose_date_is_noted_but_is_not_a_todo() {
        // The model never reads dates, so a per-document todo here would be
        // fourteen lines of noise telling nobody to do nothing.
        let d = read("0011-x.md", LEGACY);
        assert!(d.loose_date);
        assert!(!d.notes.iter().any(|n| n.contains("YYYY-MM-DD")));
    }

    #[test]
    fn proposed_asks_about_approval_not_about_shipping() {
        let src = LEGACY.replace("Superseded (retired 2026-08-26)", "Proposed");
        let d = read("0013-x.md", &src);
        assert!(d.notes.iter().any(|n| n.contains("not from whether")));
    }

    #[test]
    fn flags_a_supersedes_that_names_no_adr() {
        let src = "# 0001 — t\n\n- **Supersedes**: pre-Flux ArgoCD-era setup\n\n## Context\n";
        let d = read("0001-x.md", src);
        assert!(d.notes.iter().any(|n| n.contains("names no ADR id")));
    }

    #[test]
    fn an_already_converted_document_is_recognised() {
        let src = "---\nid: ADR-0001\nstatus: accepted\ndecisions: []\n---\n# 0001 — t\n";
        let d = read("0001-x.md", src);
        assert!(d.has_frontmatter);
    }

    #[test]
    fn every_document_is_told_to_decide_its_keys() {
        let d = read("0011-x.md", LEGACY);
        assert!(d.notes.iter().any(|n| n.contains("decision KEYS")));
    }
}
