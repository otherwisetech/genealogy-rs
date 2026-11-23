//! `genealogy-rs mcp`: a stdio MCP server (JSON-RPC 2.0, one message per line) over one family
//! tree, started by the app's AI panel for one session. It only reads: the tree file is loaded
//! again whenever it changes on disk (the app writes a snapshot of what it has open). With
//! `--propose <folder>` it also offers `propose_*` tools, which check the change and leave it in
//! that folder for the app to show as a card; nothing changes until the user accepts it there.
pub mod proposal;
pub mod tools;

use gedcom::Tree;
use proposal::{Envelope, Proposal};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::time::SystemTime;

pub const PROTOCOL_VERSIONS: &[&str] = &["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

/// The server's name inside the agent (`genealogy.search_people`).
pub const NAME: &str = "genealogy";

const HELP: &str = "The open family tree of genealogy-rs (GEDCOM). People have ids like I12; dates are GEDCOM phrases (\"12 MAR 1852\", \"ABT 1900\", \"BET 1840 AND 1850\"). Find people with search_people, then use their id. Only state facts the tools return; say when something is unknown. Family data is personal: quote only what the question needs."; // i18n-ok: for agents

struct Tool {
    name: &'static str,
    description: &'static str,
    schema: fn() -> Value,
    propose: bool,
}

fn id_schema(extra: Value) -> Value {
    let mut props = json!({"id": {"type": "string", "description": "person id, e.g. I12"}}); // i18n-ok: for agents
    if let (Some(p), Some(e)) = (props.as_object_mut(), extra.as_object()) {
        p.extend(e.clone());
    }
    json!({"type": "object", "properties": props, "required": ["id"]})
}

fn relation_schema() -> Value {
    json!({"type": "string", "enum": ["parent", "child", "partner"]})
}

const REASON: &str = "one short sentence for the user: why this change"; // i18n-ok: for agents

const TOOLS: &[Tool] = &[
    Tool { name: "tree_summary", description: "The open tree: file name, number of people and families, years covered, how complete the data is and the person on screen.", schema: || json!({"type": "object", "properties": {}}), propose: false }, // i18n-ok: for agents
    Tool { name: "search_people", description: "People whose name, years or places match `query` (accent-insensitive, every word must match). Empty query lists people in name order.", schema: || json!({"type": "object", "properties": {"query": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 200}}}), propose: false }, // i18n-ok: for agents
    Tool { name: "get_person", description: "One person: names, sex, every event with date and place, notes, parents, siblings, partners with their children, and what data is missing.", schema: || id_schema(json!({})), propose: false }, // i18n-ok: for agents
    Tool { name: "ancestors", description: "A person's ancestors up to `generations` back (default 4, max 10): generation 1 = parents, 2 = grandparents, 3 = great-grandparents. Each with its Ahnentafel number and the path (father/mother steps).", schema: || id_schema(json!({"generations": {"type": "integer", "minimum": 1, "maximum": 10}})), propose: false }, // i18n-ok: for agents
    Tool { name: "descendants", description: "A person's descendants down `generations` (default 3, max 8): generation 1 = children, 2 = grandchildren. Each with the parent they descend through.", schema: || id_schema(json!({"generations": {"type": "integer", "minimum": 1, "maximum": 8}})), propose: false }, // i18n-ok: for agents
    Tool { name: "relationship", description: "How two people are related (\"b is the great-grandmother of a\"), with the chain of people between them.", schema: || json!({"type": "object", "properties": {"a": {"type": "string"}, "b": {"type": "string"}}, "required": ["a", "b"]}), propose: false }, // i18n-ok: for agents
    Tool { name: "find_duplicates", description: "People that look like the same person entered twice (similar names, compatible sex and years), best first with a 0-1 score. With `id`, only candidates for that person.", schema: || json!({"type": "object", "properties": {"id": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 200}}}), propose: false }, // i18n-ok: for agents
    Tool { name: "data_gaps", description: "What is missing: counts per gap (birth_date, birth_place, death_place, parents, partner, sources) and the people with gaps, least complete first. Filter with `missing` (one gap) or `id` (one person).", schema: || json!({"type": "object", "properties": {"missing": {"type": "string", "enum": ["birth_date", "birth_place", "death_place", "parents", "partner", "sources"]}, "id": {"type": "string"}, "limit": {"type": "integer", "minimum": 1, "maximum": 500}}}), propose: false }, // i18n-ok: for agents
    Tool { name: "check_consistency", description: "Likely errors: impossible or odd dates and ages (death before birth, mother too young, born after a parent died…), broken families, people with no links. Each with severity and a sentence in the user's language.", schema: || json!({"type": "object", "properties": {"severity": {"type": "string", "enum": ["error", "warning", "info"]}, "limit": {"type": "integer", "minimum": 1, "maximum": 500}}}), propose: false }, // i18n-ok: for agents
    Tool { name: "propose_add_person", description: "Propose adding a person, optionally as parent / child / partner of someone in the tree (`relation` of `relative`). Dates may be loose (\"12/3/1852\", \"abt 1900\"). Shown to the user as a card; nothing changes until they accept.", schema: || json!({"type": "object", "properties": {"given": {"type": "string"}, "surname": {"type": "string"}, "sex": {"type": "string", "enum": ["M", "F", "U"]}, "birth_date": {"type": "string"}, "birth_place": {"type": "string"}, "death_date": {"type": "string"}, "death_place": {"type": "string"}, "relative": {"type": "string", "description": "id of the person the new one is related to"}, "relation": relation_schema(), "reason": {"type": "string", "description": REASON}}}), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_add_relationship", description: "Propose linking two people already in the tree: `person` is the `relation` (parent, child or partner) of `relative`. Shown as a card the user accepts.", schema: || json!({"type": "object", "properties": {"person": {"type": "string"}, "relative": {"type": "string"}, "relation": relation_schema(), "reason": {"type": "string", "description": REASON}}, "required": ["person", "relative", "relation"]}), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_set_event", description: "Propose fixing the date and / or place of a person's birth (BIRT), christening (CHR), baptism (BAPM), death (DEAT), burial (BURI) or cremation (CREM). Leave out what should stay; an empty string clears it. Shown as a card the user accepts.", schema: || json!({"type": "object", "properties": {"person": {"type": "string"}, "event": {"type": "string", "enum": proposal::EVENTS}, "date": {"type": "string"}, "place": {"type": "string"}, "reason": {"type": "string", "description": REASON}}, "required": ["person", "event"]}), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_set_name", description: "Propose correcting a person's given name and surname. Shown as a card the user accepts.", schema: || json!({"type": "object", "properties": {"person": {"type": "string"}, "given": {"type": "string"}, "surname": {"type": "string"}, "reason": {"type": "string", "description": REASON}}, "required": ["person", "given", "surname"]}), propose: true }, // i18n-ok: for agents
    Tool { name: "propose_merge", description: "Propose merging `duplicate` into `keep` (the same person entered twice; see find_duplicates). Shown as a card the user accepts; they can undo it.", schema: || json!({"type": "object", "properties": {"keep": {"type": "string"}, "duplicate": {"type": "string"}, "reason": {"type": "string", "description": REASON}}, "required": ["keep", "duplicate"]}), propose: true }, // i18n-ok: for agents
];

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
}

impl Server {
    pub fn new(tree: PathBuf, propose: Option<PathBuf>, focus: Option<String>) -> Server {
        Server { tree, propose, focus, cache: None }
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
                    Ok(v) => ok(id, json!({"content": [{"type": "text", "text": v.to_string()}], "structuredContent": v, "isError": false})),
                    Err(e) => ok(id, json!({"content": [{"type": "text", "text": e}], "isError": true})),
                }
            }
            _ => rpc_error(id, -32601, &format!("method not found: {method}")), // i18n-ok: protocol
        })
    }

    pub fn call(&mut self, name: &str, a: &Value) -> Result<Value, String> {
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
