//! Event filtering (cf. C++ `filter.cpp` / `filtermgr.cpp`, lifted into the SDK).
//!
//! A [`FilterSet`] is a list of include/exclude [`Rule`]s evaluated against an
//! [`Event`] with the standard Procmon semantics. The C++ `CFilter::Filter`
//! combined multiple include rules with an OR-of-hide that made them cancel each
//! other out; this implementation uses the correct rule (see [`FilterSet::matches`]).
//!
//! Filtering is a display-time predicate by default: the pipeline keeps every
//! event and the GUI calls `matches` while rendering, so changing the filter can
//! reveal previously hidden events (matching real Procmon).

use crate::event::{Event, EventClass};

/// A column an event can be filtered on (mirrors C++ `MAP_SOURCE_TYPE`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Column {
    /// Process architecture: `32-bit` / `64-bit`.
    Architecture,
    /// Logon-session id, `HighPart:LowPart`.
    AuthId,
    /// Process command line.
    CommandLine,
    /// Image company name (from version metadata; empty until extracted).
    Company,
    /// Completion time (raw 100-ns ticks).
    CompletionTime,
    /// Event date/start time (raw 100-ns ticks).
    Date,
    /// Image description / product name (from version metadata).
    Description,
    /// Operation-specific detail string.
    Detail,
    /// Operation duration (raw 100-ns ticks).
    Duration,
    /// Event category, e.g. `File System`.
    Class,
    /// Raw NT image path of the process.
    ImagePath,
    /// Integrity level, e.g. `Medium`.
    Integrity,
    /// Operation name, e.g. `CreateFile`.
    Operation,
    /// Parent process id.
    ParentPid,
    /// Operation target path/key.
    Path,
    /// Process id.
    Pid,
    /// Process image file name (basename).
    ProcessName,
    /// Operation result, e.g. `SUCCESS`.
    Result,
    /// PRE/POST correlation sequence.
    Sequence,
    /// Session id.
    Session,
    /// Thread id.
    Tid,
    /// Event time-of-day (raw 100-ns ticks).
    TimeOfDay,
    /// User account (`DOMAIN\\User`).
    User,
    /// Image version (from version metadata).
    Version,
    /// Token virtualization: `True` / `False`.
    Virtualized,
}

impl Column {
    /// Every column, in the order the filter dialog presents them (most-used first
    /// so the default selection is `Process Name`).
    pub const ALL: [Column; 25] = [
        Column::ProcessName,
        Column::Pid,
        Column::Operation,
        Column::Path,
        Column::Result,
        Column::Class,
        Column::Detail,
        Column::ImagePath,
        Column::CommandLine,
        Column::ParentPid,
        Column::Session,
        Column::User,
        Column::Architecture,
        Column::Integrity,
        Column::Virtualized,
        Column::AuthId,
        Column::Company,
        Column::Description,
        Column::Version,
        Column::Date,
        Column::TimeOfDay,
        Column::CompletionTime,
        Column::Duration,
        Column::Sequence,
        Column::Tid,
    ];

    /// The column's display name (cf. Process Monitor's filter dropdown).
    pub fn label(self) -> &'static str {
        match self {
            Column::Architecture => "Architecture",
            Column::AuthId => "Authentication ID",
            Column::CommandLine => "Command Line",
            Column::Company => "Company",
            Column::CompletionTime => "Completion Time",
            Column::Date => "Date & Time",
            Column::Description => "Description",
            Column::Detail => "Detail",
            Column::Duration => "Duration",
            Column::Class => "Category",
            Column::ImagePath => "Image Path",
            Column::Integrity => "Integrity",
            Column::Operation => "Operation",
            Column::ParentPid => "Parent PID",
            Column::Path => "Path",
            Column::Pid => "PID",
            Column::ProcessName => "Process Name",
            Column::Result => "Result",
            Column::Sequence => "Sequence",
            Column::Session => "Session",
            Column::Tid => "TID",
            Column::TimeOfDay => "Time of Day",
            Column::User => "User",
            Column::Version => "Version",
            Column::Virtualized => "Virtualized",
        }
    }

    /// A one-line description of the column's meaning, for the filter vocabulary so
    /// an agent doesn't have to guess from the name.
    pub fn description(self) -> &'static str {
        match self {
            Column::ProcessName => "Image file name of the process (basename), e.g. notepad.exe.",
            Column::Pid => "Process id.",
            Column::Operation => "Operation name, e.g. CreateFile / RegSetValue / TCP Send.",
            Column::Path => {
                "Target of the operation: a file path, a registry key, or network endpoints."
            }
            Column::Result => {
                "Operation result/status, e.g. SUCCESS / NAME NOT FOUND / ACCESS DENIED."
            }
            Column::Class => {
                "Event category: File System / Registry / Network / Process / Profiling."
            }
            Column::Detail => "Operation-specific detail string (free-form; varies by operation).",
            Column::ImagePath => "Full NT image path of the process executable.",
            Column::CommandLine => "Process command line.",
            Column::ParentPid => "Parent process id.",
            Column::Session => "Windows session id.",
            Column::User => "User account that ran the process (DOMAIN\\User).",
            Column::Architecture => "Process architecture: 32-bit or 64-bit.",
            Column::Integrity => "Process integrity level, e.g. Low / Medium / High / System.",
            Column::Virtualized => "Whether UAC token virtualization is enabled: True or False.",
            Column::AuthId => "Logon session id (Authentication ID), as HighPart:LowPart.",
            Column::Company => "Image company name, from the executable's version metadata.",
            Column::Description => "Image description / product name, from version metadata.",
            Column::Version => "Image file version, from version metadata.",
            Column::Date => "Event start date and time.",
            Column::TimeOfDay => "Event time of day.",
            Column::CompletionTime => "Time the operation completed.",
            Column::Duration => "How long the operation took.",
            Column::Sequence => "Event sequence number (PRE/POST correlation / capture order).",
            Column::Tid => "Thread id.",
        }
    }
}

/// How a rule's value is compared against the event's column (case-insensitive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Relation {
    Is,
    IsNot,
    LessThan,
    MoreThan,
    BeginsWith,
    EndsWith,
    Contains,
    Excludes,
}

impl Relation {
    /// Every relation, in dialog order (default selection is `is`).
    pub const ALL: [Relation; 8] = [
        Relation::Is,
        Relation::IsNot,
        Relation::LessThan,
        Relation::MoreThan,
        Relation::BeginsWith,
        Relation::EndsWith,
        Relation::Contains,
        Relation::Excludes,
    ];

    /// The relation's display name.
    pub fn label(self) -> &'static str {
        match self {
            Relation::Is => "is",
            Relation::IsNot => "is not",
            Relation::LessThan => "less than",
            Relation::MoreThan => "more than",
            Relation::BeginsWith => "begins with",
            Relation::EndsWith => "ends with",
            Relation::Contains => "contains",
            Relation::Excludes => "excludes",
        }
    }
}

/// Whether a matching rule shows or hides the event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Include,
    Exclude,
}

impl Action {
    /// Both actions, in dialog order (default selection is `Include`).
    pub const ALL: [Action; 2] = [Action::Include, Action::Exclude];

    /// The action's display name.
    pub fn label(self) -> &'static str {
        match self {
            Action::Include => "Include",
            Action::Exclude => "Exclude",
        }
    }
}

/// One filter rule.
#[derive(Debug, Clone)]
pub struct Rule {
    pub column: Column,
    pub relation: Relation,
    pub value: String,
    pub action: Action,
    pub enabled: bool,
}

impl Rule {
    /// Convenience constructor for an enabled rule.
    pub fn new(
        column: Column,
        relation: Relation,
        value: impl Into<String>,
        action: Action,
    ) -> Self {
        Self {
            column,
            relation,
            value: value.into(),
            action,
            enabled: true,
        }
    }

    /// Whether two rules express the same predicate — same column/relation/action
    /// and the same value (compared case-insensitively), ignoring `enabled`. Used
    /// for the duplicate check in [`FilterSet::add`] / [`FilterSet::contains`].
    pub fn same_rule(&self, other: &Rule) -> bool {
        self.column == other.column
            && self.relation == other.relation
            && self.action == other.action
            && self.value.eq_ignore_ascii_case(&other.value)
    }
}

/// Provides the comparison value for each filter [`Column`], so a [`FilterSet`] can
/// be evaluated against any event representation — the SDK [`Event`] (implemented
/// here) or, e.g., the GUI's unified row type (implemented there). This is what
/// lets a single filter engine serve both the driver pipeline and the UI.
///
/// Returns `Cow` so columns whose value already exists as a string (operation,
/// process name, paths held by the event) are borrowed, not allocated — filter
/// evaluation runs per event on the hot path.
pub trait FilterFields {
    /// The column's value as a string, or `None` if the event has none.
    fn filter_field(&self, column: Column) -> Option<std::borrow::Cow<'_, str>>;

    /// The column's value as an integer, for integer columns (PID, TID,
    /// sequence, parent PID, session); `None` for string columns or when the
    /// event has no value. Lets equality/ordering rules on these columns
    /// compare numerically without stringifying per evaluation.
    fn filter_number(&self, column: Column) -> Option<i64> {
        let _ = column;
        None
    }
}

impl FilterFields for Event {
    fn filter_field(&self, column: Column) -> Option<std::borrow::Cow<'_, str>> {
        column_value(self, column)
    }

    fn filter_number(&self, column: Column) -> Option<i64> {
        match column {
            Column::Pid => Some(self.pid() as i64),
            Column::Tid => Some(self.thread_id() as i64),
            Column::Sequence => Some(self.sequence() as i64),
            Column::ParentPid => self.parent_pid().map(|v| v as i64),
            Column::Session => self.session_id().map(|v| v as i64),
            _ => None,
        }
    }
}

/// An ordered set of filter rules.
#[derive(Debug, Clone, Default)]
pub struct FilterSet {
    pub rules: Vec<Rule>,
}

impl FilterSet {
    pub fn new(rules: Vec<Rule>) -> Self {
        Self { rules }
    }

    /// Whether `rule` is already present (by [`Rule::same_rule`]).
    pub fn contains(&self, rule: &Rule) -> bool {
        self.rules.iter().any(|r| r.same_rule(rule))
    }

    /// Appends `rule` unless an equivalent one already exists. Returns `true` if it
    /// was added, `false` if it was a duplicate.
    pub fn add(&mut self, rule: Rule) -> bool {
        if self.contains(&rule) {
            false
        } else {
            self.rules.push(rule);
            true
        }
    }

    /// Inserts `rule` at the front unless an equivalent one already exists. Returns
    /// `true` if it was added. Used by the context-menu quick filters so the newest
    /// rule appears at the top of the list.
    pub fn add_front(&mut self, rule: Rule) -> bool {
        if self.contains(&rule) {
            false
        } else {
            self.rules.insert(0, rule);
            true
        }
    }

    /// Removes the rule at `index`, returning it (or `None` if out of range).
    pub fn remove(&mut self, index: usize) -> Option<Rule> {
        (index < self.rules.len()).then(|| self.rules.remove(index))
    }

    /// Returns whether `ev` should be visible under these rules.
    ///
    /// Procmon semantics: any enabled **exclude** rule that matches hides the
    /// event; if any enabled **include** rule exists and none match, the event is
    /// hidden; otherwise it is shown. With no rules everything is visible.
    ///
    /// Only the columns referenced by enabled rules are materialized — at most
    /// once each per call (see [`ColumnMemo`]) — keeping both the common
    /// (empty/short) filter and many-rules-per-column sets cheap.
    pub fn matches<E: FilterFields>(&self, ev: &E) -> bool {
        let mut has_include = false;
        let mut include_hit = false;
        let mut memo = ColumnMemo::new();

        for rule in self.rules.iter().filter(|r| r.enabled) {
            let hit = clause_matches_memo(ev, rule.column, rule.relation, &rule.value, &mut memo);
            match rule.action {
                Action::Exclude => {
                    if hit {
                        return false;
                    }
                }
                Action::Include => {
                    has_include = true;
                    include_hit |= hit;
                }
            }
        }

        // Pass unless there are Include rules and none of them matched.
        !has_include || include_hit
    }

    /// Whether `ev` should be *highlighted* under these rules (the Highlight dialog
    /// reuses the same rule set): highlighted when it matches an enabled Include
    /// rule and no enabled Exclude rule. An empty set highlights nothing.
    pub fn highlights<E: FilterFields>(&self, ev: &E) -> bool {
        let mut included = false;
        let mut memo = ColumnMemo::new();
        for rule in self.rules.iter().filter(|r| r.enabled) {
            let hit = clause_matches_memo(ev, rule.column, rule.relation, &rule.value, &mut memo);
            match rule.action {
                Action::Exclude => {
                    if hit {
                        return false;
                    }
                }
                Action::Include => included |= hit,
            }
        }
        included
    }
}

/// Procmon's default display filter — the exclude rules active in the normal
/// (non-"Advanced Output") view. The single source of truth shared by the GUI's
/// Advanced Output toggle, the SDK example, and the CLI/MCP `exclude_noise` set.
///
/// Hides: our own produced executables (`procmon-gui.exe`, `procmon-cli.exe`,
/// `procmon-example.exe`) and the Sysinternals tools, the System process, the
/// IRP/FastIO bookkeeping operations, and NTFS metadata files. Every rule is an
/// Exclude, so the set is one OR-of-exclusions.
///
/// Process INIT ("Process Defined") records never reach any filter — the
/// correlator drops them after seeding the process table (see `parse`).
pub fn default_display_filter() -> Vec<Rule> {
    let proc = |name: &str| Rule::new(Column::ProcessName, Relation::Is, name, Action::Exclude);
    let ends = |name: &str| Rule::new(Column::Path, Relation::EndsWith, name, Action::Exclude);
    let op = |name: &str| {
        Rule::new(
            Column::Operation,
            Relation::BeginsWith,
            name,
            Action::Exclude,
        )
    };
    vec![
        // Our own produced executables.
        proc("procmon-gui.exe"),
        proc("procmon-cli.exe"),
        proc("procmon-example.exe"),
        // Sysinternals tools commonly running alongside.
        proc("Procmon.exe"),
        proc("Procmon64.exe"),
        proc("Procexp.exe"),
        proc("Procexp64.exe"),
        proc("Autoruns.exe"),
        proc("System"),
        // Low-level IRP/FastIO bookkeeping operations.
        op("IRP_MJ_"),
        op("FASTIO_"),
        op("FAST IO"),
        Rule::new(
            Column::Result,
            Relation::BeginsWith,
            "FAST IO",
            Action::Exclude,
        ),
        // NTFS metadata files.
        ends("pagefile.sys"),
        ends("$Mft"),
        ends("$MftMirr"),
        ends("$LogFile"),
        ends("$Volume"),
        ends("$AttrDef"),
        ends("$Root"),
        ends("$Bitmap"),
        ends("$Boot"),
        ends("$BadClus"),
        ends("$Secure"),
        ends("$Upcase"),
        Rule::new(Column::Path, Relation::Contains, "$Extend", Action::Exclude),
    ]
}

/// Per-evaluation memo of column values: within one `matches()`/`highlights()`
/// call, each referenced column is materialized at most once, however many
/// rules target it and in whatever order (Procmon's default noise filter alone
/// has 13 `Path` rules). Rules are still evaluated in list order with the
/// existing exclude short-circuit, so columns past an early exclude hit are
/// never materialized at all.
pub struct ColumnMemo<'e> {
    /// `slots[column as usize]`: `None` = not derived yet; `Some(v)` = the
    /// memoized `filter_field` result (which may itself be `None`).
    slots: [Option<Option<std::borrow::Cow<'e, str>>>; Column::ALL.len()],
}

impl<'e> ColumnMemo<'e> {
    pub fn new() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
        }
    }

    /// The memoized column value, deriving it on first use.
    fn get<E: FilterFields>(&mut self, ev: &'e E, column: Column) -> Option<&str> {
        self.slots[column as usize]
            .get_or_insert_with(|| ev.filter_field(column))
            .as_deref()
    }
}

impl Default for ColumnMemo<'_> {
    fn default() -> Self {
        Self::new()
    }
}

/// Numeric comparison shared by every clause evaluator.
fn compare_numbers(relation: Relation, actual: i64, expected: i64) -> bool {
    match relation {
        Relation::Is => actual == expected,
        Relation::IsNot => actual != expected,
        Relation::LessThan => actual < expected,
        Relation::MoreThan => actual > expected,
        _ => unreachable!("callers guard on the numeric relations"),
    }
}

/// Numeric fast path: equality/ordering relations compare integers directly
/// (no per-evaluation to_string). `actual` lazily supplies the number —
/// `filter_number` for a `Column` clause, `struct_number` for a named
/// extension field — so both clause kinds share the one guard + parse.
/// Substring relations are inherently textual, and a missing number or
/// non-numeric rule value falls back to the string path (lexicographic, as
/// before). `None` = not applicable.
fn numeric_fast_path(
    relation: Relation,
    value: &str,
    actual: impl FnOnce() -> Option<i64>,
) -> Option<bool> {
    if !matches!(
        relation,
        Relation::Is | Relation::IsNot | Relation::LessThan | Relation::MoreThan
    ) {
        return None;
    }
    let actual = actual()?;
    let expected = value.parse::<i64>().ok()?;
    Some(compare_numbers(relation, actual, expected))
}

/// [`clause_matches`] with a caller-held [`ColumnMemo`]: within one memo's
/// lifetime (one event evaluation), each referenced column is derived at most
/// once no matter how many clauses target it — the building block for query
/// engines that run a filter expression *and* a noise filter per event.
pub fn clause_matches_memo<'e, E: FilterFields>(
    ev: &'e E,
    column: Column,
    relation: Relation,
    value: &str,
    memo: &mut ColumnMemo<'e>,
) -> bool {
    if let Some(hit) = numeric_fast_path(relation, value, || ev.filter_number(column)) {
        return hit;
    }
    memo.get(ev, column)
        .map(|actual| relation_matches(relation, actual, value))
        .unwrap_or(false)
}

/// Whether a single `(column, relation, value)` clause matches `ev` — the
/// public building block for custom query predicates (e.g. an AND-of-clauses
/// query engine) over the same column/relation vocabulary and matching
/// semantics (numeric fast path + ASCII case-insensitive relations) the GUI
/// filter uses. This is [`clause_matches_memo`] with a one-shot memo; callers
/// that evaluate many clauses per row should hold a memo across them instead.
pub fn clause_matches<E: FilterFields>(
    ev: &E,
    column: Column,
    relation: Relation,
    value: &str,
) -> bool {
    clause_matches_memo(ev, column, relation, value, &mut ColumnMemo::new())
}

/// Whether a structured (extension) field `name` matches under `relation`/`value`.
/// The non-[`Column`] analog of [`clause_matches`]: a numeric fast path via
/// [`Event::struct_number`], otherwise a string compare via [`Event::struct_field`].
/// An unknown field never matches. These fields (network endpoints, file detail, …)
/// live beside the Procmon-mirrored `Column` set instead of inflating it.
pub fn clause_matches_named(ev: &Event, name: &str, relation: Relation, value: &str) -> bool {
    if let Some(hit) = numeric_fast_path(relation, value, || ev.struct_number(name)) {
        return hit;
    }
    ev.struct_field(name)
        .map(|actual| relation_matches(relation, &actual, value))
        .unwrap_or(false)
}

/// Metadata for a structured (extension) query field: its name, the event category
/// it applies to, whether it is numeric (usable as a `metric` / numeric compare),
/// and a human-readable description of what it means.
pub struct StructField {
    pub name: &'static str,
    pub category: &'static str,
    pub numeric: bool,
    pub description: &'static str,
}

/// Every structured extension field the query layer understands, beside the
/// Procmon-mirrored [`Column`] set (network + file for now; registry to follow).
/// Adding a field is one entry next to its decoder, not a new `Column` variant.
pub fn struct_fields() -> Vec<StructField> {
    let net = crate::parse::network::NETWORK_FIELDS
        .iter()
        .map(|&(name, numeric, description)| StructField {
            name,
            category: "Network",
            numeric,
            description,
        });
    let file = crate::parse::file::FILE_FIELDS
        .iter()
        .map(|&(name, numeric, description)| StructField {
            name,
            category: "File System",
            numeric,
            description,
        });
    net.chain(file).collect()
}

/// Extracts the comparison string for a column, or `None` if the event has none.
///
/// Delegates to the [`Event`] accessors, which return `None` when no process is
/// attached (network or untracked) and for metadata columns until the async
/// worker fills them. Time columns are formatted as local time strings. Values
/// the event already holds as strings are borrowed; only derived values
/// (paths, details, formatted times, numbers) allocate.
fn column_value(ev: &Event, column: Column) -> Option<std::borrow::Cow<'_, str>> {
    use std::borrow::Cow::{Borrowed, Owned};
    let yes_no = |b: bool, yes: &'static str, no: &'static str| Borrowed(if b { yes } else { no });
    match column {
        // Event-intrinsic columns.
        Column::Operation => Some(Borrowed(ev.operation_name())),
        Column::Path => ev.path().map(Owned),
        Column::Result => Some(ev.result()),
        Column::Detail => Some(Owned(ev.detail())),
        Column::Class => Some(Borrowed(ev.class_name())),
        Column::Pid => Some(Owned(ev.pid().to_string())),
        Column::Tid => Some(Owned(ev.thread_id().to_string())),
        Column::Sequence => Some(Owned(ev.sequence().to_string())),
        // Full precision so Date & Time `LessThan`/`MoreThan` compares to the tick,
        // not just the second.
        Column::Date => Some(Owned(ev.date_precise())),
        Column::TimeOfDay => Some(Owned(ev.time_of_day())),
        Column::CompletionTime => ev.completion_time().map(Owned),
        Column::Duration => ev.duration().map(Owned),

        // Process-derived columns.
        Column::ProcessName => ev.process_name().map(Borrowed),
        Column::ImagePath => ev.image_path().filter(|s| !s.is_empty()).map(Borrowed),
        Column::CommandLine => ev.command_line().filter(|s| !s.is_empty()).map(Borrowed),
        Column::ParentPid => ev.parent_pid().map(|v| Owned(v.to_string())),
        Column::Session => ev.session_id().map(|v| Owned(v.to_string())),
        Column::Architecture => ev.is_wow64().map(|w| yes_no(w, "32-bit", "64-bit")),
        Column::Virtualized => ev.is_virtualized().map(|v| yes_no(v, "True", "False")),
        Column::AuthId => ev.auth_id().map(Owned),
        Column::Integrity => ev.integrity().map(Borrowed),
        Column::User => ev.user().map(Owned),

        // Metadata columns (populated asynchronously).
        Column::Company => ev.company().map(Borrowed),
        Column::Version => ev.version().map(Borrowed),
        Column::Description => ev.description().map(Borrowed),
    }
}

/// Applies a relation, ASCII-case-insensitively without allocating (the same
/// case folding the previous `to_ascii_lowercase` comparison performed); numeric
/// relations compare as integers when both sides parse as numbers, else fall
/// back to case-insensitive lexicographic order.
fn relation_matches(relation: Relation, actual: &str, expected: &str) -> bool {
    let (a, e) = (actual.as_bytes(), expected.as_bytes());
    match relation {
        Relation::Is => a.eq_ignore_ascii_case(e),
        Relation::IsNot => !a.eq_ignore_ascii_case(e),
        Relation::Contains => contains_ci(a, e),
        Relation::Excludes => !contains_ci(a, e),
        Relation::BeginsWith => a.len() >= e.len() && a[..e.len()].eq_ignore_ascii_case(e),
        Relation::EndsWith => a.len() >= e.len() && a[a.len() - e.len()..].eq_ignore_ascii_case(e),
        Relation::LessThan => compare_ci(actual, expected).is_lt(),
        Relation::MoreThan => compare_ci(actual, expected).is_gt(),
    }
}

/// Case-insensitive substring search. A byte-window scan is correct on UTF-8:
/// multi-byte sequences are self-synchronizing and only ASCII bytes fold, so a
/// window can match only at character boundaries — exactly the matches
/// lowercase-then-`contains` produced.
fn contains_ci(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    if haystack.len() < needle.len() {
        return false;
    }
    haystack
        .windows(needle.len())
        .any(|w| w.eq_ignore_ascii_case(needle))
}

/// Orders two values numerically when both parse as integers, else lexically
/// with ASCII case folding.
fn compare_ci(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a
            .bytes()
            .map(|c| c.to_ascii_lowercase())
            .cmp(b.bytes().map(|c| c.to_ascii_lowercase())),
    }
}

/// Display name of a filterable class value (helper for building Class rules).
pub fn class_label(class: EventClass) -> &'static str {
    match class {
        EventClass::Process => "Process",
        EventClass::File => "File System",
        EventClass::Registry => "Registry",
        EventClass::Profiling => "Profiling",
        EventClass::Network => "Network",
        EventClass::Other => "Unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Event;
    use crate::kernel_types::test_support::entry_bytes;

    fn file_event(notify: u16) -> Event {
        let pre = entry_bytes(3, notify, 1, 0, &[]);
        Event::from_filter(pre.into_boxed_slice(), None, None).unwrap()
    }

    #[test]
    fn no_rules_shows_everything() {
        let fs = FilterSet::default();
        assert!(fs.matches(&file_event(20)));
    }

    #[test]
    fn exclude_hides_match() {
        let fs = FilterSet::new(vec![Rule::new(
            Column::Operation,
            Relation::Is,
            "CreateFile",
            Action::Exclude,
        )]);
        assert!(!fs.matches(&file_event(20))); // CreateFile -> hidden
        assert!(fs.matches(&file_event(21))); // CreateNamedPipe -> shown
    }

    #[test]
    fn include_requires_a_match() {
        let fs = FilterSet::new(vec![Rule::new(
            Column::Operation,
            Relation::Is,
            "CreateFile",
            Action::Include,
        )]);
        assert!(fs.matches(&file_event(20))); // matches the only include -> shown
        assert!(!fs.matches(&file_event(21))); // no include matched -> hidden
    }

    #[test]
    fn multiple_includes_do_not_cancel() {
        // The C++ bug hid events matching one include because another did not;
        // here either include matching is enough to show the event.
        let fs = FilterSet::new(vec![
            Rule::new(
                Column::Operation,
                Relation::Is,
                "CreateFile",
                Action::Include,
            ),
            Rule::new(Column::Operation, Relation::Is, "ReadFile", Action::Include),
        ]);
        assert!(fs.matches(&file_event(20))); // CreateFile matches first include
        assert!(fs.matches(&file_event(23))); // ReadFile matches second include
    }

    #[test]
    fn contains_is_case_insensitive() {
        let fs = FilterSet::new(vec![Rule::new(
            Column::Operation,
            Relation::Contains,
            "createfile",
            Action::Include,
        )]);
        assert!(fs.matches(&file_event(20)));
    }

    #[test]
    fn class_column() {
        let fs = FilterSet::new(vec![Rule::new(
            Column::Class,
            Relation::Is,
            "File System",
            Action::Exclude,
        )]);
        assert!(!fs.matches(&file_event(20)));
    }

    #[test]
    fn pid_column_on_network_event() {
        use crate::event::Event;
        use crate::network::{NetOp, NetworkEvent};
        let net = NetworkEvent {
            pid: 4321,
            is_tcp: true,
            op: NetOp::Send,
            local: "10.0.0.1:5000".parse().unwrap(),
            remote: "1.2.3.4:443".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 12,
            time: 0,
        };
        let ev = Event::from_network(
            std::sync::Arc::new(net),
            crate::event::ProcessSource::Live(None),
        );
        let fs = FilterSet::new(vec![Rule::new(
            Column::Pid,
            Relation::Is,
            "4321",
            Action::Include,
        )]);
        assert!(fs.matches(&ev));
        let fs2 = FilterSet::new(vec![Rule::new(
            Column::Pid,
            Relation::MoreThan,
            "9999",
            Action::Include,
        )]);
        assert!(!fs2.matches(&ev)); // 4321 is not > 9999
    }

    #[test]
    fn scattered_same_column_rules_evaluate_via_memo() {
        // Two Operation rules separated by a Class rule: the per-call memo must
        // serve both correctly regardless of rule order.
        let fs = FilterSet::new(vec![
            Rule::new(
                Column::Operation,
                Relation::BeginsWith,
                "Create",
                Action::Include,
            ),
            Rule::new(Column::Class, Relation::Is, "Process", Action::Exclude),
            Rule::new(
                Column::Operation,
                Relation::EndsWith,
                "Pipe",
                Action::Include,
            ),
        ]);
        assert!(fs.matches(&file_event(21))); // CreateNamedPipe: both includes hit
        assert!(!fs.matches(&file_event(23))); // ReadFile: no include hits
    }

    #[test]
    fn numeric_columns_compare_as_numbers() {
        let ev = file_event(20);
        // `file_event` builds a record with thread_id 0 and sequence 1.
        let is_rule = |v: &str| {
            FilterSet::new(vec![Rule::new(
                Column::Sequence,
                Relation::Is,
                v,
                Action::Include,
            )])
        };
        assert!(is_rule("1").matches(&ev));
        assert!(is_rule("01").matches(&ev)); // numeric, not textual, equality
        assert!(!is_rule("2").matches(&ev));
        // A non-numeric value falls back to the string path (and misses).
        assert!(!is_rule("one").matches(&ev));
    }

    #[test]
    fn substring_relations_on_numeric_columns_stay_textual() {
        use crate::network::{NetOp, NetworkEvent};
        let net = NetworkEvent {
            pid: 4321,
            is_tcp: true,
            op: NetOp::Send,
            local: "10.0.0.1:5000".parse().unwrap(),
            remote: "1.2.3.4:443".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 0,
            time: 0,
        };
        let ev = Event::from_network(
            std::sync::Arc::new(net),
            crate::event::ProcessSource::Live(None),
        );
        let fs = FilterSet::new(vec![Rule::new(
            Column::Pid,
            Relation::Contains,
            "32",
            Action::Include,
        )]);
        assert!(fs.matches(&ev)); // "4321" contains "32"
    }
}
