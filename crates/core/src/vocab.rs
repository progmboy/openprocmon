//! The filter vocabulary (`list_filter_columns`): the exact column names,
//! relation names, and per-category operation names the AI must use verbatim in
//! `query_events` filters. Derived from the SDK so it never drifts.

use procmon_sdk::kernel_types::{proc_notify, reg_notify, FILE_NOTIFY_BASE};
use procmon_sdk::{strings, Column, NetOp, Relation};
use serde::Serialize;

/// The complete filter vocabulary for building `query_events` filter
/// expressions.
#[derive(Clone, Debug, Serialize)]
pub struct FilterVocab {
    /// How to write a filter: the expression grammar, with examples.
    pub syntax: String,
    /// Symbolic operators usable in a clause (`symbol` -> what it means).
    pub operators: Vec<Operator>,
    /// Columns usable on the left of a clause (the Procmon-mirrored set), each with
    /// a one-line description of what it means.
    pub columns: Vec<ColumnDoc>,
    /// Structured extension fields usable on the left of a clause — network
    /// endpoints etc., beyond the Procmon columns; the `numeric` ones also work as
    /// a `metric` for sum/avg/min/max aggregation.
    pub extension_fields: Vec<ExtField>,
    /// Relation names (the words the operators map to; informational).
    pub relations: Vec<String>,
    /// Exact `Operation` column values, grouped by category — so a filter says
    /// `Operation == WriteFile` with a real name, not a guess.
    pub operations: Operations,
}

/// One filter operator: its symbol and the relation it expresses.
#[derive(Clone, Debug, Serialize)]
pub struct Operator {
    pub symbol: &'static str,
    pub meaning: &'static str,
}

/// A Procmon column name plus a one-line description of its meaning.
#[derive(Clone, Debug, Serialize)]
pub struct ColumnDoc {
    pub name: &'static str,
    pub description: &'static str,
}

/// A structured extension field (beyond the Procmon `columns`): a network
/// endpoint, file detail, … Usable on the left of a clause and, when `numeric`,
/// as a `metric`.
#[derive(Clone, Debug, Serialize)]
pub struct ExtField {
    pub name: &'static str,
    pub category: &'static str,
    pub numeric: bool,
    /// What the field means — so the agent uses it correctly without guessing.
    pub description: &'static str,
}

/// Operation names per category.
#[derive(Clone, Debug, Serialize)]
pub struct Operations {
    pub process: Vec<String>,
    pub file: Vec<String>,
    pub registry: Vec<String>,
    pub network: Vec<String>,
}

/// Collects the distinct, known (`!= "<Unknown>"`) names a code-range maps to.
fn distinct(names: impl Iterator<Item = &'static str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for n in names {
        if n != "<Unknown>" && !out.iter().any(|x| x == n) {
            out.push(n.to_string());
        }
    }
    out
}

/// Builds the vocabulary (no event data needed).
pub fn filter_vocab() -> FilterVocab {
    let columns = Column::ALL
        .iter()
        .map(|c| ColumnDoc {
            name: c.label(),
            description: c.description(),
        })
        .collect();
    let extension_fields = procmon_sdk::struct_fields()
        .into_iter()
        .map(|f| ExtField {
            name: f.name,
            category: f.category,
            numeric: f.numeric,
            description: f.description,
        })
        .collect();
    let relations = Relation::ALL
        .iter()
        .map(|r| r.label().to_string())
        .collect();

    let process = distinct((0..=proc_notify::SYSTEM_PERFORMANCE).map(strings::process_operation));
    let registry = distinct((0..=reg_notify::QUERYKEYSECURITY).map(strings::reg_operation));
    // File ops: the friendly major-level names (minor 0, advance=false) — these are
    // what the Operation column carries for filtering (cf. `Event::operation_name`).
    let file = distinct(
        (0u16..0x40).map(|maj| strings::file_operation(FILE_NOTIFY_BASE + maj, 0, false, false)),
    );
    let network = distinct((0u16..=9).map(|c| NetOp::from_pml(c).name()));

    FilterVocab {
        syntax: SYNTAX.to_string(),
        operators: OPERATORS.to_vec(),
        columns,
        extension_fields,
        relations,
        operations: Operations {
            process,
            file,
            registry,
            network,
        },
    }
}

/// The symbolic operators, in the order they're documented.
const OPERATORS: &[Operator] = &[
    Operator {
        symbol: "==",
        meaning: "is (equals; = is an alias)",
    },
    Operator {
        symbol: "!=",
        meaning: "is not (<> is an alias)",
    },
    Operator {
        symbol: "~",
        meaning: "contains (substring)",
    },
    Operator {
        symbol: "!~",
        meaning: "excludes (does not contain)",
    },
    Operator {
        symbol: "^=",
        meaning: "begins with",
    },
    Operator {
        symbol: "$=",
        meaning: "ends with",
    },
    Operator {
        symbol: "<",
        meaning: "less than (numeric)",
    },
    Operator {
        symbol: ">",
        meaning: "more than (numeric)",
    },
    Operator {
        symbol: "in (a, b, c)",
        meaning: "matches ANY of the listed values (OR)",
    },
];

const SYNTAX: &str = concat!(
    "A filter is an expression: `Column OP value` clauses joined with ",
    "&& (and), || (or), ! (not) and parentheses. Quote values that contain spaces or ",
    "special characters, e.g. \"File System\". Examples:\n",
    "  Operation == WriteFile\n",
    "  Category == \"File System\" && Operation == WriteFile\n",
    "  Category == Registry && Operation in (RegSetValue, RegCreateKey) && Path ~ Run\n",
    "  ProcessName == app.exe && (Result != SUCCESS || Path $= .tmp)\n",
    "Column names and operation values are case-insensitive and accept the compact form ",
    "(ProcessName) or the label (\"Process Name\").",
);
