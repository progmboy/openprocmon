//! PML (Process Monitor Log) reading and writing.
//!
//! PML is Procmon's on-disk capture format (version 9, uncompressed). The layout
//! was reverse-engineered; this is a Rust port of the reference Python parser
//! `ref-code/procmon-parser` plus a writer (the reference has none). The format is
//! offset-graph shaped (a header pointing at independent string/process/event/host
//! arrays), so the reader memory-maps the file and parses lazily for random access
//! (see [`reader`]); the writer streams events and back-fills the header offsets
//! (see [`writer`]).

mod detail;
pub mod model;
pub mod reader;
pub mod writer;

pub use model::{PmlIcon, PmlModule, PmlProcess};
pub use reader::PmlReader;
pub use writer::PmlWriter;
