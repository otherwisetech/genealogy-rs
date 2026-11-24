//! The read tools, as JSON for the agent. Labels (relationships, sentences) come out in the
//! language the server was started with; ids, tags and dates stay as they are in the GEDCOM.
use crate::query::{Filter, MatchMode, NamePart, NameQuery, arg_bool, arg_i32, arg_str, children_of, match_args, person_or_err, row};
use crate::stats::arg_n;
use gedcom::{Event, Issue, IssueKind, KinKind, Link, Missing, Person, Severity, Tree, current_year};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

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

/// (stable id, category) of a quality finding.
pub fn issue_kind(k: IssueKind) -> (&'static str, &'static str) {
    use IssueKind::*;
    match k {
        DeathBeforeBirth => ("death_before_birth", "dates"),
        BurialBeforeDeath => ("burial_before_death", "dates"),
        BornBeforeFather => ("born_before_father", "dates"),
        BornBeforeMother => ("born_before_mother", "dates"),
        BornAfterFatherDeath => ("born_after_father_death", "dates"),
        BornAfterMotherDeath => ("born_after_mother_death", "dates"),
        MarriedBeforeBirth => ("married_before_birth", "dates"),
        MarriedAfterDeath => ("married_after_death", "dates"),
        SiblingsTooClose => ("siblings_too_close", "dates"),
        VeryOld => ("very_old", "ages"),
        FatherTooYoung => ("father_too_young", "ages"),
        MotherTooYoung => ("mother_too_young", "ages"),
        FatherTooOld => ("father_too_old", "ages"),
        MotherTooOld => ("mother_too_old", "ages"),
        MarriedTooYoung => ("married_too_young", "ages"),
        LargeAgeGap => ("large_age_gap", "ages"),
        PossiblyDeceased => ("possibly_deceased", "ages"),
        OwnAncestor => ("own_ancestor", "structure"),
        HusbandFemale => ("husband_female", "structure"),
        WifeMale => ("wife_male", "structure"),
        DuplicateChild => ("duplicate_child", "structure"),
        EmptyFamily => ("empty_family", "structure"),
        SingleMemberFamily => ("single_member_family", "structure"),
        Isolated => ("isolated", "structure"),
        MissingName => ("missing_name", "structure"),
        PossibleDuplicate => ("possible_duplicate", "duplicates"),
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

fn offset(a: &Value) -> usize {
    a.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize
}

/// `total`, `offset`, the page and `next_offset` when there is more.
fn page(total: usize, off: usize, limit: usize, rows: Vec<Value>, key: &str) -> Value {
    let mut v = json!({"total": total, "offset": off});
    v[key] = Value::Array(rows);
    if off + limit < total {
        v["next_offset"] = json!(off + limit);
    }
    v
}

/// "father → mother → father" for an Ahnentafel number (1 = the person).
pub fn ahnentafel_path(n: usize) -> String {
    if n <= 1 {
        return String::new();
    }
    let bits = usize::BITS - n.leading_zeros() - 1;
    (0..bits).rev().map(|b| if n >> b & 1 == 0 { "father" } else { "mother" }).collect::<Vec<_>>().join(" → ") // i18n-ok: for agents
}

fn sort_people(people: &mut [&Person], a: &Value) -> Result<(), String> {
    let key = |y: Option<i32>| y.unwrap_or(i32::MAX);
    match arg_str(a, "sort").unwrap_or("name") {
        "name" => {}
        "birth" => people.sort_by_key(|p| key(p.birth_year())),
        "death" => people.sort_by_key(|p| key(p.death_year())),
        "age_at_death" => people.sort_by_key(|p| std::cmp::Reverse(p.birth_year().zip(p.death_year()).map(|(b, d)| d - b).unwrap_or(i32::MIN))),
        s => return Err(format!("sort must be name, birth, death or age_at_death, not {s:?}")), // i18n-ok: for agents
    }
    if arg_bool(a, "descending")?.unwrap_or(false) {
        people.reverse();
    }
    Ok(())
}

pub const COUNT_BY: &[&str] = &["sex", "living", "birth_decade", "birth_century", "death_decade", "death_century", "birth_place", "death_place", "birth_country", "surname", "given_name", "generation"];

/// The `count_by` groups of `people`, most first (periods in time order).
fn count_by(tree: &Tree, people: &[&Person], by: &str, a: &Value, top: usize) -> Result<Value, String> {
    let last = |pl: &str| pl.rsplit(',').map(str::trim).find(|s| !s.is_empty()).unwrap_or("").to_string();
    let place = |e: &Option<Event>, f: &dyn Fn(&str) -> String| e.as_ref().and_then(|e| e.place.as_deref()).map(str::trim).filter(|s| !s.is_empty()).map(f);
    let decade = |y: Option<i32>, size: i32| y.map(|y| format!("{}s", y.div_euclid(size) * size));
    let gens: HashMap<&str, usize> = match by {
        "generation" => {
            let root = arg_str(a, "descendants_of").or(arg_str(a, "ancestors_of")).ok_or("count_by generation needs descendants_of or ancestors_of")?; // i18n-ok: for agents
            let root = person_or_err(tree, root)?.id.as_str();
            if arg_str(a, "descendants_of").is_some() { crate::query::descendants(tree, root, usize::MAX) } else { crate::query::ancestors(tree, root, usize::MAX) }
        }
        _ => HashMap::new(),
    };
    let mut m: HashMap<String, (usize, crate::query::Spellings)> = HashMap::new();
    let mut unknown = 0;
    for p in people {
        let keys: Vec<String> = match by {
            "sex" => vec![p.sex.code().to_string()],
            "living" => vec![
                match crate::query::life(p) {
                    crate::query::Life::Deceased => "deceased",
                    crate::query::Life::Living => "presumed_living",
                    crate::query::Life::Unknown => "unknown",
                }
                .to_string(),
            ],
            "birth_decade" => decade(p.birth_year(), 10).into_iter().collect(),
            "birth_century" => decade(p.birth_year(), 100).into_iter().collect(),
            "death_decade" => decade(p.death_year(), 10).into_iter().collect(),
            "death_century" => decade(p.death_year(), 100).into_iter().collect(),
            "birth_place" => place(&p.birth, &|s| s.to_string()).into_iter().collect(),
            "death_place" => place(&p.death, &|s| s.to_string()).into_iter().collect(),
            "birth_country" => place(&p.birth, &last).into_iter().collect(),
            "surname" => crate::query::surname_parts(&p.surname),
            "given_name" => crate::query::given_parts(&p.given).into_iter().map(str::to_string).collect(),
            "generation" => gens.get(p.id.as_str()).map(|g| g.to_string()).into_iter().collect(),
            _ => return Err(format!("count_by must be one of {}", COUNT_BY.join(", "))), // i18n-ok: for agents
        };
        if keys.is_empty() {
            unknown += 1;
        }
        let mut seen = HashSet::new();
        for k in keys {
            let fk = gedcom::place_key(&k);
            if seen.insert(fk.clone()) {
                let e = m.entry(fk).or_default();
                e.0 += 1;
                e.1.add(&k);
            }
        }
    }
    let timeish = by.ends_with("_decade") || by.ends_with("_century") || by == "generation";
    let mut rows: Vec<(String, usize, String)> = m.into_iter().map(|(k, (n, sp))| (k, n, sp.best())).collect();
    if timeish {
        rows.sort_by_key(|(k, _, _)| k.trim_end_matches('s').parse::<i64>().unwrap_or(i64::MAX));
    } else {
        rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    }
    let distinct = rows.len();
    let groups: Vec<Value> = rows.into_iter().take(top).map(|(k, n, best)| json!({"value": if by == "sex" { k.to_uppercase() } else { best }, "count": n})).collect();
    Ok(json!({"by": by, "distinct": distinct, "without_value": unknown, "groups": groups}))
}

/// `find_people` and `search_people`: filter, then a page of compact rows (or counts).
fn find(tree: &Tree, a: &Value, name: Option<NameQuery>) -> Result<Value, String> {
    let mut filter = Filter::parse(tree, a)?;
    if let Some(q) = name.filter(|q| !q.is_empty()) {
        filter.names.push(q);
    }
    let mut people = filter.people(tree);
    let total = people.len();
    let off = offset(a);
    if let Some(by) = arg_str(a, "count_by") {
        let mut v = count_by(tree, &people, by, a, arg_n(a, "top", 50, 500))?;
        v["total"] = json!(total);
        if let Some(limit) = a.get("limit").and_then(Value::as_u64).filter(|l| *l > 0) {
            v["people"] = people.iter().take(limit.min(200) as usize).map(|p| row(p)).collect();
        }
        return Ok(v);
    }
    sort_people(&mut people, a)?;
    let limit = arg_n(a, "limit", 25, 200);
    let rows = people.iter().skip(off).take(limit).map(|p| row(p)).collect();
    Ok(page(total, off, limit, rows, "people"))
}

pub fn search_people(tree: &Tree, a: &Value) -> Result<Value, String> {
    let (mode, accents) = match_args(a, MatchMode::Word)?;
    let part = match arg_str(a, "fields").unwrap_or("name") {
        "name" => NamePart::Full,
        "given" => NamePart::Given,
        "surname" => NamePart::Surname,
        "all" => NamePart::All,
        f => return Err(format!("fields must be name, given, surname or all, not {f:?}")), // i18n-ok: for agents
    };
    let q = NameQuery::new(arg_str(a, "query").unwrap_or(""), mode, accents, part);
    let mut v = find(tree, a, Some(q))?;
    v["match"] = json!(match mode {
        MatchMode::Exact => "exact",
        MatchMode::Word => "word",
        MatchMode::Prefix => "prefix",
        MatchMode::Substring => "substring",
    });
    Ok(v)
}

fn relationship(tree: &Tree, a: &Value, focus: Option<&str>) -> Result<Value, String> {
    let x = arg_str(a, "a").or(focus).ok_or("a is required")?; // i18n-ok: for agents
    let y = arg_str(a, "b").ok_or("b is required")?; // i18n-ok: for agents
    person_or_err(tree, x)?;
    person_or_err(tree, y)?;
    let k = tree.kinship(x, y).ok_or("not related")?; // i18n-ok: for agents
    let path: Vec<Value> = k
        .path
        .iter()
        .enumerate()
        .filter_map(|(i, id)| {
            let p = tree.person(id)?;
            let mut v = json!({"id": p.id, "name": p.name()});
            if i > 0 {
                v["step"] = json!(match k.links.get(i - 1) {
                    Some(Link::Parent) => "parent",
                    Some(Link::Child) => "child",
                    Some(Link::Spouse) => "partner",
                    None => "",
                });
                v["relation_to_previous"] = json!(k.link_label(tree, i - 1));
            }
            Some(v)
        })
        .collect();
    let kind = match k.kind {
        KinKind::Same => "same_person",
        KinKind::Blood => "blood",
        KinKind::Marriage => "by_marriage",
        KinKind::None => "none",
    };
    let mut v = json!({"sentence": k.sentence(tree), "label": k.label, "kind": kind, "related": k.kind != KinKind::None, "generations_up": k.up, "generations_down": k.down, "path": path});
    if !k.common.is_empty() {
        v["common_ancestors"] = k.common.iter().filter_map(|c| tree.person(c)).map(row).collect();
    }
    if k.half {
        v["half"] = json!(true);
    }
    if k.double {
        v["double"] = json!(true);
    }
    if !k.others.is_empty() {
        v["also_related_as"] = k.others.iter().take(5).map(|o| json!({"label": o.label, "generations_up": o.up, "generations_down": o.down, "common_ancestors": o.common})).collect();
    }
    Ok(v)
}

/// Sort key for a GEDCOM date: exact dates by day, others by their year.
fn date_key(date: Option<&str>) -> (i32, u32, u32) {
    let Some(d) = date else { return (i32::MAX, 0, 0) };
    if let Some((y, m, day)) = gedcom::date::exact_ymd(d) {
        return (y, m.unwrap_or(0), day.unwrap_or(0));
    }
    gedcom::year_of(d).map(|y| (y, 0, 0)).unwrap_or((i32::MAX, 0, 0))
}

fn timeline(tree: &Tree, a: &Value) -> Result<Value, String> {
    let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
    let p = person_or_err(tree, id)?;
    let gens = arg_n(a, "generations", 2, 8);
    let scope = arg_str(a, "scope").unwrap_or("person");
    let mut who: Vec<&str> = match scope {
        "person" => vec![p.id.as_str()],
        "family" => {
            let mut v = vec![p.id.as_str()];
            v.extend(tree.parents(id).into_iter().map(|x| x.id.as_str()));
            v.extend(tree.unions(id).into_iter().flat_map(|(_, partner, kids)| partner.into_iter().chain(kids)).map(|x| x.id.as_str()));
            v.extend(tree.siblings(id).into_iter().map(|x| x.id.as_str()));
            v
        }
        "descendants" => crate::query::descendants(tree, p.id.as_str(), gens).into_keys().collect(),
        "ancestors" => crate::query::ancestors(tree, p.id.as_str(), gens).into_keys().collect(),
        s => return Err(format!("scope must be person, family, descendants or ancestors, not {s:?}")), // i18n-ok: for agents
    };
    let mut seen_ids = HashSet::new();
    who.retain(|x| seen_ids.insert(*x));
    let (from, to) = (arg_i32(a, "from")?, arg_i32(a, "to")?);
    let undated_too = arg_bool(a, "include_undated")?.unwrap_or(false);
    let mut seen_family: HashSet<(String, String, Option<String>)> = HashSet::new();
    let mut rows: Vec<((i32, u32, u32), Value)> = Vec::new();
    let mut undated = 0;
    for pid in &who {
        let name = tree.person(pid).map(|x| x.name()).unwrap_or_default();
        for it in tree.timeline(pid) {
            if let Some(f) = &it.family
                && !seen_family.insert((f.clone(), it.tag.clone(), it.date.clone()))
            {
                continue;
            }
            let key = date_key(it.date.as_deref());
            if key.0 == i32::MAX {
                undated += 1;
                if !undated_too {
                    continue;
                }
            } else if from.is_some_and(|f| key.0 < f) || to.is_some_and(|t| key.0 > t) {
                continue;
            }
            let mut v = json!({"event": it.label, "tag": it.tag, "person": {"id": pid, "name": name}});
            if key.0 != i32::MAX {
                v["year"] = json!(key.0);
            }
            if let Some(d) = it.date {
                v["date"] = json!(d);
            }
            if let Some(pl) = it.place.filter(|s| !s.trim().is_empty()) {
                v["place"] = json!(pl);
            }
            if let Some(o) = it.partner.as_deref().and_then(|o| tree.person(o)) {
                v["with"] = json!({"id": o.id, "name": o.name()});
            }
            rows.push((key, v));
        }
    }
    rows.sort_by_key(|(k, _)| *k);
    let total = rows.len();
    let off = offset(a);
    let limit = arg_n(a, "limit", 100, 500);
    let mut v = page(total, off, limit, rows.into_iter().skip(off).take(limit).map(|(_, v)| v).collect(), "events");
    v["scope"] = json!(scope);
    v["people"] = json!(who.len());
    if !undated_too && undated > 0 {
        v["undated_events_left_out"] = json!(undated);
    }
    Ok(v)
}

fn issue_json(tree: &Tree, i: &Issue) -> Value {
    let (kind, category) = issue_kind(i.kind);
    let mut v = json!({"kind": kind, "category": category, "severity": severity_id(i.severity), "message": i.message(), "people": i.people.iter().filter_map(|p| tree.person(p)).map(|p| json!({"id": p.id, "name": p.name()})).collect::<Vec<_>>()});
    if let Some(f) = &i.family {
        v["family"] = json!(f);
    }
    if i.approximate {
        v["approximate_dates"] = json!(true);
    }
    v
}

fn data_quality(tree: &Tree, a: &Value) -> Result<Value, String> {
    let check = arg_str(a, "check").unwrap_or("summary");
    let limit = arg_n(a, "limit", if check == "summary" { 10 } else { 50 }, 500);
    let off = offset(a);
    let sev = arg_str(a, "severity");
    if sev.is_some_and(|s| !matches!(s, "error" | "warning" | "info")) {
        return Err("severity must be error, warning or info".into()); // i18n-ok: for agents
    }
    let one = arg_str(a, "id").map(|id| person_or_err(tree, id)).transpose()?;
    if !matches!(check, "summary" | "duplicates" | "missing" | "dates" | "ages" | "structure") {
        return Err("check must be summary, duplicates, missing, dates, ages or structure".into()); // i18n-ok: for agents
    }
    if check == "missing" {
        return gaps(tree, a, one, limit, off);
    }
    if check == "duplicates"
        && let Some(p) = one
    {
        let c: Vec<Value> = tree.duplicate_candidates(&p.id).into_iter().skip(off).take(limit).filter_map(|(o, s)| Some(json!({"person": row(tree.person(&o)?), "score": (s * 100.).round() / 100.}))).collect();
        return Ok(json!({"person": row(p), "candidates": c}));
    }
    let all: Vec<Issue> = tree
        .quality_issues()
        .into_iter()
        .filter(|i| sev.is_none_or(|s| s == severity_id(i.severity)))
        .filter(|i| one.is_none_or(|p| i.people.contains(&p.id)))
        .collect();
    if check == "summary" {
        let mut by_kind: BTreeMap<(&str, &str), (usize, &'static str, String)> = BTreeMap::new();
        let mut by_sev: BTreeMap<&str, usize> = BTreeMap::new();
        for i in &all {
            let (kind, cat) = issue_kind(i.kind);
            let e = by_kind.entry((cat, kind)).or_insert((0, severity_id(i.severity), i.kind.label()));
            e.0 += 1;
            *by_sev.entry(severity_id(i.severity)).or_default() += 1;
        }
        let mut kinds: Vec<Value> = by_kind.into_iter().map(|((cat, kind), (n, s, label))| json!({"category": cat, "kind": kind, "severity": s, "label": label, "count": n})).collect();
        kinds.sort_by_key(|k| std::cmp::Reverse(k["count"].as_u64().unwrap_or(0)));
        let mut gap_counts: BTreeMap<&str, usize> = BTreeMap::new();
        let ids: Vec<&String> = match one {
            Some(p) => vec![&p.id],
            None => tree.order.iter().collect(),
        };
        for id in ids {
            for m in tree.completeness(id).missing {
                *gap_counts.entry(missing_id(m)).or_default() += 1;
            }
        }
        let worst: Vec<Value> = all.iter().filter(|i| i.severity == Severity::Error).chain(all.iter().filter(|i| i.severity != Severity::Error && i.kind != IssueKind::PossibleDuplicate)).take(limit).map(|i| issue_json(tree, i)).collect();
        let dups: Vec<Value> = all.iter().filter(|i| i.kind == IssueKind::PossibleDuplicate).take(5).map(|i| issue_json(tree, i)).collect();
        return Ok(json!({"people": tree.people.len(), "issues": all.len(), "by_severity": by_sev, "by_kind": kinds, "people_missing": gap_counts, "worst": worst, "possible_duplicates": dups, "more": "data_quality with check = duplicates | dates | ages | structure | missing lists every case, with offset / limit"})); // i18n-ok: for agents
    }
    let rows: Vec<&Issue> = all.iter().filter(|i| issue_kind(i.kind).1 == check).collect();
    let total = rows.len();
    let out: Vec<Value> = rows
        .into_iter()
        .skip(off)
        .take(limit)
        .map(|i| {
            if i.kind == IssueKind::PossibleDuplicate && i.people.len() == 2 {
                let r = |id: &String| tree.person(id).map(row).unwrap_or(json!(id));
                json!({"a": r(&i.people[0]), "b": r(&i.people[1]), "score": i.a as f64 / 100.})
            } else {
                issue_json(tree, i)
            }
        })
        .collect();
    Ok(page(total, off, limit, out, if check == "duplicates" { "pairs" } else { "issues" }))
}

fn gaps(tree: &Tree, a: &Value, one: Option<&Person>, limit: usize, off: usize) -> Result<Value, String> {
    let only = arg_str(a, "missing");
    if only.is_some_and(|m| !["birth_date", "birth_place", "death_place", "parents", "partner", "sources"].contains(&m)) {
        return Err("missing must be birth_date, birth_place, death_place, parents, partner or sources".into()); // i18n-ok: for agents
    }
    let ids: Vec<&String> = match one {
        Some(p) => vec![&p.id],
        None => tree.order.iter().collect(),
    };
    let mut counts = BTreeMap::new();
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
        let mut v = row(&tree.people[id]);
        v["completeness_percent"] = json!(c.percent);
        v["missing"] = json!(missing);
        rows.push((c.percent, v));
    }
    rows.sort_by_key(|(p, _)| *p);
    let total = rows.len();
    let mut v = page(total, off, limit, rows.into_iter().skip(off).take(limit).map(|(_, v)| v).collect(), "people");
    v["people_missing"] = json!(counts);
    Ok(v)
}

pub fn read(tree: &Tree, name: &str, a: &Value, focus: Option<&str>, file: &str) -> Result<Value, String> {
    match name {
        "tree_stats" => crate::stats::tree_stats(tree, a, focus, file),
        "name_stats" => crate::stats::name_stats(tree, a),
        "place_stats" => crate::stats::place_stats(tree, a),
        "search_people" => search_people(tree, a),
        "find_people" => {
            let (mode, accents) = match_args(a, MatchMode::Word)?;
            find(tree, a, arg_str(a, "name").map(|n| NameQuery::new(n, mode, accents, NamePart::Full)))
        }
        "get_person" => {
            let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
            let p = person_or_err(tree, id)?;
            let mut v = brief(p);
            v["given"] = json!(p.given);
            v["surname"] = json!(p.surname);
            v["events"] = p.events.iter().map(event).collect();
            let notes: Vec<String> = p.notes.iter().map(|n| if n.chars().count() > NOTE_MAX { format!("{}…", n.chars().take(NOTE_MAX).collect::<String>()) } else { n.clone() }).collect();
            if !notes.is_empty() {
                v["notes"] = json!(notes);
            }
            v["parents"] = tree.parents(id).into_iter().map(row).collect();
            v["siblings"] = tree.siblings(id).into_iter().map(row).collect();
            v["unions"] = tree
                .unions(id)
                .into_iter()
                .map(|(f, partner, kids)| json!({"family": f.id, "partner": partner.map(row), "events": f.events.iter().map(event).collect::<Vec<_>>(), "children": kids.into_iter().map(row).collect::<Vec<_>>()}))
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
                let mut v = row(anc);
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
            Ok(json!({"person": row(p), "generations": per_gen, "ancestors": out}))
        }
        "descendants" => {
            let id = arg_str(a, "id").ok_or("id is required")?; // i18n-ok: for agents
            let p = person_or_err(tree, id)?;
            let gens = arg_n(a, "generations", 3, 8);
            let limit = arg_n(a, "limit", 300, 1000);
            let mut seen: HashSet<&str> = HashSet::from([id]);
            let mut frontier = vec![id];
            let mut out = Vec::new();
            let mut total = 0;
            for g in 1..=gens {
                let mut next = Vec::new();
                for parent in frontier {
                    for k in children_of(tree, parent).filter_map(|k| tree.person(k)) {
                        if !seen.insert(k.id.as_str()) {
                            continue;
                        }
                        total += 1;
                        next.push(k.id.as_str());
                        if out.len() >= limit {
                            continue;
                        }
                        let mut v = row(k);
                        v["generation"] = json!(g);
                        v["parent"] = json!(parent);
                        if let Some(o) = k.famc.iter().filter_map(|f| tree.family(f)).find(|f| f.partners().any(|x| x == parent)).and_then(|f| f.other_partner(parent)) {
                            v["other_parent"] = json!(o);
                        }
                        out.push(v);
                    }
                }
                frontier = next;
            }
            let mut v = json!({"person": row(p), "count": total, "descendants": out});
            if total > limit {
                v["truncated"] = json!(true);
            }
            Ok(v)
        }
        "relationship" => relationship(tree, a, focus),
        "timeline" => timeline(tree, a),
        "data_quality" => data_quality(tree, a),
        _ => Err(format!("unknown tool {name}")), // i18n-ok: for agents
    }
}
