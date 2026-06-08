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
pub trait FilterFields {
    /// The column's value as a string, or `None` if the event has none.
    fn filter_field(&self, column: Column) -> Option<String>;
}

impl FilterFields for Event {
    fn filter_field(&self, column: Column) -> Option<String> {
        column_value(self, column)
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
    /// Only the columns referenced by enabled rules are materialized, keeping the
    /// common (empty/short) filter cheap on the hot path.
    pub fn matches<E: FilterFields>(&self, ev: &E) -> bool {
        let mut has_include = false;
        let mut include_hit = false;

        for rule in self.rules.iter().filter(|r| r.enabled) {
            let hit = rule_hits(ev, rule);
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
        for rule in self.rules.iter().filter(|r| r.enabled) {
            let hit = rule_hits(ev, rule);
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

/// Whether `rule`'s relation matches `ev`'s value for the rule's column.
fn rule_hits<E: FilterFields>(ev: &E, rule: &Rule) -> bool {
    ev.filter_field(rule.column)
        .map(|actual| relation_matches(rule.relation, &actual, &rule.value))
        .unwrap_or(false)
}

/// Extracts the comparison string for a column, or `None` if the event has none.
///
/// Delegates to the [`Event`] accessors, which return `None` when no process is
/// attached (network or untracked) and for metadata columns until the async
/// worker fills them. Time columns are formatted as local time strings.
fn column_value(ev: &Event, column: Column) -> Option<String> {
    let yes_no =
        |b: bool, yes: &'static str, no: &'static str| if b { yes } else { no }.to_string();
    match column {
        // Event-intrinsic columns.
        Column::Operation => Some(ev.operation_name().to_string()),
        Column::Path => ev.path(),
        Column::Result => Some(ev.result().into_owned()),
        Column::Detail => Some(ev.detail()),
        Column::Class => Some(ev.class_name().to_string()),
        Column::Pid => Some(ev.pid().to_string()),
        Column::Tid => Some(ev.thread_id().to_string()),
        Column::Sequence => Some(ev.sequence().to_string()),
        // Full precision so Date & Time `LessThan`/`MoreThan` compares to the tick,
        // not just the second.
        Column::Date => Some(ev.date_precise()),
        Column::TimeOfDay => Some(ev.time_of_day()),
        Column::CompletionTime => ev.completion_time(),
        Column::Duration => ev.duration(),

        // Process-derived columns.
        Column::ProcessName => ev.process_name().map(str::to_string),
        Column::ImagePath => ev
            .image_path()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        Column::CommandLine => ev
            .command_line()
            .filter(|s| !s.is_empty())
            .map(str::to_string),
        Column::ParentPid => ev.parent_pid().map(|v| v.to_string()),
        Column::Session => ev.session_id().map(|v| v.to_string()),
        Column::Architecture => ev.is_wow64().map(|w| yes_no(w, "32-bit", "64-bit")),
        Column::Virtualized => ev.is_virtualized().map(|v| yes_no(v, "True", "False")),
        Column::AuthId => ev.auth_id(),
        Column::Integrity => ev.integrity().map(str::to_string),
        Column::User => ev.user(),

        // Metadata columns (populated asynchronously).
        Column::Company => ev.company().map(str::to_string),
        Column::Version => ev.version().map(str::to_string),
        Column::Description => ev.description().map(str::to_string),
    }
}

/// Applies a relation, case-insensitively; numeric relations compare as integers
/// when both sides parse as numbers, else fall back to lexicographic order.
fn relation_matches(relation: Relation, actual: &str, expected: &str) -> bool {
    let a = actual.to_ascii_lowercase();
    let e = expected.to_ascii_lowercase();
    match relation {
        Relation::Is => a == e,
        Relation::IsNot => a != e,
        Relation::Contains => a.contains(&e),
        Relation::Excludes => !a.contains(&e),
        Relation::BeginsWith => a.starts_with(&e),
        Relation::EndsWith => a.ends_with(&e),
        Relation::LessThan => compare(&a, &e).is_lt(),
        Relation::MoreThan => compare(&a, &e).is_gt(),
    }
}

/// Orders two values numerically when both parse as integers, else lexically.
fn compare(a: &str, b: &str) -> std::cmp::Ordering {
    match (a.parse::<i64>(), b.parse::<i64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        _ => a.cmp(b),
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
}
