//! The universal event query: a filter expression (a Wireshark-style display
//! filter) + optional group-by aggregation.
//!
//! A filter is an [`Expr`] tree of clauses combined with `&&` / `||` / `!` and
//! parentheses. A leaf clause is `Column <op> value`, reusing the SDK's column
//! vocabulary and per-clause matching ([`procmon_sdk::clause_matches`]). One
//! query primitive (filter + `group_by`) subsumes per-path / per-process /
//! cross-reference aggregations.

use procmon_sdk::{clause_matches, Column, Event, FilterFields, Relation};
use serde::Serialize;

/// A leaf condition: a column, a relation, and one or more candidate values
/// (matches if the relation holds against ANY value — the `in (...)` form).
#[derive(Clone, Debug)]
pub struct Clause {
    pub column: Column,
    pub relation: Relation,
    pub values: Vec<String>,
}

impl Clause {
    /// Whether `ev` matches (OR over the clause's values).
    pub fn matches<E: FilterFields>(&self, ev: &E) -> bool {
        self.values
            .iter()
            .any(|v| clause_matches(ev, self.column, self.relation, v))
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
    pub fn matches<E: FilterFields>(&self, ev: &E) -> bool {
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
        let column = parse_column(&col_name)
            .ok_or_else(|| format!("unknown filter column: {col_name:?}"))?;

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

// --- group-by ---------------------------------------------------------------

/// One aggregation bucket: a distinct column value and how many matching events
/// had it. `processes` is the number of distinct process names that touched the
/// value (populated for cross-reference style group-bys on `Path`).
#[derive(Clone, Debug, Serialize)]
pub struct GroupRow {
    pub value: String,
    pub count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub processes: Option<u64>,
}

/// Accumulates group-by counts (and distinct process names per group) as events
/// stream past.
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
    /// count per group.
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
