//! The universal event query: a filter expression + optional group-by
//! aggregation.
//!
//! A filter is an [`Expr`] tree of clauses combined with `&&` / `||` / `!` and
//! parentheses. A leaf clause is `Field <op> value`, where a [`Field`] is either a
//! Procmon-mirrored SDK [`Column`] or a structured extension field (network
//! endpoints, …) read from the decoded event. One query primitive (filter +
//! `group_by`, with an optional numeric `metric`) subsumes per-path / per-process /
//! cross-reference aggregations.

use procmon_sdk::{clause_matches, Column, Event, FilterFields, Relation};
use serde::Serialize;

/// A leaf condition: a field, a relation, and one or more candidate values
/// (matches if the relation holds against ANY value — the `in (...)` form).
#[derive(Clone, Debug)]
pub struct Clause {
    pub column: Field,
    pub relation: Relation,
    pub values: Vec<String>,
}

impl Clause {
    /// Whether `ev` matches (OR over the clause's values).
    pub fn matches(&self, ev: &Event) -> bool {
        self.values
            .iter()
            .any(|v| self.column.matches(ev, self.relation, v))
    }
}

/// A filter expression tree. Build one with [`parse_filter`].
#[derive(Clone, Debug)]
pub enum Expr {
    Clause(Clause),
    Not(Box<Expr>),
    And(Vec<Expr>),
    Or(Vec<Expr>),
}

impl Expr {
    /// Whether `ev` satisfies the expression.
    pub fn matches(&self, ev: &Event) -> bool {
        match self {
            Expr::Clause(c) => c.matches(ev),
            Expr::Not(e) => !e.matches(ev),
            Expr::And(v) => v.iter().all(|e| e.matches(ev)),
            Expr::Or(v) => v.iter().any(|e| e.matches(ev)),
        }
    }
}

// --- tokenizer --------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    LParen,
    RParen,
    Comma,
    AmpAmp,
    PipePipe,
    Bang,
    Rel(Relation),
    /// An unquoted word (column name, value, or a logical keyword).
    Bare(String),
    /// A quoted value (never a keyword).
    Quoted(String),
}

/// Characters that terminate a bareword (operators / structure / quote).
fn is_special(c: char) -> bool {
    matches!(
        c,
        '(' | ')' | '&' | '|' | '!' | '~' | '=' | '<' | '>' | '^' | '$' | '"' | ','
    )
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    let two = |chars: &[char], i: usize, c: char| chars.get(i + 1) == Some(&c);
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        match c {
            '(' => {
                out.push(Tok::LParen);
                i += 1;
            }
            ')' => {
                out.push(Tok::RParen);
                i += 1;
            }
            ',' => {
                out.push(Tok::Comma);
                i += 1;
            }
            '&' if two(&chars, i, '&') => {
                out.push(Tok::AmpAmp);
                i += 2;
            }
            '|' if two(&chars, i, '|') => {
                out.push(Tok::PipePipe);
                i += 2;
            }
            '!' if two(&chars, i, '~') => {
                out.push(Tok::Rel(Relation::Excludes));
                i += 2;
            }
            '!' if two(&chars, i, '=') => {
                out.push(Tok::Rel(Relation::IsNot));
                i += 2;
            }
            '!' => {
                out.push(Tok::Bang);
                i += 1;
            }
            '=' if two(&chars, i, '=') => {
                out.push(Tok::Rel(Relation::Is));
                i += 2;
            }
            '=' => {
                out.push(Tok::Rel(Relation::Is));
                i += 1;
            }
            '<' if two(&chars, i, '>') => {
                out.push(Tok::Rel(Relation::IsNot));
                i += 2;
            }
            '<' => {
                out.push(Tok::Rel(Relation::LessThan));
                i += 1;
            }
            '>' => {
                out.push(Tok::Rel(Relation::MoreThan));
                i += 1;
            }
            '~' => {
                out.push(Tok::Rel(Relation::Contains));
                i += 1;
            }
            '^' if two(&chars, i, '=') => {
                out.push(Tok::Rel(Relation::BeginsWith));
                i += 2;
            }
            '$' if two(&chars, i, '=') => {
                out.push(Tok::Rel(Relation::EndsWith));
                i += 2;
            }
            '^' | '$' => return Err(format!("'{c}' must be '{c}=' (begins/ends with)")),
            '&' | '|' => return Err(format!("use '{c}{c}' for logical and/or")),
            '"' => {
                let mut v = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    v.push(chars[i]);
                    i += 1;
                }
                if i >= chars.len() {
                    return Err("unterminated quoted value".into());
                }
                i += 1; // closing quote
                out.push(Tok::Quoted(v));
            }
            _ => {
                let start = i;
                while i < chars.len() && !chars[i].is_whitespace() && !is_special(chars[i]) {
                    i += 1;
                }
                out.push(Tok::Bare(chars[start..i].iter().collect()));
            }
        }
    }
    Ok(out)
}

// --- parser (recursive descent: or > and > not > primary) -------------------

struct Parser {
    toks: Vec<Tok>,
    pos: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos)
    }
    fn bump(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).cloned();
        if t.is_some() {
            self.pos += 1;
        }
        t
    }
    /// True (and consumed) if the next token is `kw` (`||`/`&&`/`!`) or the
    /// matching logical word (`or`/`and`/`not`).
    fn eat_logical(&mut self, sym: &Tok, word: &str) -> bool {
        match self.peek() {
            Some(t) if t == sym => {
                self.pos += 1;
                true
            }
            Some(Tok::Bare(w)) if w.eq_ignore_ascii_case(word) => {
                self.pos += 1;
                true
            }
            _ => false,
        }
    }

    fn or(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.and()?];
        while self.eat_logical(&Tok::PipePipe, "or") {
            parts.push(self.and()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Expr::Or(parts)
        })
    }

    fn and(&mut self) -> Result<Expr, String> {
        let mut parts = vec![self.not()?];
        while self.eat_logical(&Tok::AmpAmp, "and") {
            parts.push(self.not()?);
        }
        Ok(if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            Expr::And(parts)
        })
    }

    fn not(&mut self) -> Result<Expr, String> {
        if self.eat_logical(&Tok::Bang, "not") {
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.primary()
    }

    fn primary(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Tok::LParen) {
            self.pos += 1;
            let e = self.or()?;
            if self.bump() != Some(Tok::RParen) {
                return Err("expected ')'".into());
            }
            return Ok(e);
        }
        self.clause()
    }

    fn clause(&mut self) -> Result<Expr, String> {
        let col_tok = self
            .bump()
            .ok_or_else(|| "expected a column name".to_string())?;
        let col_name = match col_tok {
            Tok::Bare(w) | Tok::Quoted(w) => w,
            other => return Err(format!("expected a column name, got {other:?}")),
        };
        let column =
            parse_field(&col_name).ok_or_else(|| format!("unknown filter column: {col_name:?}"))?;

        // `Column in (a, b, c)` — OR over values (a single Is clause).
        if matches!(self.peek(), Some(Tok::Bare(w)) if w.eq_ignore_ascii_case("in")) {
            self.pos += 1;
            if self.bump() != Some(Tok::LParen) {
                return Err("expected '(' after 'in'".into());
            }
            let mut values = Vec::new();
            loop {
                values.push(self.value()?);
                match self.bump() {
                    Some(Tok::Comma) => continue,
                    Some(Tok::RParen) => break,
                    other => return Err(format!("expected ',' or ')', got {other:?}")),
                }
            }
            if values.is_empty() {
                return Err("'in (...)' needs at least one value".into());
            }
            return Ok(Expr::Clause(Clause {
                column,
                relation: Relation::Is,
                values,
            }));
        }

        let relation = match self.bump() {
            Some(Tok::Rel(r)) => r,
            other => {
                return Err(format!(
                    "expected an operator (== != ~ !~ ^= $= < > or 'in') after {col_name:?}, got {other:?}"
                ))
            }
        };
        let value = self.value()?;
        Ok(Expr::Clause(Clause {
            column,
            relation,
            values: vec![value],
        }))
    }

    fn value(&mut self) -> Result<String, String> {
        match self.bump() {
            Some(Tok::Bare(v)) | Some(Tok::Quoted(v)) => Ok(v),
            other => Err(format!("expected a value, got {other:?}")),
        }
    }
}

/// Parses a filter expression into an [`Expr`]. An empty/whitespace string is
/// rejected — callers treat "no filter" as `None` before calling this.
///
/// Grammar: `expr := or`; `or := and (("||"|"or") and)*`;
/// `and := not (("&&"|"and") not)*`; `not := ("!"|"not") not | primary`;
/// `primary := "(" expr ")" | clause`;
/// `clause := Column OP value | Column "in" "(" value ("," value)* ")"`.
/// `OP` is one of `== != ~ !~ ^= $= < >` (and `=`/`<>` aliases). Values with
/// spaces or special characters must be `"quoted"`.
pub fn parse_filter(s: &str) -> Result<Expr, String> {
    let toks = tokenize(s)?;
    if toks.is_empty() {
        return Err("empty filter".into());
    }
    let mut p = Parser { toks, pos: 0 };
    let expr = p.or()?;
    if p.pos != p.toks.len() {
        return Err(format!("unexpected trailing input near {:?}", p.peek()));
    }
    Ok(expr)
}

// --- column resolution ------------------------------------------------------

/// Normalizes a name for lenient matching: lowercase, drop spaces/`_`.
fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Resolves a column name: its label ("Process Name"), a compact form
/// ("ProcessName"), or a short alias ("class", "op").
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

/// A query field: either a Procmon-mirrored [`Column`] or a structured extension
/// field (network endpoint, file detail, …) resolved at runtime from the decoded
/// event. Keeps per-category detail out of the `Column` set — see
/// [`procmon_sdk::struct_fields`].
#[derive(Clone, Debug)]
pub enum Field {
    Col(Column),
    /// The canonical name of a structured extension field (`procmon_sdk` owns the
    /// per-category readers and the name table).
    Ext(String),
}

impl Field {
    /// The field's string value for `ev` (`None` if the event has none).
    fn value_str<'a>(&self, ev: &'a Event) -> Option<std::borrow::Cow<'a, str>> {
        match self {
            Field::Col(c) => ev.filter_field(*c),
            Field::Ext(name) => ev.struct_field(name),
        }
    }
    /// The field's numeric value for `ev` (numeric columns / fields only).
    fn value_num(&self, ev: &Event) -> Option<i64> {
        match self {
            Field::Col(c) => ev.filter_number(*c),
            Field::Ext(name) => ev.struct_number(name),
        }
    }
    /// Whether `ev`'s value for this field satisfies `relation`/`value`.
    fn matches(&self, ev: &Event, relation: Relation, value: &str) -> bool {
        match self {
            Field::Col(c) => clause_matches(ev, *c, relation, value),
            Field::Ext(name) => procmon_sdk::clause_matches_named(ev, name, relation, value),
        }
    }
    fn is_path(&self) -> bool {
        matches!(self, Field::Col(Column::Path))
    }
}

/// Resolves a field name to a [`Column`] or a structured extension field
/// (network endpoints, …). `None` if it matches neither.
pub fn parse_field(name: &str) -> Option<Field> {
    if let Some(c) = parse_column(name) {
        return Some(Field::Col(c));
    }
    let n = norm(name);
    procmon_sdk::struct_fields()
        .into_iter()
        .find(|f| norm(f.name) == n)
        .map(|f| Field::Ext(f.name.to_string()))
}

// --- group-by ---------------------------------------------------------------

/// One aggregation bucket: the group-by column value(s) and the match count.
/// `processes` is the number of distinct process names in the bucket (populated
/// for cross-reference style group-bys that include `Path`). When a numeric
/// `metric` column was requested, the bucket also carries sum/avg/min/max of that
/// metric plus the first/last event time seen in it.
#[derive(Clone, Debug, Serialize)]
pub struct GroupRow {
    /// One entry per group-by column, in the requested order.
    pub values: Vec<String>,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processes: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sum: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avg: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_time: Option<String>,
}

/// Per-bucket accumulator.
#[derive(Default)]
struct Acc {
    count: u64,
    procs: rustc_hash::FxHashSet<String>,
    sum: i64,
    n: u64,
    min: Option<i64>,
    max: Option<i64>,
    first_time: Option<String>,
    last_time: Option<String>,
}

/// Accumulates group-by buckets keyed on one or more columns. With a numeric
/// `metric` column it also rolls up sum/avg/min/max and the first/last event time
/// (computed only in that mode, so a plain count group-by stays light).
pub struct Grouper {
    cols: Vec<Field>,
    metric: Option<Field>,
    with_processes: bool,
    groups: rustc_hash::FxHashMap<Vec<String>, Acc>,
}

impl Grouper {
    pub fn new(cols: Vec<Field>, metric: Option<Field>) -> Self {
        let with_processes = cols.iter().any(Field::is_path);
        Self {
            cols,
            metric,
            with_processes,
            groups: rustc_hash::FxHashMap::default(),
        }
    }

    /// Records `ev`. Skipped if it lacks a value for any group-by column.
    pub fn observe(&mut self, ev: &Event) {
        let mut key = Vec::with_capacity(self.cols.len());
        for f in &self.cols {
            match f.value_str(ev) {
                Some(v) => key.push(v.into_owned()),
                None => return, // event lacks one of the grouped dimensions
            }
        }
        let acc = self.groups.entry(key).or_default();
        acc.count += 1;
        if self.with_processes {
            if let Some(pn) = ev.process_name() {
                acc.procs.insert(pn.to_string());
            }
        }
        if let Some(m) = &self.metric {
            if let Some(n) = m.value_num(ev) {
                acc.sum += n;
                acc.n += 1;
                acc.min = Some(acc.min.map_or(n, |x| x.min(n)));
                acc.max = Some(acc.max.map_or(n, |x| x.max(n)));
            }
            // Events stream in file (= time) order, so first/last seen == min/max.
            if let Some(t) = ev.filter_field(Column::Date) {
                let t = t.into_owned();
                if acc.first_time.is_none() {
                    acc.first_time = Some(t.clone());
                }
                acc.last_time = Some(t);
            }
        }
    }

    /// Buckets sorted by count desc (ties broken by the values).
    pub fn into_rows(self) -> Vec<GroupRow> {
        let has_metric = self.metric.is_some();
        let with_processes = self.with_processes;
        let mut rows: Vec<GroupRow> = self
            .groups
            .into_iter()
            .map(|(values, a)| GroupRow {
                values,
                count: a.count,
                processes: with_processes.then_some(a.procs.len() as u64),
                sum: has_metric.then_some(a.sum),
                avg: has_metric.then(|| {
                    if a.n > 0 {
                        a.sum as f64 / a.n as f64
                    } else {
                        0.0
                    }
                }),
                min: if has_metric { a.min } else { None },
                max: if has_metric { a.max } else { None },
                first_time: if has_metric { a.first_time } else { None },
                last_time: if has_metric { a.last_time } else { None },
            })
            .collect();
        rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.values.cmp(&b.values)));
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_or_not_in_and_ops() {
        // A representative expression exercising every construct.
        let e = parse_filter(
            r#"Category == "File System" && ProcessName == notepad.exe
               && (Operation in (WriteFile, DeleteFile) || Path ~ Temp)
               && !(Result != SUCCESS)"#,
        )
        .expect("parse");
        // Top level is an And of 4 parts.
        match e {
            Expr::And(v) => assert_eq!(v.len(), 4),
            other => panic!("expected And, got {other:?}"),
        }
    }

    #[test]
    fn operator_relations_map_correctly() {
        let cases = [
            ("Path ~ x", Relation::Contains),
            ("Path !~ x", Relation::Excludes),
            ("Path ^= x", Relation::BeginsWith),
            ("Path $= x", Relation::EndsWith),
            ("Pid == 4", Relation::Is),
            ("Pid != 4", Relation::IsNot),
            ("Pid < 4", Relation::LessThan),
            ("Pid > 4", Relation::MoreThan),
        ];
        for (src, rel) in cases {
            match parse_filter(src).unwrap() {
                Expr::Clause(c) => assert_eq!(c.relation, rel, "{src}"),
                other => panic!("{src} -> {other:?}"),
            }
        }
    }

    #[test]
    fn quoted_values_allow_spaces_and_specials() {
        match parse_filter(r#"Path $= "$Mft""#).unwrap() {
            Expr::Clause(c) => assert_eq!(c.values, vec!["$Mft".to_string()]),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn errors_are_friendly() {
        assert!(parse_filter("Bogus == x")
            .unwrap_err()
            .contains("unknown filter column"));
        assert!(parse_filter("Path").unwrap_err().contains("operator"));
        assert!(parse_filter("").unwrap_err().contains("empty"));
    }
}
