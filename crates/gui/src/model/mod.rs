//! GUI domain model and the event-source boundary.
//!
//! `domain` holds owned, render-ready types; `filter` the GUI filter model;
//! `source` the [`EventSource`](source::EventSource) trait; `buffer` the retained
//! event store; `sdk_source` maps real `procmon_sdk::Event`s (live + PML) into the
//! same domain types.

pub(crate) mod buffer;
pub(crate) mod config;
pub(crate) mod domain;
pub(crate) mod filter;
pub(crate) mod sdk_source;
pub(crate) mod source;
