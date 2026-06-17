//! The universal event query: filter (cross-column AND, in-column OR) + optional
//! group-by aggregation. This one primitive subsumes the per-path / per-process
//! / cross-reference summaries — the AI composes them via filters + `group_by`.
//!
//! Semantics deliberately differ from the SDK's `FilterSet` (which is the GUI's
//! include-OR *view* filter): a query is "match ALL clauses", with multiple
//! values in one clause OR-ing. Both reuse the SDK's column vocabulary and
//! per-clause matching via [`procmon_sdk::clause_matches`].

use procmon_sdk::{clause_matches, Column, Event, FilterFields, Relation};
use serde::{Deserialize, Serialize};

/// A resolved filter clause: a column, a relation, and one or more candidate
/// values (the clause matches if the event matches the relation against ANY
/// value).
#[derive(Clone, Debug)]
pub struct Clause {
    pub column: Column,
    pub relation: Relation,
    pub values: Vec<String>,
}

impl Clause {
    /// Whether `ev` matches this clause (OR over the clause's values).
    pub fn matches<E: FilterFields>(&self, ev: &E) -> bool {
        self.values
            .iter()
            .any(|v| clause_matches(ev, self.column, self.relation, v))
    }
}

/// Whether `ev` matches every clause (cross-clause AND). Empty = match all.
pub fn matches_all<E: FilterFields>(ev: &E, clauses: &[Clause]) -> bool {
    clauses.iter().all(|c| c.matches(ev))
}

// --- JSON-facing raw clause (MCP tool args) --------------------------------

/// One or many string values, deserialized from either a bare string or an
/// array (so `"value": "x"` and `"value": ["x","y"]` both work).
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OneOrMany {
    One(String),
    Many(Vec<String>),
}

impl OneOrMany {
    fn into_vec(self) -> Vec<String> {
        match self {
            OneOrMany::One(s) => vec![s],
            OneOrMany::Many(v) => v,
        }
    }
}

/// A clause as it arrives over JSON (MCP) before column/relation resolution.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RawClause {
    pub column: String,
    #[serde(default = "default_relation")]
    pub relation: String,
    pub value: OneOrMany,
}

fn default_relation() -> String {
    "is".to_string()
}

impl RawClause {
    pub fn resolve(self) -> Result<Clause, String> {
        let column = parse_column(&self.column)
            .ok_or_else(|| format!("unknown filter column: {:?}", self.column))?;
        let relation = parse_relation(&self.relation)
            .ok_or_else(|| format!("unknown filter relation: {:?}", self.relation))?;
        let values = self.value.into_vec();
        if values.is_empty() {
            return Err("filter clause has no value".into());
        }
        Ok(Clause {
            column,
            relation,
            values,
        })
    }
}

/// Resolves a list of raw clauses, failing on the first bad column/relation.
pub fn resolve_clauses(raw: Vec<RawClause>) -> Result<Vec<Clause>, String> {
    raw.into_iter().map(RawClause::resolve).collect()
}

/// Parses a CLI-style `"Column relation value"` clause (e.g. `"ProcessName is
/// notepad.exe"`, `"Path contains \\Temp\\"`). The relation is matched greedily
/// against the known relation names so multi-word relations ("begins with")
/// work; the rest is the value.
pub fn parse_clause_str(s: &str) -> Result<Clause, String> {
    let s = s.trim();
    // Find the column (first token), then the longest known relation starting at
    // the next token, then the remainder is the value.
    let (col_str, rest) = s
        .split_once(char::is_whitespace)
        .ok_or_else(|| format!("filter needs 'Column relation value': {s:?}"))?;
    let column =
        parse_column(col_str).ok_or_else(|| format!("unknown filter column: {col_str:?}"))?;
    let rest = rest.trim_start();
    // Try two-word then one-word relations.
    for rel in Relation::ALL {
        let label = rel.label();
        if let Some(val) = rest
            .strip_prefix(label)
            .filter(|v| v.is_empty() || v.starts_with(char::is_whitespace))
        {
            let value = val.trim().to_string();
            if value.is_empty() {
                return Err(format!("filter needs a value: {s:?}"));
            }
            return Ok(Clause {
                column,
                relation: rel,
                values: vec![value],
            });
        }
    }
    Err(format!("unknown filter relation in: {s:?}"))
}

// --- column / relation name resolution -------------------------------------

/// Normalizes a name for lenient matching: lowercase, drop spaces/`_`/`&`.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '_' && *c != '&')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Resolves a column name: its display label (e.g. "Process Name"), a compact
/// form ("ProcessName"), or a short alias ("class", "op").
pub fn parse_column(name: &str) -> Option<Column> {
    let n = norm(name);
    let alias = match n.as_str() {
        "class" => "category",
        "op" => "operation",
        "proc" | "process" => "processname",
        "ppid" => "parentpid",
        _ => n.as_str(),
    };
    Column::ALL.into_iter().find(|c| norm(c.label()) == alias)
}

/// Resolves a relation name: its label ("is not"), compact ("isnot"), or a
/// common operator alias ("==", "!=", ">", "<").
pub fn parse_relation(name: &str) -> Option<Relation> {
    match name.trim() {
        "==" | "=" => return Some(Relation::Is),
        "!=" | "<>" => return Some(Relation::IsNot),
        ">" => return Some(Relation::MoreThan),
        "<" => return Some(Relation::LessThan),
        _ => {}
    }
    let n = norm(name);
    Relation::ALL.into_iter().find(|r| norm(r.label()) == n)
}

// --- group-by ---------------------------------------------------------------

/// One aggregation bucket: a distinct column value and how many matching events
/// had it. `processes` is the number of distinct process names that touched the
/// value (populated for cross-reference style group-bys).
#[derive(Clone, Debug, Serialize)]
pub struct GroupRow {
    pub value: String,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processes: Option<u64>,
}

/// Accumulates group-by counts (and distinct process names per group) as events
/// stream past. The `value` is the event's value for the group-by column.
#[derive(Default)]
pub struct Grouper {
    counts: rustc_hash::FxHashMap<String, (u64, rustc_hash::FxHashSet<String>)>,
}

impl Grouper {
    /// Records `ev`'s value for `column` (skips events with no value).
    pub fn observe(&mut self, ev: &Event, column: Column) {
        if let Some(value) = ev.filter_field(column) {
            let entry = self.counts.entry(value.into_owned()).or_default();
            entry.0 += 1;
            if let Some(pn) = ev.process_name() {
                entry.1.insert(pn.to_string());
            }
        }
    }

    /// Sorted-by-count-desc rows; `with_processes` includes the distinct process
    /// count per group (for cross-reference views).
    pub fn into_rows(self, with_processes: bool) -> Vec<GroupRow> {
        let mut rows: Vec<GroupRow> = self
            .counts
            .into_iter()
            .map(|(value, (count, procs))| GroupRow {
                value,
                count,
                processes: with_processes.then_some(procs.len() as u64),
            })
            .collect();
        rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.value.cmp(&b.value)));
        rows
    }
}
