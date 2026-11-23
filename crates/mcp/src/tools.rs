//! The read tools, as JSON for the agent. Labels (relationships, sentences) come out in the
//! language the server was started with; ids, tags and dates stay as they are in the GEDCOM.
use gedcom::{Event, IssueKind, Missing, Person, Severity, Tree, current_year};
use serde_json::{Value, json};
use std::collections::HashSet;

const NOTE_MAX: usize = 2000;

pub fn missing_id(m: Missing) -> &'static str {
    match m {
        Missing::BirthDate => "birth_date",
        Missing::BirthPlace => "birth_place",
        Missing::DeathPlace => "death_place",
        Missing::Parents => "parents",
        Missing::Partner => "partner",
        Missing::Sources => "sources",
    }
}

fn severity_id(s: Severity) -> &'static str {
    match s {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "info",
    }
}

fn event(e: &Event) -> Value {
    let mut v = json!({"tag": e.tag, "label": e.label()});
    for (k, x) in [("type", &e.kind), ("date", &e.date), ("place", &e.place), ("value", &e.value)] {
        if let Some(x) = x.as_deref().filter(|x| !x.trim().is_empty()) {
            v[k] = json!(x);
        }
    }
    v
}

/// Name, sex, birth / death and lifespan: enough to tell people apart.
pub fn brief(p: &Person) -> Value {
    let mut v = json!({"id": p.id, "name": p.name(), "sex": p.sex.code()});
    if let Some(b) = &p.birth {
        v["birth"] = event(b);
    }
    if let Some(d) = &p.death {
        v["death"] = event(d);
    }
    let life = p.lifespan();
    if !life.is_empty() {
        v["lifespan"] = json!(life);
    }
    if p.death.is_none() && p.birth_year().is_none_or(|y| current_year() - y < 100) {
        v["maybe_living"] = json!(true);
    }
    v
}

fn person_or_err<'a>(tree: &'a Tree, id: &str) -> Result<&'a Person, String> {
    tree.person(id).ok_or_else(|| format!("no person with id {id:?}; use search_people to find the id")) // i18n-ok: for agents
}

fn arg_str<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(Value::as_str).map(str::trim).filter(|v| !v.is_empty())
}

fn arg_n(a: &Value, k: &str, default: u64, max: u64) -> usize {
    a.get(k).and_then(Value::as_u64).unwrap_or(default).clamp(1, max) as usize
}

/// "father → mother → father" for an Ahnentafel number (1 = the person).
pub fn ahnentafel_path(n: usize) -> String {
    if n <= 1 {
        return String::new();
    }
    let bits = usize::BITS - n.leading_zeros() - 1;
    (0..bits).rev().map(|b| if n >> b & 1 == 0 { "father" } else { "mother" }).collect::<Vec<_>>().join(" → ") // i18n-ok: for agents
}

pub fn read(tree: &Tree, name: &str, a: &Value, focus: Option<&str>, file: &str) -> Result<Value, String> {
    match name {
        "tree_summary" => {
            let (people, families) = tree.stats();
            let years: Vec<i32> = tree.people.values().flat_map(|p| [p.birth_year(), p.death_year()]).flatten().collect();
            let mut gaps = std::collections::BTreeMap::new();
            let mut total = 0u64;
            for id in &tree.order {
                let c = tree.completeness(id);
                total += c.percent as u64;
                for m in c.missing {
                    *gaps.entry(missing_id(m)).or_insert(0usize) += 1;
                }
            }
            let mut v = json!({"file": file, "title": tree.title(), "people": people, "families": families, "average_completeness_percent": if people > 0 { total / people as u64 } else { 0 }, "people_missing": gaps});
            if let (Some(lo), Some(hi)) = (years.iter().min(), years.iter().max()) {
                v["years"] = json!([lo, hi]);
            }
            if let Some(p) = focus.and_then(|f| tree.person(f)) {
                v["on_screen"] = brief(p);
            }
            Ok(v)
        }
        "search_people" => {
            let limit = arg_n(a, "limit", 20, 200);
            let hits = gedcom::search(tree, arg_str(a, "query").unwrap_or(""));
            let people: Vec<Value> = hits.iter().take(limit).filter_map(|id| tree.person(id)).map(brief).collect();
            Ok(json!({"count": hits.len(), "people": people}))
        }
        "get_person" => {
            let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
            let p = person_or_err(tree, id)?;
            let mut v = brief(p);
            v["given"] = json!(p.given);
            v["surname"] = json!(p.surname);
            v["events"] = p.birth.iter().chain(p.events.iter()).chain(p.death.iter()).map(event).collect();
            let notes: Vec<String> = p.notes.iter().map(|n| if n.chars().count() > NOTE_MAX { format!("{}…", n.chars().take(NOTE_MAX).collect::<String>()) } else { n.clone() }).collect();
            if !notes.is_empty() {
                v["notes"] = json!(notes);
            }
            v["parents"] = tree.parents(id).into_iter().map(brief).collect();
            v["siblings"] = tree.siblings(id).into_iter().map(brief).collect();
            v["unions"] = tree
                .unions(id)
                .into_iter()
                .map(|(f, partner, kids)| json!({"family": f.id, "partner": partner.map(brief), "events": f.events.iter().map(event).collect::<Vec<_>>(), "children": kids.into_iter().map(brief).collect::<Vec<_>>()}))
                .collect();
            let c = tree.completeness(id);
            v["completeness"] = json!({"percent": c.percent, "missing": c.missing.iter().map(|m| missing_id(*m)).collect::<Vec<_>>()});
            if let Some(f) = focus.filter(|f| *f != id)
                && let Some(r) = tree.relationship(f, id)
            {
                v["relationship_to_person_on_screen"] = json!(r);
            }
            Ok(v)
        }
        "ancestors" => {
            let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
            let p = person_or_err(tree, id)?;
            let gens = arg_n(a, "generations", 4, 10);
            let slots = tree.pedigree(id, gens as u32 + 1);
            let mut out = Vec::new();
            for (i, slot) in slots.iter().enumerate().skip(1) {
                let Some(pid) = slot else { continue };
                let Some(anc) = tree.person(pid) else { continue };
                let n = i + 1;
                let mut v = brief(anc);
                v["generation"] = json!(usize::BITS - n.leading_zeros() - 1);
                v["ahnentafel"] = json!(n);
                v["path"] = json!(ahnentafel_path(n));
                if let Some(r) = tree.relationship(id, pid) {
                    v["relationship"] = json!(r);
                }
                out.push(v);
            }
            let known = |g: u32| out.iter().filter(|v| v["generation"] == json!(g)).count();
            let per_gen: Vec<Value> = (1..=gens as u32).map(|g| json!({"generation": g, "known": known(g), "possible": 1u64 << g})).collect();
            Ok(json!({"person": brief(p), "generations": per_gen, "ancestors": out}))
        }
        "descendants" => {
            let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
            let p = person_or_err(tree, id)?;
            let gens = arg_n(a, "generations", 3, 8);
            let mut seen: HashSet<&str> = HashSet::from([id]);
            let mut frontier = vec![id];
            let mut out = Vec::new();
            for g in 1..=gens {
                let mut next = Vec::new();
                for parent in frontier {
                    for (_, partner, kids) in tree.unions(parent) {
                        for k in kids {
                            if !seen.insert(k.id.as_str()) {
                                continue;
                            }
                            let mut v = brief(k);
                            v["generation"] = json!(g);
                            v["parent"] = json!(parent);
                            if let Some(o) = partner {
                                v["other_parent"] = json!(o.id);
                            }
                            out.push(v);
                            next.push(k.id.as_str());
                        }
                    }
                }
                if out.len() > 1000 {
                    break;
                }
                frontier = next;
            }
            Ok(json!({"person": brief(p), "count": out.len(), "descendants": out}))
        }
        "relationship" => {
            let (x, y) = (arg_str(a, "a").ok_or("a is required")?, arg_str(a, "b").ok_or("b is required")?); // i18n-ok: for agents
            person_or_err(tree, x)?;
            person_or_err(tree, y)?;
            let k = tree.kinship(x, y).ok_or("not related")?; // i18n-ok: for agents
            let chain: Vec<Value> = k.path.iter().filter_map(|id| tree.person(id)).map(|p| json!({"id": p.id, "name": p.name()})).collect();
            Ok(json!({"sentence": k.sentence(tree), "label": k.label, "related": k.kind != gedcom::KinKind::None, "generations_up": k.up, "generations_down": k.down, "common_ancestors": k.common, "path": chain}))
        }
        "find_duplicates" => {
            let limit = arg_n(a, "limit", 30, 200);
            if let Some(id) = arg_str(a, "id") {
                let p = person_or_err(tree, id)?;
                let c: Vec<Value> = tree.duplicate_candidates(id).into_iter().take(limit).filter_map(|(o, s)| Some(json!({"person": brief(tree.person(&o)?), "score": (s * 100.).round() / 100.}))).collect();
                return Ok(json!({"person": brief(p), "candidates": c}));
            }
            let pairs: Vec<Value> = tree
                .quality_issues()
                .into_iter()
                .filter(|i| i.kind == IssueKind::PossibleDuplicate && i.people.len() == 2)
                .take(limit)
                .filter_map(|i| Some(json!({"a": brief(tree.person(&i.people[0])?), "b": brief(tree.person(&i.people[1])?), "score": i.a as f64 / 100.})))
                .collect();
            Ok(json!({"count": pairs.len(), "pairs": pairs}))
        }
        "data_gaps" => {
            let limit = arg_n(a, "limit", 50, 500);
            let only = arg_str(a, "missing");
            let ids: Vec<&String> = match arg_str(a, "id") {
                Some(id) => vec![&person_or_err(tree, id)?.id],
                None => tree.order.iter().collect(),
            };
            let mut counts = std::collections::BTreeMap::new();
            let mut rows: Vec<(u8, Value)> = Vec::new();
            for id in ids {
                let c = tree.completeness(id);
                let missing: Vec<&str> = c.missing.iter().map(|m| missing_id(*m)).collect();
                for m in &missing {
                    *counts.entry(*m).or_insert(0usize) += 1;
                }
                if missing.is_empty() || only.is_some_and(|o| !missing.contains(&o)) {
                    continue;
                }
                let mut v = brief(&tree.people[id]);
                v["completeness_percent"] = json!(c.percent);
                v["missing"] = json!(missing);
                v["sentence"] = json!(c.sentence());
                rows.push((c.percent, v));
            }
            rows.sort_by_key(|(p, _)| *p);
            let total = rows.len();
            Ok(json!({"people_missing": counts, "people_with_gaps": total, "people": rows.into_iter().take(limit).map(|(_, v)| v).collect::<Vec<_>>()}))
        }
        "check_consistency" => {
            let limit = arg_n(a, "limit", 50, 500);
            let sev = arg_str(a, "severity");
            let issues: Vec<Value> = tree
                .quality_issues()
                .into_iter()
                .filter(|i| i.kind != IssueKind::PossibleDuplicate && sev.is_none_or(|s| s == severity_id(i.severity)))
                .map(|i| json!({"severity": severity_id(i.severity), "kind": i.kind.label(), "message": i.message(), "people": i.people.iter().filter_map(|p| tree.person(p)).map(|p| json!({"id": p.id, "name": p.name()})).collect::<Vec<_>>(), "family": i.family, "approximate": i.approximate}))
                .collect();
            let total = issues.len();
            Ok(json!({"count": total, "issues": issues.into_iter().take(limit).collect::<Vec<_>>()}))
        }
        _ => Err(format!("unknown tool {name}")), // i18n-ok: for agents
    }
}
