//! Which rules apply where (SEMANTICS section 2.5).
//!
//! Applicability is display, never activity. Nothing here changes a verdict,
//! whether a rule is active, or what `check` reports; it decides which active
//! rules a surface lists, and every surface that uses it reports what it hid.
//!
//! `None` from the functions below means **no filtering**, and it is not the
//! same as an empty trait set: a registry without `areas`, or a path no area
//! covers, filters nothing, while an area that declares `[]` leaves only
//! untargeted rules.

use crate::glob;
use crate::model::{Level, Registry, Rule};

/// Whether a rule applies to a set of traits: it is untargeted, or it names
/// one of them.
pub fn applies(rule: &Rule, traits: &[String]) -> bool {
    rule.applies.is_empty() || rule.applies.iter().any(|t| traits.contains(t))
}

/// R: the union of every declared area's traits, or `None` when the registry
/// declares no areas and nothing is filtered.
pub fn repo_traits(reg: &Registry) -> Option<Vec<String>> {
    if reg.areas.is_empty() {
        return None;
    }
    let mut v: Vec<String> = reg
        .areas
        .iter()
        .flat_map(|a| a.traits.iter().cloned())
        .collect();
    v.sort();
    v.dedup();
    Some(v)
}

/// The traits of one path: the union of every area matching it, or `None`
/// when no area matches (or none is declared) and nothing is filtered for it.
///
/// A path ending in `/` is a directory, and an area applies to it when it
/// matches anything beneath it: `--path web` is covered by `web/**`, which
/// matched no file literally named `web`.
pub fn path_traits(reg: &Registry, path: &str) -> Option<Vec<String>> {
    let dir = path.strip_suffix('/');
    let mut hit = false;
    let mut v: Vec<String> = Vec::new();
    for a in &reg.areas {
        let covers = match dir {
            Some(d) => glob::matches_under(&a.glob, d),
            None => glob::matches(&a.glob, path),
        };
        if covers {
            hit = true;
            v.extend(a.traits.iter().cloned());
        }
    }
    if !hit {
        return None;
    }
    v.sort();
    v.dedup();
    Some(v)
}

/// The traits in force for a query, as the sets a rule is tested against.
///
/// With no paths it is R. With paths it is one set per path, and a path no
/// area covers contributes no set at all but makes the whole query
/// unfiltered — a rule is kept when it applies to **any** queried path, and
/// an uncovered path is one every rule applies to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// Nothing is filtered.
    All,
    /// A rule is kept when it applies to at least one of these sets.
    AnyOf(Vec<Vec<String>>),
}

impl Scope {
    pub fn for_query(reg: &Registry, paths: &[String]) -> Scope {
        if reg.areas.is_empty() {
            return Scope::All;
        }
        if paths.is_empty() {
            return match repo_traits(reg) {
                Some(r) => Scope::AnyOf(vec![r]),
                None => Scope::All,
            };
        }
        let mut sets = Vec::new();
        for p in paths {
            match path_traits(reg, p) {
                None => return Scope::All,
                Some(t) => sets.push(t),
            }
        }
        Scope::AnyOf(sets)
    }

    pub fn keeps(&self, rule: &Rule) -> bool {
        match self {
            Scope::All => true,
            Scope::AnyOf(sets) => sets.iter().any(|t| applies(rule, t)),
        }
    }

    /// The traits named by this scope, for the `omitted` report.
    pub fn traits(&self) -> Vec<String> {
        match self {
            Scope::All => Vec::new(),
            Scope::AnyOf(sets) => {
                let mut v: Vec<String> = sets.iter().flatten().cloned().collect();
                v.sort();
                v.dedup();
                v
            }
        }
    }
}

/// What a filtered listing left out because of traits alone.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct Omitted {
    pub constraints: usize,
    pub heuristics: usize,
    /// The traits the listing was filtered by; empty when traits filtered
    /// nothing (a queried path in no area, or every trait asked for).
    pub traits: Vec<String>,
}

impl Omitted {
    pub fn new(traits: Vec<String>) -> Omitted {
        Omitted {
            constraints: 0,
            heuristics: 0,
            traits,
        }
    }

    pub fn count(&mut self, rule: &Rule) {
        match rule.level {
            Level::Constraint => self.constraints += 1,
            Level::Heuristic => self.heuristics += 1,
        }
    }

    pub fn total(&self) -> usize {
        self.constraints + self.heuristics
    }
}

/// Split `rules` (already narrowed by every other filter the caller asked
/// for) into what the scope keeps and what it omitted. The report is `None`
/// when the registry declares no areas, because then nothing was filtered and
/// no surface may say it was.
pub fn filter<'a>(
    reg: &Registry,
    scope: &Scope,
    rules: Vec<&'a Rule>,
) -> (Vec<&'a Rule>, Option<Omitted>) {
    if reg.areas.is_empty() {
        return (rules, None);
    }
    // The traits the listing was filtered BY. When nothing was filtered —
    // a queried path in no area, or `--all-traits` — that is none, and
    // naming the repository's traits there read as a filter that never ran.
    let mut om = Omitted::new(scope.traits());
    let mut kept = Vec::new();
    for r in rules {
        if scope.keeps(r) {
            kept.push(r);
        } else {
            om.count(r);
        }
    }
    (kept, Some(om))
}

/// Why `name` is not a usable trait name, or `None` when it is.
pub fn bad_trait(name: &str) -> Option<String> {
    let ok = name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && name.len() <= 32;
    if ok {
        None
    } else {
        Some(format!(
            "trait `{}` must be lowercase letters, digits and `-`, starting with a \
             letter, at most 32 characters",
            name
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{AdrId, Area};

    fn rule(id: &str, level: Level, applies: &[&str]) -> Rule {
        let mut r = Rule::new(id, level, AdrId::new("ADR-0001"), "s", "f.md");
        r.applies = applies.iter().map(|s| s.to_string()).collect();
        r
    }

    fn reg(areas: &[(&str, &[&str])]) -> Registry {
        let mut r = Registry::empty("docs/adr");
        r.areas = areas
            .iter()
            .map(|(g, t)| Area::new(*g, t.iter().map(|s| s.to_string()).collect(), 1))
            .collect();
        r
    }

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn no_areas_filters_nothing() {
        let r = reg(&[]);
        assert_eq!(Scope::for_query(&r, &[]), Scope::All);
        let cli = rule("cli.a", Level::Constraint, &["cli"]);
        let (kept, om) = filter(&r, &Scope::All, vec![&cli]);
        assert_eq!(kept.len(), 1);
        assert!(om.is_none());
    }

    #[test]
    fn repo_union_hides_other_traits() {
        let r = reg(&[("web/**", &["ui"])]);
        let sc = Scope::for_query(&r, &[]);
        let cli = rule("cli.a", Level::Constraint, &["cli"]);
        let ui = rule("ui.a", Level::Heuristic, &["ui"]);
        let any = rule("x.a", Level::Constraint, &[]);
        let (kept, om) = filter(&r, &sc, vec![&cli, &ui, &any]);
        assert_eq!(kept.len(), 2);
        let om = om.unwrap();
        assert_eq!((om.constraints, om.heuristics), (1, 0));
        assert_eq!(om.traits, s(&["ui"]));
    }

    #[test]
    fn overlapping_areas_union() {
        let r = reg(&[("**", &["ui"]), ("cmd/**", &["cli"])]);
        assert_eq!(path_traits(&r, "cmd/main.go"), Some(s(&["cli", "ui"])));
        assert_eq!(path_traits(&r, "web/a.tsx"), Some(s(&["ui"])));
    }

    #[test]
    fn empty_area_leaves_only_untargeted() {
        let r = reg(&[("docs/**", &[])]);
        let sc = Scope::for_query(&r, &s(&["docs/a.md"]));
        assert!(!sc.keeps(&rule("cli.a", Level::Constraint, &["cli"])));
        assert!(sc.keeps(&rule("x.a", Level::Constraint, &[])));
    }

    #[test]
    fn uncovered_path_filters_nothing() {
        let r = reg(&[("web/**", &["ui"])]);
        assert_eq!(Scope::for_query(&r, &s(&["scripts/x.sh"])), Scope::All);
        assert_eq!(
            Scope::for_query(&r, &s(&["web/a.tsx", "scripts/x.sh"])),
            Scope::All
        );
    }

    #[test]
    fn a_directory_takes_the_areas_under_it() {
        let r = reg(&[("web/**", &["ui"]), ("cmd/**", &["cli"])]);
        assert_eq!(path_traits(&r, "web/"), Some(s(&["ui"])));
        assert_eq!(path_traits(&r, "web"), None);
        assert_eq!(path_traits(&r, "docs/"), None);
        assert_eq!(path_traits(&r, "/"), Some(s(&["cli", "ui"])));
    }

    #[test]
    fn several_paths_keep_on_any() {
        let r = reg(&[("web/**", &["ui"]), ("cmd/**", &["cli"])]);
        let sc = Scope::for_query(&r, &s(&["web/a.tsx", "cmd/main.go"]));
        assert!(sc.keeps(&rule("cli.a", Level::Constraint, &["cli"])));
        assert!(sc.keeps(&rule("ui.a", Level::Constraint, &["ui"])));
        assert!(!sc.keeps(&rule("d.a", Level::Constraint, &["data"])));
    }

    #[test]
    fn empty_applies_is_untargeted() {
        assert!(applies(&rule("x.a", Level::Constraint, &[]), &[]));
    }

    #[test]
    fn trait_names() {
        assert!(bad_trait("cli").is_none());
        assert!(bad_trait("http-api").is_none());
        assert!(bad_trait("CLI").is_some());
        assert!(bad_trait("1x").is_some());
        assert!(bad_trait("").is_some());
    }
}
