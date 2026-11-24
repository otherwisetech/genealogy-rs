//! The aggregate tools on `fixtures/names.ged` (invented people): Pedro Prueba (I1) + Rosa (I2) →
//! Juan (I3) + Elena García López (I5) → Luis (I7) + Carmen Garcia (I9) → Ana María (I10) + Raúl
//! (I17) → Juan José (I13) → Juan Carlos (I14, born before his father); and Juana (I4) + Marco de
//! la Cruz (I6) → Luisa (I8) + Tomás (I11) → José Luis (I12). Also Maria (I15, dies before
//! birth), Pedro (I16, entered twice) and MARÍA GARCÍA (I18, 150 years old).
use genmcp::Server;
use serde_json::{Value, json};
use std::path::Path;

const NAMES: &str = include_str!("fixtures/names.ged");

fn server(dir: &Path) -> Server {
    i18n::set_locale("en");
    let p = dir.join("tree.ged");
    std::fs::write(&p, NAMES).unwrap();
    Server::new(p, None, Some("I13".into()))
}

fn rpc(s: &mut Server, name: &str, args: Value) -> Value {
    s.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}})).unwrap()
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let r = rpc(s, name, args);
    assert_eq!(r["result"]["isError"], json!(false), "{name}: {r}");
    r["result"]["structuredContent"].clone()
}

fn ids(v: &Value) -> Vec<String> {
    let mut v: Vec<String> = v.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect();
    v.sort_by_key(|s| s[1..].parse::<u32>().unwrap_or(0));
    v
}

fn count_of(rows: &Value, name: &str) -> u64 {
    rows.as_array().unwrap().iter().find(|r| r["name"] == json!(name)).map(|r| r["count"].as_u64().unwrap()).unwrap_or(0)
}

#[test]
fn name_counts_are_exact() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let v = call(&mut s, "name_stats", json!({"top": 50}));
    assert_eq!(v["people"], json!(18));
    let g = &v["given_names"]["top"];
    assert_eq!(count_of(g, "Juan"), 3, "Juan, Juan José, Juan Carlos — never Juana");
    assert_eq!(count_of(g, "Juana"), 1);
    assert_eq!(count_of(g, "Luis"), 2, "Luis and José Luis — never Luisa");
    assert_eq!(count_of(g, "Luisa"), 1);
    assert_eq!(count_of(g, "José"), 2);
    assert_eq!(count_of(g, "Ana"), 1);
    assert_eq!(count_of(g, "María"), 3, "María, Maria and MARÍA are one name");
    assert_eq!(count_of(g, "Pedro"), 2);
    let maria = g.as_array().unwrap().iter().find(|r| r["name"] == json!("María")).unwrap();
    assert_eq!(maria["also_written"], json!(["Maria"]));
    assert_eq!(maria["female"], json!(3));
    assert_eq!(g[0]["name"], json!("Juan"), "most first, ties by name");
    assert_eq!(g[1]["name"], json!("María"));

    let sn = &v["surnames"]["top"];
    assert_eq!(count_of(sn, "Prueba"), 9);
    assert_eq!(count_of(sn, "García"), 5, "García López, Prueba García, Garcia, Prueba Garcia, GARCÍA");
    assert_eq!(count_of(sn, "de la Cruz"), 3, "the particle stays with the surname");
    assert_eq!(count_of(sn, "Ejemplo"), 3);
    assert_eq!(count_of(sn, "López"), 1);
    assert_eq!(sn[0]["name"], json!("Prueba"));

    let full = call(&mut s, "name_stats", json!({"kind": "given", "given_mode": "full", "top": 50}));
    let fg = &full["given_names"]["top"];
    assert_eq!(count_of(fg, "Juan"), 1);
    assert_eq!(count_of(fg, "Juan José"), 1);
    assert_eq!(count_of(fg, "Ana María"), 1);
    assert!(full.get("surnames").is_none());

    let first = call(&mut s, "name_stats", json!({"kind": "given", "given_mode": "first", "top": 50}));
    assert_eq!(count_of(&first["given_names"]["top"], "José"), 1, "only José Luis starts with José");
    assert_eq!(count_of(&first["given_names"]["top"], "Luis"), 1);

    let one = call(&mut s, "name_stats", json!({"name": "juan"}));
    assert_eq!(one["lookup"], json!({"name": "juan", "as_given_name": 3, "as_surname": 0}));

    let women = call(&mut s, "name_stats", json!({"sex": "F", "kind": "given", "top": 50}));
    assert_eq!(count_of(&women["given_names"]["top"], "Juan"), 0);
    let branch = call(&mut s, "name_stats", json!({"descendants_of": "I4", "kind": "given"}));
    assert_eq!(branch["people"], json!(3), "Juana, Luisa, José Luis");
    let c19 = call(&mut s, "name_stats", json!({"born_from": 1800, "born_to": 1899, "kind": "given", "top": 50}));
    assert_eq!(count_of(&c19["given_names"]["top"], "Juan"), 1);
    let here = call(&mut s, "name_stats", json!({"place": "tunja", "place_event": "birth", "kind": "given", "top": 50}));
    assert_eq!(here["people"], json!(3), "Juan, Juana (Boyaca without accent) and Luis");
}

#[test]
fn search_matches_whole_words() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let juan = call(&mut s, "search_people", json!({"query": "Juan"}));
    assert_eq!(ids(&juan["people"]), ["I3", "I13", "I14"]);
    assert_eq!(juan["total"], json!(3));
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "luis"}))["people"]), ["I7", "I12"]);
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "Juan", "match": "prefix"}))["people"]), ["I3", "I4", "I13", "I14"]);
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "uan", "match": "substring"}))["people"]), ["I3", "I4", "I13", "I14"]);
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "Juan", "match": "exact"}))["people"]), ["I3"]);
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "juan jose prueba"}))["people"]), ["I13"]);
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "maria"}))["people"]), ["I10", "I15", "I18"], "accents folded");
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "María", "accent_sensitive": true}))["people"]), ["I10", "I18"], "case still folded; Maria has no accent");
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "Luis", "fields": "surname"}))["people"]), Vec::<String>::new());
    assert_eq!(ids(&call(&mut s, "search_people", json!({"query": "tunja", "fields": "all"}))["people"]).len(), 4, "places with fields all (I1 died there)");
    let page = call(&mut s, "search_people", json!({"query": "prueba", "fields": "surname", "limit": 4}));
    assert_eq!(page["total"], json!(9));
    assert_eq!(page["people"].as_array().unwrap().len(), 4);
    assert_eq!(page["next_offset"], json!(4));
    let last = call(&mut s, "search_people", json!({"query": "prueba", "fields": "surname", "limit": 4, "offset": 8}));
    assert_eq!(last["people"].as_array().unwrap().len(), 1);
    assert!(last.get("next_offset").is_none());
    let men = call(&mut s, "search_people", json!({"query": "pedro", "born_from": 1815}));
    assert_eq!(ids(&men["people"]), ["I1", "I16"], "ABT 1820 has a year too");
}

#[test]
fn relationships_of_several_degrees() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let rel = |s: &mut Server, a: &str, b: &str| call(s, "relationship", json!({"a": a, "b": b}));
    let r = rel(&mut s, "I7", "I8");
    assert_eq!((r["generations_up"].as_u64(), r["generations_down"].as_u64()), (Some(2), Some(2)));
    assert_eq!(r["label"], json!("first cousin"));
    let r = rel(&mut s, "I10", "I12");
    assert_eq!(r["label"], json!("second cousin"));
    let r = rel(&mut s, "I12", "I13");
    assert_eq!((r["generations_up"].as_u64(), r["generations_down"].as_u64()), (Some(3), Some(4)));
    assert!(r["label"].as_str().unwrap().starts_with("second cousin") && r["label"].as_str().unwrap().contains("removed"), "{r}");
    let path = r["path"].as_array().unwrap();
    assert_eq!(path.first().unwrap()["id"], json!("I12"));
    assert_eq!(path.last().unwrap()["id"], json!("I13"));
    assert_eq!(path.len(), 8, "3 up, 4 down");
    assert_eq!(path[1]["step"], json!("parent"));
    assert_eq!(path[7]["step"], json!("child"));
    let mut common = ids(&r["common_ancestors"]);
    common.sort();
    assert_eq!(common, ["I1", "I2"]);
    let r = rel(&mut s, "I13", "I8");
    assert!(r["label"].as_str().unwrap().starts_with("first cousin") && r["label"].as_str().unwrap().contains("removed"), "{r}");
    assert_eq!((r["generations_up"].as_u64(), r["generations_down"].as_u64()), (Some(4), Some(2)));
    assert_eq!(r["path"][1]["relation_to_previous"], json!("mother"));
    let r = rel(&mut s, "I10", "I3");
    assert_eq!(r["label"], json!("grandfather"));
    let r = call(&mut s, "relationship", json!({"b": "I17"}));
    assert_eq!(r["kind"], json!("blood"), "Raúl is the father of the person on screen (I13)");
    let r = rel(&mut s, "I12", "I11");
    assert_eq!(r["label"], json!("father"));
    let r = rel(&mut s, "I7", "I16");
    assert_eq!(r["related"], json!(false));
}

#[test]
fn tree_stats_describe_the_tree() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let v = call(&mut s, "tree_stats", json!({}));
    assert_eq!(v["people"], json!(18));
    assert_eq!(v["sex"], json!({"F": 8, "M": 10, "U": 0}));
    assert_eq!(v["families"]["count"], json!(7));
    assert_eq!(v["families"]["with_children"], json!(7));
    assert_eq!(v["families"]["largest"]["children"], json!(2));
    assert_eq!(v["generations"]["deepest_line"], json!(6), "Pedro → Juan → Luis → Ana María → Juan José → Juan Carlos");
    assert_eq!(v["generations"]["deepest_line_ends_with"]["id"], json!("I14"));
    assert_eq!(v["living"]["presumed_living"], json!(1), "only Juan José (1950)");
    assert_eq!(v["living"]["unknown"], json!(2), "Tomás and Raúl have no dates");
    assert_eq!(v["lifespan"]["known"], json!(3), "150 and −10 years are left out");
    assert_eq!(v["lifespan"]["oldest"]["age"], json!(70));
    assert_eq!(v["lifespan"]["median_years"], json!(65));
    assert_eq!(v["years"], json!([1700, 1950]));
    let births = v["births"].as_array().unwrap();
    assert_eq!(births.iter().find(|b| b["period"] == json!("1800s")).unwrap()["count"], json!(10));
    assert_eq!(v["top_birth_places"][0], json!({"place": "Bogotá, Cundinamarca, Colombia", "people": 4}));
    assert_eq!(v["top_birth_places"][1], json!({"place": "Tunja, Boyacá, Colombia", "people": 3}), "Boyaca and Boyacá are one place");
    assert!(v["completeness_percent"]["birth_date"].as_f64().unwrap() > 80.);
    let dec = call(&mut s, "tree_stats", json!({"descendants_of": "I4", "period": "decade"}));
    assert_eq!(dec["people"], json!(3));
    assert_eq!(dec["filtered_from"], json!(18));
    assert_eq!(dec["births"][0], json!({"period": "1850s", "count": 1}));
}

#[test]
fn place_stats_group_and_climb_the_hierarchy() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let births = call(&mut s, "place_stats", json!({"event": "birth"}));
    assert_eq!(births["places"][0]["place"], json!("Bogotá, Cundinamarca, Colombia"));
    assert_eq!(births["places"][0]["people"], json!(4));
    assert_eq!(births["places"][0]["years"], json!([1855, 1950]));
    assert_eq!(births["by_last_part"][0], json!({"place": "Colombia", "people": 7}));
    let countries = call(&mut s, "place_stats", json!({"event": "birth", "level": 1}));
    assert_eq!(countries["places"].as_array().unwrap().iter().map(|p| (p["place"].as_str().unwrap(), p["people"].as_u64().unwrap())).collect::<Vec<_>>(), [("Colombia", 7), ("España", 2)]);
    let marr = call(&mut s, "place_stats", json!({"event": "marriage"}));
    assert_eq!(marr["events"], json!(4), "two marriages, both partners");
    assert_eq!(marr["places"][0]["people"], json!(2));
    let all = call(&mut s, "place_stats", json!({"within": "Bogotá"}));
    let bog = &all["places"][0];
    assert_eq!(bog["by_event"], json!({"birth": 4, "death": 1, "residence": 1}));
    let periods = call(&mut s, "place_stats", json!({"event": "birth", "level": 1, "by_period": true}));
    let c19 = periods["by_century"].as_array().unwrap().iter().find(|c| c["period"] == json!("1800s")).unwrap();
    assert_eq!(c19["top"][0], json!({"place": "Colombia", "events": 5}));
    assert_eq!(c19["top"][1], json!({"place": "España", "events": 2}));
}

#[test]
fn find_people_answers_ad_hoc_questions() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let v = call(&mut s, "find_people", json!({"sex": "F", "born_to": 1900, "missing": ["death"]}));
    assert_eq!(ids(&v["people"]), ["I4", "I5", "I8", "I9"]);
    let v = call(&mut s, "find_people", json!({"descendants_of": "I1", "count_by": "generation"}));
    assert_eq!(v["groups"][0], json!({"value": "0", "count": 1}));
    assert_eq!(v["groups"][5], json!({"value": "5", "count": 1}));
    let v = call(&mut s, "find_people", json!({"given": "luis"}));
    assert_eq!(ids(&v["people"]), ["I7", "I12"]);
    let v = call(&mut s, "find_people", json!({"count_by": "birth_century"}));
    assert_eq!(v["groups"][0], json!({"value": "1700s", "count": 1}));
    assert_eq!(v["without_value"], json!(2));
    let v = call(&mut s, "find_people", json!({"age_at_death_min": 65, "sort": "age_at_death"}));
    assert_eq!(ids(&v["people"]), ["I2", "I3", "I18"]);
    assert_eq!(v["people"][0]["id"], json!("I18"), "oldest first");
    let v = call(&mut s, "find_people", json!({"living": true}));
    assert_eq!(ids(&v["people"]), ["I13"]);
    let v = call(&mut s, "find_people", json!({"relatives_of": "I12", "has": ["birth_place"], "count_by": "birth_country"}));
    assert_eq!(v["groups"], json!([{"value": "Colombia", "count": 6}, {"value": "España", "count": 2}]));
    let v = call(&mut s, "find_people", json!({"descendants_of": "I7", "include_partners": true}));
    assert_eq!(ids(&v["people"]), ["I7", "I9", "I10", "I13", "I14", "I17"]);
    let bad = rpc(&mut s, "find_people", json!({"sql": "select *"}));
    assert_eq!(bad["result"]["isError"], json!(true));
    assert!(bad["result"]["content"][0]["text"].as_str().unwrap().contains("born_from"), "lists what it takes");
    assert_eq!(rpc(&mut s, "find_people", json!({"has": ["shoe_size"]}))["result"]["isError"], json!(true));
}

#[test]
fn timeline_in_date_order() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let v = call(&mut s, "timeline", json!({"id": "I3"}));
    let tags: Vec<&str> = v["events"].as_array().unwrap().iter().map(|e| e["tag"].as_str().unwrap()).collect();
    assert_eq!(tags, ["BIRT", "OCCU", "MARR", "RESI", "DEAT"]);
    assert_eq!(v["events"][2]["with"]["id"], json!("I5"));
    let fam = call(&mut s, "timeline", json!({"id": "I1", "scope": "descendants", "generations": 1}));
    let years: Vec<i64> = fam["events"].as_array().unwrap().iter().map(|e| e["year"].as_i64().unwrap()).collect();
    assert!(years.windows(2).all(|w| w[0] <= w[1]), "{years:?}");
    let marriages = fam["events"].as_array().unwrap().iter().filter(|e| e["tag"] == json!("MARR") && e["year"] == json!(1848)).count();
    assert_eq!(marriages, 1, "a couple's marriage is listed once");
    let window = call(&mut s, "timeline", json!({"id": "I1", "scope": "descendants", "from": 1870, "to": 1890}));
    assert!(window["events"].as_array().unwrap().iter().all(|e| (1870..=1890).contains(&e["year"].as_i64().unwrap())));
}

#[test]
fn data_quality_finds_the_planted_problems() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    let sum = call(&mut s, "data_quality", json!({}));
    let kinds: Vec<&str> = sum["by_kind"].as_array().unwrap().iter().map(|k| k["kind"].as_str().unwrap()).collect();
    for k in ["death_before_birth", "born_before_father", "very_old", "possible_duplicate", "isolated"] {
        assert!(kinds.contains(&k), "{k} in {kinds:?}");
    }
    assert!(sum["by_severity"]["error"].as_u64().unwrap() >= 2);
    assert!(sum["worst"].as_array().unwrap().iter().take(2).all(|i| i["severity"] == json!("error")));
    assert!(sum["people_missing"]["birth_place"].as_u64().unwrap() > 0);
    let dates = call(&mut s, "data_quality", json!({"check": "dates"}));
    let who: Vec<(&str, &str)> = dates["issues"].as_array().unwrap().iter().map(|i| (i["kind"].as_str().unwrap(), i["people"][0]["id"].as_str().unwrap())).collect();
    assert!(who.contains(&("death_before_birth", "I15")), "{who:?}");
    assert!(who.contains(&("born_before_father", "I14")), "{who:?}");
    let ages = call(&mut s, "data_quality", json!({"check": "ages"}));
    assert!(ages["issues"].as_array().unwrap().iter().any(|i| i["kind"] == json!("very_old") && i["people"][0]["id"] == json!("I18")));
    let dups = call(&mut s, "data_quality", json!({"check": "duplicates"}));
    let pair = &dups["pairs"][0];
    let mut got = [pair["a"]["id"].as_str().unwrap(), pair["b"]["id"].as_str().unwrap()];
    got.sort();
    assert_eq!(got, ["I1", "I16"]);
    let one = call(&mut s, "data_quality", json!({"check": "duplicates", "id": "I1"}));
    assert_eq!(one["candidates"][0]["person"]["id"], json!("I16"));
    let gaps = call(&mut s, "data_quality", json!({"check": "missing", "missing": "parents"}));
    assert!(gaps["people"].as_array().unwrap().iter().all(|p| p["missing"].as_array().unwrap().contains(&json!("parents"))));
    assert!(gaps["total"].as_u64().unwrap() >= 5);
}

#[test]
fn repeated_single_name_searches_get_a_hint() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path());
    for q in ["Juan", "Luis"] {
        assert!(call(&mut s, "search_people", json!({"query": q})).get("hint").is_none());
    }
    let third = call(&mut s, "search_people", json!({"query": "Pedro"}));
    assert!(third["hint"].as_str().unwrap().contains("name_stats"));
    let r = rpc(&mut s, "search_people", json!({"query": "Rosa"}));
    assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("name_stats"), "the text the model reads has it too");
}

#[test]
fn tool_list_is_valid() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("tree.ged");
    std::fs::write(&p, NAMES).unwrap();
    let mut s = Server::new(p, Some(d.path().join("proposals")), Some("I13".into()));
    let list = s.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/list"})).unwrap();
    let tools = list["result"]["tools"].as_array().unwrap();
    let mut names = std::collections::HashSet::new();
    for t in tools {
        let name = t["name"].as_str().unwrap();
        assert!(names.insert(name), "{name} listed twice");
        assert!(!name.is_empty() && name.len() <= 64 && name.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{name}");
        let desc = t["description"].as_str().unwrap();
        assert!(desc.len() > 40 && desc.len() < 1500, "{name}: description length {}", desc.len());
        let schema = &t["inputSchema"];
        assert_eq!(schema["type"], json!("object"), "{name}");
        let props = schema["properties"].as_object().unwrap_or_else(|| panic!("{name}: properties"));
        for r in schema["required"].as_array().unwrap() {
            assert!(props.contains_key(r.as_str().unwrap()), "{name}: required {r} is not a property");
        }
        for (k, p) in props {
            let ty = p["type"].as_str().unwrap_or_else(|| panic!("{name}.{k}: no type"));
            assert!(["string", "integer", "boolean", "array", "object", "number"].contains(&ty), "{name}.{k}: {ty}");
            if let Some(e) = p.get("enum") {
                assert!(!e.as_array().unwrap().is_empty(), "{name}.{k}: empty enum");
            }
            if ty == "array" {
                assert!(p.get("items").is_some(), "{name}.{k}: array without items");
            }
        }
        assert_eq!(t["annotations"]["readOnlyHint"], json!(!name.starts_with("propose_")), "{name}");
        // read tools answer (or refuse cleanly) with their required arguments only
        if !name.starts_with("propose_") {
            let mut args = json!({});
            for r in schema["required"].as_array().unwrap() {
                args[r.as_str().unwrap()] = json!("I3");
            }
            let r = rpc(&mut s, name, args);
            assert_eq!(r["result"]["isError"], json!(false), "{name}: {r}");
            let text = r["result"]["content"][0]["text"].as_str().unwrap();
            assert!(text.len() < 20_000, "{name}: {} bytes", text.len());
        }
    }
    for want in ["name_stats", "tree_stats", "place_stats", "find_people", "search_people", "relationship", "timeline", "data_quality"] {
        assert!(names.contains(want), "{want} missing");
    }
    let init = s.handle(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {}})).unwrap();
    let help = init["result"]["instructions"].as_str().unwrap();
    for t in ["name_stats", "place_stats", "relationship", "data_quality", "find_people"] {
        assert!(help.contains(t), "instructions mention {t}");
    }
}

/// Thousands of people: every aggregate stays well under a second in a release build and the
/// replies stay small.
#[test]
fn large_trees_stay_fast_and_compact() {
    let given = ["Juan", "Juana", "Luis", "Luisa", "Ana María", "José", "María", "Pedro", "Rosa", "Elena"];
    let surnames = ["Prueba", "Ejemplo", "García", "de la Cruz", "Muestra", "Ensayo"];
    let places = ["Tunja, Boyacá, Colombia", "Bogotá, Cundinamarca, Colombia", "Sevilla, Andalucía, España", "Lima, Perú"];
    // written as GEDCOM directly: Tree edits reindex on every call
    let n = 4000;
    let mut g = String::from("0 HEAD\n1 CHAR UTF-8\n");
    let mut fams = String::new();
    let parent = |c: usize| c - 20 - (c % 20);
    let mut kids = vec![Vec::new(); n];
    for c in 40..n {
        kids[parent(c)].push(c);
    }
    for i in 0..n {
        let sex = if i % 2 == 0 { "M" } else { "F" };
        let surname = format!("{} {}", surnames[i % surnames.len()], surnames[(i / 7) % surnames.len()]);
        g += &format!("0 @I{i}@ INDI\n1 NAME {} /{surname}/\n1 SEX {sex}\n1 BIRT\n2 DATE {}\n2 PLAC {}\n", given[i % given.len()], 1700 + i / 20, places[i % places.len()]);
        // each person after the first 40 is the child of someone 20 to 40 places before
        if i >= 40 {
            g += &format!("1 FAMC @F{i}@\n");
            fams += &format!("0 @F{i}@ FAM\n1 {} @I{}@\n1 CHIL @I{i}@\n", if parent(i) % 2 == 0 { "HUSB" } else { "WIFE" }, parent(i));
        }
        for c in &kids[i] {
            g += &format!("1 FAMS @F{c}@\n");
        }
    }
    g += &fams;
    g += "0 TRLR\n";
    let t = gedcom::Tree::parse(&g).unwrap();
    assert_eq!(t.people.len(), n);
    let ids: Vec<String> = (0..n).map(|i| format!("I{i}")).collect();
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("tree.ged");
    std::fs::write(&p, t.to_gedcom()).unwrap();
    let mut s = Server::new(p, None, None);
    i18n::set_locale("en");
    call(&mut s, "tree_stats", json!({}));
    let start = std::time::Instant::now();
    for (tool, args) in [
        ("name_stats", json!({})),
        ("tree_stats", json!({})),
        ("place_stats", json!({"event": "birth", "level": 1})),
        ("find_people", json!({"descendants_of": ids[50], "count_by": "generation"})),
        ("search_people", json!({"query": "juan"})),
        ("relationship", json!({"a": ids[3990], "b": ids[3900]})),
        ("timeline", json!({"id": ids[45], "scope": "descendants", "generations": 3})),
        ("data_quality", json!({})),
        ("find_people", json!({"missing": ["death"], "born_to": 1750, "count_by": "surname"})),
    ] {
        let r = rpc(&mut s, tool, args);
        assert_eq!(r["result"]["isError"], json!(false), "{tool}: {r}");
        let text = r["result"]["content"][0]["text"].as_str().unwrap();
        assert!(text.len() < 30_000, "{tool}: {} bytes", text.len());
    }
    let v = call(&mut s, "name_stats", json!({"kind": "given"}));
    assert_eq!(count_of(&v["given_names"]["top"], "Juan"), 400);
    assert_eq!(count_of(&v["given_names"]["top"], "María"), 800, "María and Ana María");
    let limit = if cfg!(debug_assertions) { 30 } else { 5 };
    assert!(start.elapsed().as_secs() < limit, "{:?}", start.elapsed());
}
