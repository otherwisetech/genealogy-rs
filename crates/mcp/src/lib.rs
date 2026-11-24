//! `genealogy-rs mcp`: a stdio MCP server (JSON-RPC 2.0, one message per line) over one family
//! tree, started by the app's AI panel for one session. It only reads: the tree file is loaded
//! again whenever it changes on disk (the app writes a snapshot of what it has open). With
//! `--propose <folder>` it also offers `propose_*` tools, which check the change and leave it in
//! that folder for the app to show as a card; nothing changes until the user accepts it there.
pub mod proposal;
pub mod query;
pub mod stats;
pub mod tools;

use gedcom::Tree;
use proposal::{Envelope, Proposal};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::collections::HashSet;
use std::time::{Duration, Instant, SystemTime};

pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The server's name inside the agent (`genealogy.search_people`).
pub const NAME: &str = "genealogy";

const HELP: &str = "The open family tree of genealogy-rs (GEDCOM). People have ids like I12; dates are GEDCOM phrases (\"12 MAR 1852\", \"ABT 1900\", \"BET 1840 AND 1850\").

Pick the tool that answers the whole question in one call; never loop a lookup tool over guessed values:
- most common / repeated names or surnames, \"how many Juans\" → name_stats (once; exact counts, \"Ana María\" counts for Ana and for María, accents folded)
- size and shape of the tree, men / women, living, births per century, lifespans, generations, children per family, completeness → tree_stats
- where the family comes from, places of birth / death / marriage / residence → place_stats (level 1 = country, by_period for change over time)
- how two people are related → relationship (computed from the tree, with the path)
- life events in order for a person, a couple or a branch → timeline
- \"who / how many …\" with conditions (born before 1900 in Tunja, women with no death date, descendants of I3 named María, people missing parents) → find_people with typed filters; count_by gives counts per group in the same call
- errors, missing data, duplicates → data_quality (check = summary first)
- a person's id from a name → search_people (whole-word match: \"Juan\" never returns \"Juana\"), then get_person / ancestors / descendants.
name_stats, tree_stats, place_stats and find_people take the same filters (sex, born_from / born_to, place, descendants_of / ancestors_of / relatives_of…) to work on part of the tree.

Only state facts the tools return; say when something is unknown. Family data is personal: quote only what the question needs."; // i18n-ok: for agents

struct Tool {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    propose: bool,
}

/// `{"type": "object"}` with `props` (plus the shared person filters when `filters`).
fn object(props: Value, required: &[&str], filters: bool) -> Value {
    let mut all = if filters { query::filter_schema() } else { json!({}) };
    if let (Some(p), Some(e)) = (all.as_object_mut(), props.as_object()) {
        p.extend(e.clone());
    }
    json!({"type": "object", "properties": all, "required": required, "additionalProperties": false})
}

fn id_schema(extra: Value) -> Value {
    let mut props = json!({"id": {"type": "string", "description": "person id, e.g. I12"}}); // i18n-ok: for agents
    if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    object(props, &["id"], false)
}

fn relation_schema() -> Value {
    json!({"type": "string", "enum": ["parent", "child", "partner"]})
}

fn paging(max: u64) -> Value {
    json!({"limit": {"type": "integer", "minimum": 1, "maximum": max}, "offset": {"type": "integer", "minimum": 0, "description": "skip this many rows (use next_offset from the previous page)"}}) // i18n-ok: for agents
}

fn merged(a: Value, b: Value) -> Value {
    let mut a = a;
    if let (Some(x), Some(y)) = (a.as_object_mut(), b.as_object()) {
        x.extend(y.clone());
    }
    a
}

const REASON: &str = "one short sentence for the user: why this change"; // i18n-ok: for agents

const TOOLS: &[Tool] = &[
    Tool { name: "tree_stats", description: "The whole tree (or the people matching the filters) in one call: people and families, sex, living / deceased, births and deaths per decade or century, lifespans (average, median, oldest), deepest generation line, children per family, top birth and death places, data completeness %. Start here for \"how big is the tree\", \"how many men and women\", \"how long did they live\".", schema: || object(json!({"period": {"type": "string", "enum": ["decade", "century"], "description": "default: decades when the tree spans ≤ 150 years"}, "top_places": {"type": "integer", "minimum": 1, "maximum": 50}}), &[], true), propose: false }, // i18n-ok: for agents
    Tool { name: "name_stats", description: "Exact frequencies of given names and surnames, counted separately, top N with counts (people, not mentions). \"Ana María\" counts toward Ana and María (given_mode full counts \"Ana María\" as written; first counts only the first name); surnames \"García López\" count for García and López (\"de la Cruz\" stays whole). Accents and case are folded for grouping (María = Maria); the most common spelling is shown, other spellings in also_written. With `name`: the exact count of that one name. For \"¿qué nombres se repiten más?\" call this once, never search_people per name. Filters (sex, born_from / born_to, place, descendants_of…) narrow it to part of the tree.", schema: || object(json!({"kind": {"type": "string", "enum": ["both", "given", "surname"]}, "given_mode": {"type": "string", "enum": ["each", "first", "full"]}, "surname_mode": {"type": "string", "enum": ["each", "first", "full"]}, "name": {"type": "string", "description": "count only this name (as given name and as surname)"}, "top": {"type": "integer", "minimum": 1, "maximum": 200}}), &[], true), propose: false }, // i18n-ok: for agents
    Tool { name: "place_stats", description: "People per place for births, deaths, marriages, residences or burials (`event`, default any), most people first, with years and a per-event breakdown. `level` groups by the last parts of the place (\"Tunja, Boyacá, Colombia\": 1 = Colombia, 2 = Boyacá, Colombia; 0 = as written); `within` keeps places inside one region; by_period adds the top places per century (where the family moved). For \"¿de dónde viene la familia?\" call this once with event birth (and level 1 or 2).", schema: || object(merged(json!({"event": {"type": "string", "enum": ["birth", "death", "marriage", "residence", "burial", "any"]}, "level": {"type": "integer", "minimum": 0, "maximum": 6}, "within": {"type": "string", "description": "only places containing these words, e.g. \"Boyacá\""}, "by_period": {"type": "boolean"}, "top": {"type": "integer", "minimum": 1, "maximum": 200}}), json!({"offset": {"type": "integer", "minimum": 0}})), &[], true), propose: false }, // i18n-ok: for agents
    Tool { name: "find_people", description: "Structured query with typed filters (all optional, combined with AND): name / given / surname (with match mode), sex, born_from / born_to, died_from / died_to, age_at_death_min / max, living, place (+ place_event), descendants_of / ancestors_of / relatives_of (+ max_generations, include_partners), has / missing fields. Returns total and a page of compact rows (sort, offset, limit). count_by (sex, birth_decade, birth_century, birth_place, birth_country, surname, given_name, generation, living…) returns counts per group instead, e.g. \"how many descendants of I3 per generation\" = descendants_of I3 + count_by generation. Use this for any ad-hoc \"who / how many\" question instead of many searches.", schema: || object(merged(json!({"name": {"type": "string", "description": "words of the full name (see `match`)"}, "sort": {"type": "string", "enum": ["name", "birth", "death", "age_at_death"]}, "descending": {"type": "boolean"}, "count_by": {"type": "string", "enum": tools::COUNT_BY}, "top": {"type": "integer", "minimum": 1, "maximum": 500, "description": "with count_by: how many groups"}}), paging(200)), &[], true), propose: false }, // i18n-ok: for agents
    Tool { name: "search_people", description: "Find people by name to get their ids. match: word (default; every query word is a whole word of the name: \"Juan\" finds \"Juan José Pérez\" but never \"Juana\"; \"Luis\" never \"Luisa\"), exact (the whole given name, surname or full name), prefix, substring. Accent- and case-insensitive unless accent_sensitive. fields: name (default), given, surname, or all (also years, places, id). Takes the same filters as find_people; returns total plus a page (offset / limit). To count or rank names use name_stats instead.", schema: || object(merged(json!({"query": {"type": "string"}, "fields": {"type": "string", "enum": ["name", "given", "surname", "all"]}, "sort": {"type": "string", "enum": ["name", "birth", "death", "age_at_death"]}}), paging(200)), &[], true), propose: false }, // i18n-ok: for agents
    Tool { name: "get_person", description: "One person: names, sex, every event with date and place, notes, parents, siblings, partners with their children, and what data is missing.", schema: || id_schema(json!({})), propose: false }, // i18n-ok: for agents
    Tool { name: "ancestors", description: "A person's ancestors up to `generations` back (default 4, max 10): generation 1 = parents, 2 = grandparents, 3 = great-grandparents. Each with its Ahnentafel number and the path (father/mother steps).", schema: || id_schema(json!({"generations": {"type": "integer", "minimum": 1, "maximum": 10}})), propose: false }, // i18n-ok: for agents
    Tool { name: "descendants", description: "A person's descendants down `generations` (default 3, max 8): generation 1 = children, 2 = grandchildren. Each with the parent they descend through; count is the total even when the list is cut at `limit`. For counts only, find_people descendants_of + count_by generation is cheaper.", schema: || id_schema(json!({"generations": {"type": "integer", "minimum": 1, "maximum": 8}, "limit": {"type": "integer", "minimum": 1, "maximum": 1000}})), propose: false }, // i18n-ok: for agents
    Tool { name: "relationship", description: "How b is related to a, computed from the tree: label in the user's language (\"primo segundo, con una generación de diferencia\", \"tía abuela\", \"suegro\"), a sentence, generations up / down to the closest common ancestors, and the path person by person (each step parent / child / partner and what that person is to the previous one). Also other relationships when there is pedigree collapse. `a` defaults to the person on screen.", schema: || object(json!({"a": {"type": "string", "description": "person id (default: the person on screen)"}, "b": {"type": "string", "description": "person id"}}), &["b"], false), propose: false }, // i18n-ok: for agents
    Tool { name: "timeline", description: "Life events in date order (births, baptisms, marriages, residences, occupations, deaths…) with date, place and who, for one person (scope person), their close family (family: parents, partners, children, siblings), or a branch (descendants / ancestors, `generations` deep, default 2). from / to limit the years; undated events are counted, and listed with include_undated.", schema: || object(merged(json!({"id": {"type": "string"}, "scope": {"type": "string", "enum": ["person", "family", "descendants", "ancestors"]}, "generations": {"type": "integer", "minimum": 1, "maximum": 8}, "from": {"type": "integer"}, "to": {"type": "integer"}, "include_undated": {"type": "boolean"}}), paging(500)), &["id"], false), propose: false }, // i18n-ok: for agents
    Tool { name: "data_quality", description: "What is wrong or missing, in one call. check summary (default): counts per problem kind and severity, missing-data counts, the worst cases and likely duplicates. Then list one category: duplicates (people entered twice, 0-1 score; with id: candidates for that person), dates (impossible dates: death before birth, child born before a parent or after a parent died, married before birth…), ages (suspicious ages: very old, mother too young, father too old…), structure (broken families, people with no links, missing names), missing (people with gaps, least complete first; `missing` picks one gap). `id` limits it to one person. Messages come in the user's language.", schema: || object(merged(json!({"check": {"type": "string", "enum": ["summary", "duplicates", "dates", "ages", "structure", "missing"]}, "severity": {"type": "string", "enum": ["error", "warning", "info"]}, "id": {"type": "string"}, "missing": {"type": "string", "enum": ["birth_date", "birth_place", "death_place", "parents", "partner", "sources"]}}), paging(500)), &[], false), propose: false }, // i18n-ok: for agents
    Tool { name: "propose_add_person", description: "Propose adding a person, optionally as parent / child / partner of someone in the tree (`relation` of `relative`). Dates may be loose (\"12/3/1852\", \"abt 1900\"). Shown to the user as a card; nothing changes until they accept.", schema: || object(json!({"given": {"type": "string"}, "surname": {"type": "string"}, "sex": {"type": "string", "enum": ["M", "F", "U"]}, "birth_date": {"type": "string"}, "birth_place": {"type": "string"}, "death_date": {"type": "string"}, "death_place": {"type": "string"}, "relative": {"type": "string", "description": "id of the person the new one is related to"}, "relation": relation_schema(), "reason": {"type": "string", "description": REASON}}), &[], false), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_add_relationship", description: "Propose linking two people already in the tree: `person` is the `relation` (parent, child or partner) of `relative`. Shown as a card the user accepts.", schema: || object(json!({"person": {"type": "string"}, "relative": {"type": "string"}, "relation": relation_schema(), "reason": {"type": "string", "description": REASON}}), &["person", "relative", "relation"], false), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_set_event", description: "Propose fixing the date and / or place of a person's birth (BIRT), christening (CHR), baptism (BAPM), death (DEAT), burial (BURI) or cremation (CREM). Leave out what should stay; an empty string clears it. Shown as a card the user accepts.", schema: || object(json!({"person": {"type": "string"}, "event": {"type": "string", "enum": proposal::EVENTS}, "date": {"type": "string"}, "place": {"type": "string"}, "reason": {"type": "string", "description": REASON}}), &["person", "event"], false), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_set_name", description: "Propose correcting a person's given name and surname. Shown as a card the user accepts.", schema: || object(json!({"person": {"type": "string"}, "given": {"type": "string"}, "surname": {"type": "string"}, "reason": {"type": "string", "description": REASON}}), &["person", "given", "surname"], false), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_merge", description: "Propose merging `duplicate` into `keep` (the same person entered twice; see data_quality check duplicates). Shown as a card the user accepts; they can undo it.", schema: || object(json!({"keep": {"type": "string"}, "duplicate": {"type": "string"}, "reason": {"type": "string", "description": REASON}}), &["keep", "duplicate"], false), propose: true }, // i18n-ok: for agents
];

/// Calls within this long count as one burst for the repeated-lookup hint.
const BURST: Duration = Duration::from_secs(180);
/// This many different one-word name lookups in a burst earn the hint.
const BURST_NAMES: usize = 3;
/// This many get_person calls in a burst earn the aggregate hint.
const BURST_PEOPLE: usize = 8;

/// Recent lookups, to notice the model counting by hand (one search per guessed name).
#[derive(Default)]
struct Guard {
    recent: Vec<(Instant, &'static str, String)>,
}

impl Guard {
    /// Records a call; returns a hint when the burst looks like hand counting.
    fn note(&mut self, tool: &str, a: &Value) -> Option<&'static str> {
        let now = Instant::now();
        self.recent.retain(|(t, _, _)| now.duration_since(*t) < BURST);
        let one_word = |k: &str| query::arg_str(a, k).filter(|v| v.split_whitespace().count() == 1).map(gedcom::fold);
        let (kind, key): (&'static str, Option<String>) = match tool {
            "search_people" => ("name", one_word("query").or_else(|| one_word("given")).or_else(|| one_word("surname"))),
            "find_people" => ("name", one_word("name").or_else(|| one_word("given")).or_else(|| one_word("surname"))),
            "get_person" => ("person", query::arg_str(a, "id").map(str::to_string)),
            _ => ("", None),
        };
        let key = key?;
        self.recent.push((now, kind, key));
        let distinct = |k: &str| self.recent.iter().filter(|(_, x, _)| *x == k).map(|(_, _, v)| v.as_str()).collect::<HashSet<_>>().len();
        match kind {
            "name" if distinct("name") >= BURST_NAMES => Some("You are looking up single names one at a time. To rank or count names call name_stats once (exact counts for every given name and surname; `name` gives one name's count). For other counts or lists use find_people with filters and count_by."), // i18n-ok: for agents
            "person" if distinct("person") >= BURST_PEOPLE => Some("Many get_person calls in a row: for counts, places, dates or lifespans across many people, tree_stats, place_stats, name_stats, timeline or find_people (count_by) answer in one call."), // i18n-ok: for agents
            _ => None,
        }
    }
}

fn ok(id: Value, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}

fn rpc_error(id: Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}})
}

pub struct Server {
    /// The tree to read (the app's snapshot of what it has open).
    pub tree: PathBuf,
    /// Where proposals go; `None`: read-only, no `propose_*` tools.
    pub propose: Option<PathBuf>,
    /// The person on screen when the session started.
    pub focus: Option<String>,
    cache: Option<(Option<SystemTime>, u64, Tree)>,
    guard: Guard,
}

impl Server {
    pub fn new(tree: PathBuf, propose: Option<PathBuf>, focus: Option<String>) -> Server {
        Server { tree, propose, focus, cache: None, guard: Guard::default() }
    }

    /// Parses `mcp --tree <file> [--propose <folder>] [--focus <id>] [--lang <code>]`.
    pub fn from_args(args: &[String]) -> Result<Server, String> {
        let (mut tree, mut propose, mut focus) = (None, None, None);
        let mut it = args.iter();
        while let Some(a) = it.next() {
            let mut val = || it.next().cloned().ok_or_else(|| format!("{a} needs a value")); // i18n-ok: command line
            match a.as_str() {
                "--tree" => tree = Some(PathBuf::from(val()?)),
                "--propose" => propose = Some(PathBuf::from(val()?)),
                "--focus" => focus = Some(val()?),
                "--lang" => {
                    i18n::set_locale(&val()?);
                }
                _ => return Err(format!("unknown argument {a}")), // i18n-ok: command line
            }
        }
        Ok(Server::new(tree.ok_or("--tree <file> is required")?, propose, focus)) // i18n-ok: command line
    }

    pub fn tools(&self) -> Vec<Value> {
        let on = self.propose.is_some();
        TOOLS.iter().filter(|t| on || !t.propose).map(|t| json!({"name": t.name, "description": t.description, "inputSchema": (t.schema)(), "annotations": {"readOnlyHint": !t.propose, "destructiveHint": false}})).collect()
    }

    fn load(&mut self) -> Result<&Tree, String> {
        let meta = std::fs::metadata(&self.tree).map_err(|e| format!("{}: {e}", self.tree.display()))?; // i18n-ok: path: error
        let stamp = (meta.modified().ok(), meta.len());
        if self.cache.as_ref().is_none_or(|(m, l, _)| (*m, *l) != stamp) {
            let tree = gedcom::load(&self.tree).map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
            self.cache = Some((stamp.0, stamp.1, tree));
        }
        Ok(&self.cache.as_ref().expect("loaded").2)
    }

    pub fn handle(&mut self, msg: &Value) -> Option<Value> {
        let id = msg.get("id").cloned()?;
        if msg.get("jsonrpc").and_then(Value::as_str) != Some("2.0") {
            return Some(rpc_error(id, -32600, "invalid request"));
        }
        let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
        let params = msg.get("params").cloned().unwrap_or(json!({}));
        Some(match method {
            "initialize" => {
                let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
                let version = PROTOCOL_VERSIONS.iter().find(|v| **v == asked).copied().unwrap_or(PROTOCOL_VERSIONS[0]);
                ok(id, json!({"protocolVersion": version, "capabilities": {"tools": {"listChanged": false}}, "serverInfo": {"name": NAME, "version": env!("CARGO_PKG_VERSION")}, "instructions": HELP}))
            }
            "ping" => ok(id, json!({})),
            "tools/list" => ok(id, json!({"tools": self.tools()})),
            "tools/call" => {
                let name = params.get("name").and_then(Value::as_str).unwrap_or("");
                let args = params.get("arguments").cloned().unwrap_or(json!({}));
                if !TOOLS.iter().any(|t| t.name == name && (self.propose.is_some() || !t.propose)) {
                    return Some(rpc_error(id, -32602, &format!("unknown tool {name:?}"))); // i18n-ok: protocol
                }
                match self.call(name, &args) {
                    Ok(mut v) => {
                        if let Some(hint) = self.guard.note(name, &args)
                            && v.is_object()
                        {
                            v["hint"] = json!(hint);
                        }
                        ok(id, json!({"content": [{"type": "text", "text": v.to_string()}], "structuredContent": v, "isError": false}))
                    }
                    Err(e) => ok(id, json!({"content": [{"type": "text", "text": e}], "isError": true})),
                }
            }
            _ => rpc_error(id, -32601, &format!("method not found: {method}")), // i18n-ok: protocol
        })
    }

    /// Arguments the tool doesn't take are refused with the list it does take, so a guessed
    /// parameter never silently changes nothing.
    fn check_args(name: &str, a: &Value) -> Result<(), String> {
        let Some(t) = TOOLS.iter().find(|t| t.name == name) else { return Ok(()) };
        if a.is_null() {
            return Ok(());
        }
        let schema = (t.schema)();
        let Some(obj) = a.as_object() else { return Err("arguments must be an object".into()) }; // i18n-ok: for agents
        let props = schema["properties"].as_object().cloned().unwrap_or_default();
        let unknown: Vec<&String> = obj.keys().filter(|k| !props.contains_key(k.as_str())).collect();
        if unknown.is_empty() {
            return Ok(());
        }
        let mut known: Vec<&String> = props.keys().collect();
        known.sort();
        Err(format!("{name} has no parameter {}; it takes: {}", unknown.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", "), known.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(", "))) // i18n-ok: for agents
    }

    pub fn call(&mut self, name: &str, a: &Value) -> Result<Value, String> {
        Self::check_args(name, a)?;
        if let Some(p) = name.strip_prefix("propose_") {
            return self.propose(p, a);
        }
        let focus = self.focus.clone();
        let file = self.tree.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        let tree = self.load()?;
        tools::read(tree, name, a, focus.as_deref(), &file)
    }

    fn propose(&mut self, kind: &str, a: &Value) -> Result<Value, String> {
        let dir = self.propose.clone().ok_or("proposals are off in this session")?; // i18n-ok: for agents
        let s = |k: &str| a.get(k).and_then(Value::as_str).map(str::to_string);
        let need = |k: &str| s(k).filter(|v| !v.trim().is_empty()).ok_or_else(|| format!("{k} is required")); // i18n-ok: for agents
        let relation = |k: &str| s(k).map(|r| proposal::Relation::parse(&r).ok_or_else(|| format!("{k} must be parent, child or partner"))).transpose(); // i18n-ok: for agents
        let p = match kind {
            "add_person" => Proposal::AddPerson {
                given: s("given").unwrap_or_default(),
                surname: s("surname").unwrap_or_default(),
                sex: s("sex"),
                birth_date: s("birth_date"),
                birth_place: s("birth_place"),
                death_date: s("death_date"),
                death_place: s("death_place"),
                relative: s("relative"),
                relation: relation("relation")?,
            },
            "add_relationship" => Proposal::AddRelationship { person: need("person")?, relative: need("relative")?, relation: relation("relation")?.ok_or("relation is required")? }, // i18n-ok: for agents
            "set_event" => Proposal::SetEvent { person: need("person")?, event: need("event")?, date: s("date"), place: s("place") },
            "set_name" => Proposal::SetName { person: need("person")?, given: s("given").unwrap_or_default(), surname: s("surname").unwrap_or_default() },
            "merge" => Proposal::Merge { keep: need("keep")?, duplicate: need("duplicate")? },
            _ => return Err(format!("unknown proposal {kind}")), // i18n-ok: for agents
        }
        .normalized();
        p.validate(self.load()?).map_err(|e| e.for_agent())?;
        let env = Envelope { proposal: p.clone(), reason: s("reason").unwrap_or_default().trim().to_string() };
        proposal::deliver(&dir, &env).map_err(|e| e.to_string())?;
        Ok(json!({"status": "awaiting_approval", "proposal": p, "message": "Shown to the user as a card in genealogy-rs; it changes the tree only if they accept it. Tell them to review the card; don't say it is done."})) // i18n-ok: for agents
    }

    /// Serves stdin → stdout until stdin closes.
    pub fn serve(&mut self) -> std::io::Result<()> {
        let stdin = std::io::stdin();
        let mut out = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            let reply = match serde_json::from_str::<Value>(&line) {
                Ok(msg) => self.handle(&msg),
                Err(_) => Some(rpc_error(Value::Null, -32700, "parse error")),
            };
            if let Some(r) = reply {
                writeln!(out, "{r}")?;
                out.flush()?;
            }
        }
        Ok(())
    }
}
