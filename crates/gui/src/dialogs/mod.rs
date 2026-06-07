//! Modal dialogs (filter, and later highlight / process tree / summary).
//!
//! Dialog bodies that react to edits are view entities (gpui-component renders the
//! dialog layer from `Root`, so an inline closure would not re-render on its own).

pub(crate) mod about;
pub(crate) mod filter_dialog;
pub(crate) mod form_dialog;
pub(crate) mod path_summary;
pub(crate) mod process_summary;
pub(crate) mod process_tree;
pub(crate) mod save_dialog;
pub(crate) mod settings_dialog;
pub(crate) mod summary;
