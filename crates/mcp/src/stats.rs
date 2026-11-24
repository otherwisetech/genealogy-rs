//! Aggregate tools: exact counts over the whole tree (or a filtered part of it) in one call, so
//! the model never has to guess names and search them one by one.
use crate::query::{EventKind, Filter, Life, Spellings, arg_bool, arg_i32, arg_str, events_of, given_parts, life, place_matches, row, surname_parts};
use gedcom::{Missing, Person, Sex, Tree, fold, place_key, year_of};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

pub fn arg_n(a: &Value, k: &str, default: u64, max: u64) -> usize {
    a.get(k).and_then(Value::as_u64).unwrap_or(default).clamp(1, max) as usize
}

#[derive(Default)]
struct NameCount {
    people: usize,
    male: usize,
    female: usize,
    spellings: Spellings,
}

fn top_names(counts: HashMap<String, NameCount>, top: usize, with_sex: bool) -> (usize, Vec<Value>) {
    let distinct = counts.len();
    let mut v: Vec<(String, NameCount)> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.people.cmp(&a.1.people).then(a.0.cmp(&b.0)));
    let rows = v
        .into_iter()
        .take(top)
        .map(|(_, c)| {
            let mut r = json!({"name": c.spellings.best(), "count": c.people});
            if with_sex {
                if c.male > 0 {
                    r["male"] = json!(c.male);
                }
                if c.female > 0 {
                    r["female"] = json!(c.female);
                }
            }
            let other = c.spellings.variants();
            if !other.is_empty() {
                r["also_written"] = json!(other);
            }
            r
        })
        .collect();
    (distinct, rows)
}

fn tally(map: &mut HashMap<String, NameCount>, seen: &mut HashSet<String>, written: &str, sex: Sex) {
    let key = fold(written);
    if key.is_empty() || !seen.insert(key.clone()) {
        return;
    }
    let c = map.entry(key).or_default();
    c.people += 1;
    match sex {
        Sex::Male => c.male += 1,
        Sex::Female => c.female += 1,
        Sex::Unknown => {}
    }
    c.spellings.add(written);
}

/// `name_stats`: how many people carry each given name and each surname.
pub fn name_stats(tree: &Tree, a: &Value) -> Result<Value, String> {
    let filter = Filter::parse(tree, a)?;
    let top = arg_n(a, "top", 20, 200);
    let kind = arg_str(a, "kind").unwrap_or("both");
    if !matches!(kind, "both" | "given" | "surname") {
        return Err("kind must be given, surname or both".into()); // i18n-ok: for agents
    }
    let given_mode = arg_str(a, "given_mode").unwrap_or("each");
    let surname_mode = arg_str(a, "surname_mode").unwrap_or("each");
    if !matches!(given_mode, "each" | "first" | "full") || !matches!(surname_mode, "each" | "first" | "full") {
        return Err("given_mode / surname_mode must be each, first or full".into()); // i18n-ok: for agents
    }
    let people = filter.people(tree);
    let (mut given, mut surname) = (HashMap::new(), HashMap::new());
    let (mut with_given, mut with_surname) = (0, 0);
    for p in &people {
        let g: Vec<String> = match given_mode {
            "full" => vec![p.given.split_whitespace().collect::<Vec<_>>().join(" ")],
            "first" => given_parts(&p.given).into_iter().take(1).map(str::to_string).collect(),
            _ => given_parts(&p.given).into_iter().map(str::to_string).collect(),
        };
        let s: Vec<String> = match surname_mode {
            "full" => vec![p.surname.split_whitespace().collect::<Vec<_>>().join(" ")],
            "first" => surname_parts(&p.surname).into_iter().take(1).collect(),
            _ => surname_parts(&p.surname),
        };
        let mut seen = HashSet::new();
        for w in g.iter().filter(|w| !w.is_empty()) {
            tally(&mut given, &mut seen, w, p.sex);
        }
        with_given += usize::from(!seen.is_empty());
        let mut seen = HashSet::new();
        for w in s.iter().filter(|w| !w.is_empty()) {
            tally(&mut surname, &mut seen, w, p.sex);
        }
        with_surname += usize::from(!seen.is_empty());
    }
    let mut v = json!({"people": people.len(), "given_mode": given_mode, "surname_mode": surname_mode});
    if let Some(n) = arg_str(a, "name") {
        let key = fold(n);
        let count = |m: &HashMap<String, NameCount>| m.get(&key).map(|c| c.people).unwrap_or(0);
        v["lookup"] = json!({"name": n, "as_given_name": count(&given), "as_surname": count(&surname)});
    }
    if kind != "surname" {
        let (distinct, rows) = top_names(given, top, true);
        v["given_names"] = json!({"people_with_one": with_given, "distinct": distinct, "top": rows});
    }
    if kind != "given" {
        let (distinct, rows) = top_names(surname, top, false);
        v["surnames"] = json!({"people_with_one": with_surname, "distinct": distinct, "top": rows});
    }
    Ok(v)
}

fn period_of(year: i32, century: bool) -> i32 {
    let size = if century { 100 } else { 10 };
    year.div_euclid(size) * size
}

fn period_counts(years: impl Iterator<Item = i32>, century: bool) -> Value {
    let mut m: BTreeMap<i32, usize> = BTreeMap::new();
    for y in years {
        *m.entry(period_of(y, century)).or_default() += 1;
    }
    Value::Array(m.into_iter().map(|(p, n)| json!({"period": format!("{p}s"), "count": n})).collect())
}

/// Most frequent places of `kind`, grouped by `place_key`, most common spelling shown.
fn top_places(tree: &Tree, people: &[&Person], kind: EventKind, top: usize) -> Vec<Value> {
    let mut m: HashMap<String, (HashSet<&str>, Spellings)> = HashMap::new();
    for p in people {
        for (_, _, pl) in events_of(tree, p, kind) {
            let Some(pl) = pl.map(str::trim).filter(|s| !s.is_empty()) else { continue };
            let e = m.entry(place_key(pl)).or_default();
            e.0.insert(p.id.as_str());
            e.1.add(pl);
        }
    }
    let mut v: Vec<(String, usize, String)> = m.into_iter().map(|(k, (ids, sp))| (k, ids.len(), sp.best())).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(top).map(|(_, n, name)| json!({"place": name, "people": n})).collect()
}

fn pct(n: usize, of: usize) -> f64 {
    if of == 0 { 0. } else { (n as f64 * 1000. / of as f64).round() / 10. }
}

/// Longest parent → child line among `set` (cycle-safe): generations, and its youngest person.
fn depth(tree: &Tree, set: &HashSet<&str>) -> (usize, Option<String>) {
    let mut memo: HashMap<&str, usize> = HashMap::new();
    fn go<'a>(tree: &'a Tree, set: &HashSet<&str>, id: &'a str, memo: &mut HashMap<&'a str, usize>, stack: &mut HashSet<&'a str>) -> usize {
        if let Some(d) = memo.get(id) {
            return *d;
        }
        if !stack.insert(id) {
            return 0;
        }
        let parents: Vec<&str> = tree.person(id).into_iter().flat_map(|p| p.famc.iter()).filter_map(|f| tree.family(f)).flat_map(|f| f.partners()).map(String::as_str).filter(|x| set.contains(x)).collect();
        let d = 1 + parents.into_iter().map(|x| go(tree, set, x, memo, stack)).max().unwrap_or(0);
        stack.remove(id);
        memo.insert(id, d);
        d
    }
    let mut best = (0, None);
    let mut ids: Vec<&str> = set.iter().copied().collect();
    ids.sort_unstable();
    for id in ids {
        let d = go(tree, set, id, &mut memo, &mut HashSet::new());
        if d > best.0 {
            best = (d, Some(id.to_string()));
        }
    }
    best
}

/// `tree_stats`: the shape of the tree (or of the filtered people) in one call.
pub fn tree_stats(tree: &Tree, a: &Value, focus: Option<&str>, file: &str) -> Result<Value, String> {
    let filter = Filter::parse(tree, a)?;
    let people = filter.people(tree);
    let n = people.len();
    let set: HashSet<&str> = people.iter().map(|p| p.id.as_str()).collect();
    let births: Vec<i32> = people.iter().filter_map(|p| p.birth_year()).collect();
    let deaths: Vec<i32> = people.iter().filter_map(|p| p.death_year()).collect();
    let span = births.iter().chain(&deaths).min().zip(births.iter().chain(&deaths).max());
    let century = match arg_str(a, "period") {
        Some("decade") => false,
        Some("century") => true,
        Some(_) => return Err("period must be decade or century".into()), // i18n-ok: for agents
        None => span.is_none_or(|(lo, hi)| hi - lo > 150),
    };
    let top = arg_n(a, "top_places", 5, 50);

    let mut sex = BTreeMap::from([("F", 0usize), ("M", 0), ("U", 0)]);
    let mut lifec = BTreeMap::from([("deceased", 0usize), ("presumed_living", 0), ("unknown", 0)]);
    let mut gaps: BTreeMap<&str, usize> = BTreeMap::new();
    let mut complete_sum = 0u64;
    let mut dead_or_presumed = 0;
    for p in &people {
        *sex.entry(p.sex.code()).or_default() += 1;
        let l = life(p);
        *lifec.entry(match l {
            Life::Deceased => "deceased",
            Life::Living => "presumed_living",
            Life::Unknown => "unknown",
        })
        .or_default() += 1;
        dead_or_presumed += usize::from(l == Life::Deceased);
        let c = tree.completeness(&p.id);
        complete_sum += c.percent as u64;
        for m in c.missing {
            *gaps.entry(crate::tools::missing_id(m)).or_default() += 1;
        }
    }
    let has = |f: &dyn Fn(&Person) -> bool| people.iter().filter(|p| f(p)).count();
    let filled = |s: &Option<String>| s.as_deref().is_some_and(|v| !v.trim().is_empty());
    let with_death_date = has(&|p| p.death.as_ref().is_some_and(|e| filled(&e.date)));
    let with_death_place = has(&|p| p.death.as_ref().is_some_and(|e| filled(&e.place)));
    let completeness = json!({
        "average_percent": if n > 0 { complete_sum / n as u64 } else { 0 },
        "birth_date": pct(has(&|p| p.birth.as_ref().is_some_and(|e| filled(&e.date))), n),
        "birth_place": pct(has(&|p| p.birth.as_ref().is_some_and(|e| filled(&e.place))), n),
        "death_date_of_deceased": pct(with_death_date, dead_or_presumed),
        "death_place_of_deceased": pct(with_death_place, dead_or_presumed),
        "parents": pct(n - gaps.get("parents").copied().unwrap_or(0), n),
        "partner": pct(n - gaps.get("partner").copied().unwrap_or(0), n),
        "sources": pct(n - gaps.get(crate::tools::missing_id(Missing::Sources)).copied().unwrap_or(0), n),
        "sex": pct(n - sex["U"], n),
    });

    // lifespans: both years known, 0..=120
    let mut ages: Vec<(i32, &Person)> = people.iter().filter_map(|p| Some((p.death_year()? - p.birth_year()?, *p))).filter(|(a, _)| (0..=120).contains(a)).collect();
    ages.sort_by_key(|(a, p)| (*a, p.id.clone()));
    let lifespan = if ages.is_empty() {
        json!({"known": 0})
    } else {
        let sum: i64 = ages.iter().map(|(a, _)| *a as i64).sum();
        let (oldest_age, oldest) = ages[ages.len() - 1];
        let mut by: BTreeMap<i32, (i64, usize)> = BTreeMap::new();
        for (age, p) in &ages {
            if let Some(b) = p.birth_year() {
                let e = by.entry(period_of(b, true)).or_default();
                e.0 += *age as i64;
                e.1 += 1;
            }
        }
        json!({
            "known": ages.len(),
            "average_years": ((sum as f64 / ages.len() as f64) * 10.).round() / 10.,
            "median_years": ages[ages.len() / 2].0,
            "oldest": {"age": oldest_age, "person": row(oldest)},
            "average_by_birth_century": by.into_iter().map(|(c, (s, k))| json!({"period": format!("{c}s"), "average_years": ((s as f64 / k as f64) * 10.).round() / 10., "people": k})).collect::<Vec<_>>(),
        })
    };

    // families with a partner or child in the set
    let fams: Vec<&gedcom::Family> = {
        let mut ids: Vec<&String> = tree.families.keys().collect();
        ids.sort();
        ids.into_iter().map(|id| &tree.families[id]).filter(|f| f.partners().chain(f.children.iter()).any(|x| set.contains(x.as_str()))).collect()
    };
    let mut sizes: BTreeMap<usize, usize> = BTreeMap::new();
    let mut kids_total = 0;
    let mut biggest: Option<&gedcom::Family> = None;
    for f in &fams {
        let k = f.children.iter().filter(|c| tree.person(c).is_some()).count();
        kids_total += k;
        *sizes.entry(k.min(10)).or_default() += 1;
        if biggest.is_none_or(|b| b.children.len() < k) {
            biggest = Some(f);
        }
    }
    let with_kids = fams.len() - sizes.get(&0).copied().unwrap_or(0);
    let mut families = json!({
        "count": fams.len(),
        "with_children": with_kids,
        "children_per_family": if fams.is_empty() { 0. } else { (kids_total as f64 * 100. / fams.len() as f64).round() / 100. },
        "children_per_family_with_children": if with_kids == 0 { 0. } else { (kids_total as f64 * 100. / with_kids as f64).round() / 100. },
        "families_by_number_of_children": sizes.iter().map(|(k, n)| json!({"children": if *k == 10 { "10+".to_string() } else { k.to_string() }, "families": n})).collect::<Vec<_>>(),
    });
    if let Some(b) = biggest.filter(|b| !b.children.is_empty()) {
        let parents: Vec<String> = b.partners().filter_map(|x| tree.person(x)).map(|p| p.name()).collect();
        families["largest"] = json!({"family": b.id, "parents": parents, "children": b.children.len()});
    }

    let (deepest, youngest) = depth(tree, &set);
    let roots = people.iter().filter(|p| !p.famc.iter().filter_map(|f| tree.family(f)).any(|f| f.partners().any(|x| set.contains(x.as_str())))).count();
    let mut generations = json!({"deepest_line": deepest, "people_without_parents_in_tree": roots});
    if let Some(y) = youngest.as_deref().and_then(|y| tree.person(y)) {
        generations["deepest_line_ends_with"] = row(y);
    }

    let mut v = json!({
        "file": file,
        "title": tree.title(),
        "people": n,
        "families": families,
        "sex": sex,
        "living": lifec,
        "births": period_counts(births.iter().copied(), century),
        "deaths": period_counts(deaths.iter().copied(), century),
        "lifespan": lifespan,
        "generations": generations,
        "top_birth_places": top_places(tree, &people, EventKind::Birth, top),
        "top_death_places": top_places(tree, &people, EventKind::Death, top),
        "completeness_percent": completeness,
        "people_missing": gaps,
    });
    if !filter.is_empty() {
        v["filtered_from"] = json!(tree.people.len());
    }
    if let Some((lo, hi)) = span {
        v["years"] = json!([lo, hi]);
    }
    if let Some(p) = focus.and_then(|f| tree.person(f)) {
        v["on_screen"] = row(p);
    }
    Ok(v)
}

/// The last `level` comma parts of a place («Tunja, Boyacá, Colombia», 1 → «Colombia»); 0 keeps
/// it whole.
fn place_at(place: &str, level: usize) -> String {
    let parts: Vec<&str> = place.split(',').map(str::trim).filter(|s| !s.is_empty()).collect();
    if level == 0 || level >= parts.len() { parts.join(", ") } else { parts[parts.len() - level..].join(", ") }
}

#[derive(Default)]
struct PlaceAgg<'a> {
    people: HashSet<&'a str>,
    events: usize,
    by_event: BTreeMap<&'static str, usize>,
    years: Option<(i32, i32)>,
    spellings: Spellings,
}

/// `place_stats`: where the people of the tree (or a branch) were born, married, lived and died.
pub fn place_stats(tree: &Tree, a: &Value) -> Result<Value, String> {
    let filter = Filter::parse(tree, a)?;
    let kind = arg_str(a, "event").map(EventKind::parse).transpose()?.unwrap_or(EventKind::Any);
    let level = arg_i32(a, "level")?.unwrap_or(0).clamp(0, 6) as usize;
    let within = arg_str(a, "within").map(place_key);
    let top = arg_n(a, "top", 20, 200);
    let offset = a.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let people = filter.people(tree);
    let mut groups: HashMap<String, PlaceAgg> = HashMap::new();
    let mut countries: HashMap<String, (HashSet<&str>, Spellings)> = HashMap::new();
    let (mut events_total, mut without_place) = (0, 0);
    let mut people_total: HashSet<&str> = HashSet::new();
    for p in &people {
        for (tag, date, pl) in events_of(tree, p, kind) {
            let Some(pl) = pl.map(str::trim).filter(|s| !s.is_empty()) else {
                without_place += 1;
                continue;
            };
            if within.as_deref().is_some_and(|w| !place_matches(pl, w)) {
                continue;
            }
            events_total += 1;
            people_total.insert(p.id.as_str());
            let shown = place_at(pl, level);
            let g = groups.entry(place_key(&shown)).or_default();
            g.people.insert(p.id.as_str());
            g.events += 1;
            *g.by_event.entry(EventKind::of_tag(tag)).or_default() += 1;
            g.spellings.add(&shown);
            if let Some(y) = date.and_then(year_of) {
                g.years = Some(g.years.map_or((y, y), |(lo, hi)| (lo.min(y), hi.max(y))));
            }
            let country = place_at(pl, 1);
            let c = countries.entry(place_key(&country)).or_default();
            c.0.insert(p.id.as_str());
            c.1.add(&country);
        }
    }
    let distinct = groups.len();
    let mut rows: Vec<(String, PlaceAgg)> = groups.into_iter().collect();
    rows.sort_by(|a, b| b.1.people.len().cmp(&a.1.people.len()).then(b.1.events.cmp(&a.1.events)).then(a.0.cmp(&b.0)));
    let page: Vec<Value> = rows
        .iter()
        .skip(offset)
        .take(top)
        .map(|(_, g)| {
            let mut r = json!({"place": g.spellings.best(), "people": g.people.len(), "events": g.events});
            if kind == EventKind::Any && g.by_event.len() > 1 {
                r["by_event"] = json!(g.by_event);
            }
            if let Some((lo, hi)) = g.years {
                r["years"] = json!([lo, hi]);
            }
            r
        })
        .collect();
    let mut v = json!({"event": kind.id(), "level": level, "people": people_total.len(), "events": events_total, "events_without_place": without_place, "distinct_places": distinct, "offset": offset, "places": page});
    if offset + top < distinct {
        v["next_offset"] = json!(offset + top);
    }
    if level != 1 && countries.len() > 1 {
        let mut c: Vec<(usize, String)> = countries.into_values().map(|(ids, sp)| (ids.len(), sp.best())).collect();
        c.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        v["by_last_part"] = c.into_iter().take(10).map(|(n, name)| json!({"place": name, "people": n})).collect();
    }
    if arg_bool(a, "by_period")?.unwrap_or(false) {
        // where each century's births / events were: the top 3 places per century
        let mut per: BTreeMap<i32, HashMap<String, (usize, Spellings)>> = BTreeMap::new();
        for p in &people {
            for (_, date, pl) in events_of(tree, p, kind) {
                let (Some(y), Some(pl)) = (date.and_then(year_of), pl.map(str::trim).filter(|s| !s.is_empty())) else { continue };
                if within.as_deref().is_some_and(|w| !place_matches(pl, w)) {
                    continue;
                }
                let shown = place_at(pl, level);
                let e = per.entry(period_of(y, true)).or_default().entry(place_key(&shown)).or_default();
                e.0 += 1;
                e.1.add(&shown);
            }
        }
        v["by_century"] = per
            .into_iter()
            .map(|(c, m)| {
                let mut r: Vec<(usize, String)> = m.into_values().map(|(n, sp)| (n, sp.best())).collect();
                r.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
                json!({"period": format!("{c}s"), "top": r.into_iter().take(3).map(|(n, name)| json!({"place": name, "events": n})).collect::<Vec<_>>()})
            })
            .collect();
    }
    Ok(v)
}
