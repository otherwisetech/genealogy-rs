//! On the synthetic `family.ged` fixture: Gaspar (I30) → Tomás (I10) + Inés (I11) → Rosa (I12,
//! with Mateo I13) and Andrés (I14, with Clara I15) → Simón (I20), Cruz (I21), Lucía (I22).
use genmcp::Server;
use genmcp::proposal::{self, Envelope, Proposal, Relation};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const FAMILY: &str = include_str!("../../gedcom/tests/fixtures/family.ged");

fn tree_file(dir: &Path, text: &str) -> PathBuf {
    let p = dir.join("tree.ged");
    std::fs::write(&p, text).unwrap();
    p
}

fn server(dir: &Path, propose: bool) -> Server {
    i18n::set_locale("en");
    Server::new(tree_file(dir, FAMILY), propose.then(|| dir.join("proposals")), Some("I20".into()))
}

fn call(s: &mut Server, name: &str, args: Value) -> Value {
    let r = s.handle(&json!({"jsonrpc": "2.0", "id": 1, "method": "tools/call", "params": {"name": name, "arguments": args}})).unwrap();
    assert_eq!(r["result"]["isError"], json!(false), "{name}: {r}");
    r["result"]["structuredContent"].clone()
}

fn ids(v: &Value) -> Vec<String> {
    v.as_array().unwrap().iter().map(|p| p["id"].as_str().unwrap().to_string()).collect()
}

#[test]
fn read_only_sessions_offer_no_proposals() {
    let d = tempfile::tempdir().unwrap();
    let names = |s: &Server| s.tools().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect::<Vec<_>>();
    let ro = names(&server(d.path(), false));
    assert!(ro.contains(&"ancestors".to_string()) && ro.contains(&"data_gaps".to_string()));
    assert!(!ro.iter().any(|n| n.starts_with("propose_")));
    let mut s = server(d.path(), false);
    let r = s.handle(&json!({"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {"name": "propose_merge", "arguments": {"keep": "I10", "duplicate": "I30"}}})).unwrap();
    assert_eq!(r["error"]["code"], json!(-32602));
    assert!(names(&server(d.path(), true)).contains(&"propose_add_person".to_string()));
}

#[test]
fn speaks_json_rpc() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), false);
    let init = s.handle(&json!({"jsonrpc": "2.0", "id": 0, "method": "initialize", "params": {"protocolVersion": "2025-06-18"}})).unwrap();
    assert_eq!(init["result"]["protocolVersion"], json!("2025-06-18"));
    assert_eq!(init["result"]["serverInfo"]["name"], json!("genealogy"));
    assert!(s.handle(&json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
    assert_eq!(s.handle(&json!({"jsonrpc": "2.0", "id": 3, "method": "nope"})).unwrap()["error"]["code"], json!(-32601));
    let bad = s.handle(&json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "get_person", "arguments": {"id": "I404"}}})).unwrap();
    assert_eq!(bad["result"]["isError"], json!(true));
}

#[test]
fn search_and_person_with_relatives() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), false);
    let hits = call(&mut s, "search_people", json!({"query": "valdes"}));
    let mut found = ids(&hits["people"]);
    found.sort();
    assert_eq!(found, ["I13", "I20", "I21"]);
    let rosa = call(&mut s, "get_person", json!({"id": "I12"}));
    let mut parents = ids(&rosa["parents"]);
    parents.sort();
    assert_eq!(parents, ["I10", "I11"]);
    assert_eq!(ids(&rosa["siblings"]), ["I14"]);
    assert_eq!(rosa["unions"][0]["partner"]["id"], json!("I13"));
    assert_eq!(ids(&rosa["unions"][0]["children"]).len(), 2);
    assert!(rosa["relationship_to_person_on_screen"].as_str().is_some(), "Rosa is Simón's mother");
    let summary = call(&mut s, "tree_summary", json!({}));
    assert_eq!(summary["people"], json!(11));
    assert_eq!(summary["on_screen"]["id"], json!("I20"));
}

#[test]
fn ancestors_and_descendants_by_generation() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), false);
    let a = call(&mut s, "ancestors", json!({"id": "I20", "generations": 3}));
    let gaspar = a["ancestors"].as_array().unwrap().iter().find(|v| v["id"] == json!("I30")).expect("great-grandfather");
    assert_eq!(gaspar["generation"], json!(3));
    assert_eq!(gaspar["ahnentafel"], json!(12));
    assert_eq!(gaspar["path"], json!("mother → father → father"));
    assert_eq!(a["generations"][0], json!({"generation": 1, "known": 2, "possible": 2}));
    let two = call(&mut s, "ancestors", json!({"id": "I20", "generations": 2}));
    assert!(!ids(&two["ancestors"]).contains(&"I30".to_string()));

    let desc = call(&mut s, "descendants", json!({"id": "I30"}));
    let simon = desc["descendants"].as_array().unwrap().iter().find(|v| v["id"] == json!("I20")).unwrap();
    assert_eq!(simon["generation"], json!(3));
    assert_eq!(simon["parent"], json!("I12"));
    assert_eq!(desc["count"], json!(6), "Tomás, Rosa, Andrés, Simón, Cruz, Lucía");
}

#[test]
fn relationship_gaps_and_consistency() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), false);
    let r = call(&mut s, "relationship", json!({"a": "I20", "b": "I30"}));
    assert_eq!(r["related"], json!(true));
    assert_eq!(r["generations_up"], json!(3));
    let gaps = call(&mut s, "data_gaps", json!({}));
    assert!(gaps["people_with_gaps"].as_u64().unwrap() > 0);
    assert!(gaps["people_missing"]["sources"].as_u64().is_some());
    let only = call(&mut s, "data_gaps", json!({"missing": "parents"}));
    assert!(only["people"].as_array().unwrap().iter().all(|p| p["missing"].as_array().unwrap().contains(&json!("parents"))));
    assert!(ids(&only["people"]).contains(&"I30".to_string()));
    let c = call(&mut s, "check_consistency", json!({}));
    assert!(c["issues"].as_array().unwrap().iter().all(|i| i["message"].as_str().is_some_and(|m| !m.is_empty())));
}

#[test]
fn finds_a_person_entered_twice() {
    let d = tempfile::tempdir().unwrap();
    let mut t = gedcom::Tree::parse(FAMILY).unwrap();
    t.set_event("I10", "BIRT", "1890", "").unwrap();
    let dup = t.add_person("Tomas", "Ordonez", gedcom::Sex::Male);
    t.set_event(&dup, "BIRT", "ABT 1890", "").unwrap();
    let mut s = Server::new(tree_file(d.path(), &t.to_gedcom()), None, None);
    let all = call(&mut s, "find_duplicates", json!({}));
    let pair = &all["pairs"][0];
    let mut got = vec![pair["a"]["id"].as_str().unwrap(), pair["b"]["id"].as_str().unwrap()];
    got.sort();
    assert_eq!(got, ["I10", dup.as_str()]);
    let one = call(&mut s, "find_duplicates", json!({"id": "I10"}));
    assert_eq!(one["candidates"][0]["person"]["id"], json!(dup));
}

#[test]
fn reloads_the_tree_when_the_file_changes() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), false);
    assert_eq!(call(&mut s, "tree_summary", json!({}))["people"], json!(11));
    let mut t = gedcom::Tree::parse(FAMILY).unwrap();
    t.add_person("Nueva", "Persona", gedcom::Sex::Female);
    std::fs::write(d.path().join("tree.ged"), t.to_gedcom()).unwrap();
    assert_eq!(call(&mut s, "tree_summary", json!({}))["people"], json!(12));
}

#[test]
fn proposals_wait_in_the_folder_until_the_app_applies_them() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), true);
    let before = std::fs::read_to_string(d.path().join("tree.ged")).unwrap();
    let r = call(&mut s, "propose_add_person", json!({"given": "Pía", "surname": "Ordóñez", "sex": "f", "birth_date": "12/3/1952", "relative": "I22", "relation": "child", "reason": "Named in a letter"}));
    assert_eq!(r["status"], json!("awaiting_approval"));
    assert_eq!(std::fs::read_to_string(d.path().join("tree.ged")).unwrap(), before, "the server never writes the tree");
    let got = proposal::collect(&d.path().join("proposals"));
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].reason, "Named in a letter");
    assert!(proposal::collect(&d.path().join("proposals")).is_empty(), "collected once");
    let Proposal::AddPerson { birth_date, sex, .. } = &got[0].proposal else { panic!() };
    assert_eq!(birth_date.as_deref(), Some("12 MAR 1952"));
    assert_eq!(sex.as_deref(), Some("F"));

    let mut t = gedcom::Tree::parse(FAMILY).unwrap();
    let new = got[0].proposal.apply(&mut t).unwrap();
    assert_eq!(t.parents(&new).iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["I22"]);
    assert_eq!(t.person(&new).unwrap().birth.as_ref().unwrap().date.as_deref(), Some("12 MAR 1952"));
}

#[test]
fn bad_proposals_are_refused_and_never_delivered() {
    let d = tempfile::tempdir().unwrap();
    let mut s = server(d.path(), true);
    for (tool, args) in [
        ("propose_add_relationship", json!({"person": "I404", "relative": "I10", "relation": "parent"})),
        ("propose_add_relationship", json!({"person": "I10", "relative": "I10", "relation": "partner"})),
        ("propose_set_event", json!({"person": "I10", "event": "OCCU", "date": "1900"})),
        ("propose_set_event", json!({"person": "I10", "event": "BIRT"})),
        ("propose_add_person", json!({"given": " ", "surname": ""})),
        ("propose_merge", json!({"keep": "I12", "duplicate": "I20"})),
    ] {
        let r = s.handle(&json!({"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {"name": tool, "arguments": args}})).unwrap();
        assert_eq!(r["result"]["isError"], json!(true), "{tool} {args}");
    }
    assert!(proposal::collect(&d.path().join("proposals")).is_empty());
}

#[test]
fn events_keep_what_the_proposal_leaves_out() {
    let mut t = gedcom::Tree::parse(FAMILY).unwrap();
    t.set_event("I10", "BIRT", "1890", "Toledo").unwrap();
    let date_only = Proposal::SetEvent { person: "I10".into(), event: "birt".into(), date: Some("3/4/1891".into()), place: None }.normalized();
    date_only.apply(&mut t).unwrap();
    assert_eq!(proposal::current_event(&t, "I10", "BIRT"), ("3 APR 1891".into(), "Toledo".into()));
    let clear = Proposal::SetEvent { person: "I10".into(), event: "BIRT".into(), date: None, place: Some(String::new()) }.normalized();
    clear.apply(&mut t).unwrap();
    assert_eq!(proposal::current_event(&t, "I10", "BIRT"), ("3 APR 1891".into(), String::new()));

    let link = Proposal::AddRelationship { person: "I15".into(), relative: "I30".into(), relation: Relation::Child };
    link.apply(&mut t).unwrap();
    assert!(t.parents("I15").iter().any(|p| p.id == "I30"));
    let env = Envelope { proposal: link, reason: String::new() };
    assert_eq!(serde_json::from_str::<Envelope>(&serde_json::to_string(&env).unwrap()).unwrap(), env);
}

#[test]
fn ahnentafel_paths() {
    assert_eq!(genmcp::tools::ahnentafel_path(1), "");
    assert_eq!(genmcp::tools::ahnentafel_path(2), "father");
    assert_eq!(genmcp::tools::ahnentafel_path(7), "mother → mother");
    assert_eq!(genmcp::tools::ahnentafel_path(12), "mother → father → father");
}

#[test]
fn arguments() {
    let a = |v: &[&str]| Server::from_args(&v.iter().map(|s| s.to_string()).collect::<Vec<_>>());
    let s = a(&["--tree", "/t/x.ged", "--propose", "/t/p", "--focus", "I3"]).unwrap();
    assert_eq!(s.tree, PathBuf::from("/t/x.ged"));
    assert_eq!(s.propose, Some(PathBuf::from("/t/p")));
    assert_eq!(s.focus.as_deref(), Some("I3"));
    assert!(a(&["--propose", "/t/p"]).is_err());
    assert!(a(&["--tree"]).is_err());
}
