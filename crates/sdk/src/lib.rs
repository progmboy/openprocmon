//! `procmon-sdk`: a Rust SDK for the OpenProcessMonitor kernel miniFilter driver.
//!
//! The SDK connects to the driver's Filter Manager port, receives batches of
//! process/file/registry records, correlates pending operations with their
//! completions, and presents them as lightweight [`Event`] handles with lazy,
//! zero-allocation accessors. The kernel/user-mode wire format is defined in
//! `kernel/logsdk.h` and mirrored in [`kernel_types`].

pub mod driver;
pub mod error;
pub mod event;
pub mod filter;
pub mod kernel_types;
pub mod message;
pub mod metadata;
pub mod monitor;
pub mod network;
pub mod parse;
pub mod path;
mod pipeline;
pub mod pml;
pub mod port;
pub mod process;
pub mod resolver;
pub mod sid;
pub mod source;
pub mod strings;
pub mod symbols;
pub mod time;

pub use driver::DriverLoader;
pub use error::{Error, Result};
pub use event::{Event, EventClass};
pub use filter::{Action, Column, FilterFields, FilterSet, Relation, Rule};
pub use monitor::{MonitorController, MonitorFlags};
pub use network::{NetOp, NetworkEvent};
pub use parse::{parse_block, parse_block_tracked};

pub use process::{Module, ProcessInfo, ProcessManager, ProcessMeta, ProcessRecord};
pub use pml::{PmlIcon, PmlProcess, PmlReader, PmlWriter};
pub use resolver::AddressResolver;
pub use source::EventSource;
pub use symbols::{SymModule, SymbolResolver};
