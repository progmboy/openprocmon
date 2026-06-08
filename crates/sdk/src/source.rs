//! Unified event entry point: live (driver) and PML (file) behind one type.
//!
//! [`EventSource`] is constructed from either source ([`from_driver`](EventSource::from_driver)
//! / [`from_pml`](EventSource::from_pml)) and exposes one consumption view —
//! [`events`](EventSource::events) yields [`Event`]s the same way for both (live is
//! a blocking stream, PML a finite sequential walk). Source-specific controls
//! (live pause/stop/set_monitor, PML random access) are reached by downcasting via
//! [`as_driver`](EventSource::as_driver) / [`as_pml`](EventSource::as_pml), keeping
//! the unified API's semantics clean.

use std::path::Path;
use std::sync::Arc;

use arc_swap::ArcSwap;

use crate::{DriverLoader, Event, FilterSet, MonitorController, MonitorFlags, PmlReader, Result};

enum Source {
    // MonitorController is large (~216 bytes); box it so the enum isn't dominated
    // by the live variant (clippy::large_enum_variant).
    Driver(Box<MonitorController>, crossbeam_channel::Receiver<Event>),
    Pml(Arc<PmlReader>),
}

/// A unified source of [`Event`]s — a live driver capture or an offline PML file.
pub struct EventSource {
    inner: Source,
    filter: Arc<ArcSwap<FilterSet>>,
}

impl EventSource {
    /// Connects to the driver (loading it on demand) and starts capturing `flags`.
    pub fn from_driver(loader: DriverLoader, flags: MonitorFlags) -> Result<Self> {
        let mut ctrl = MonitorController::connect_with_driver(loader)?;
        let rx = ctrl.start_with(flags)?;
        Ok(Self {
            filter: Arc::new(ArcSwap::from_pointee(FilterSet::default())),
            inner: Source::Driver(Box::new(ctrl), rx),
        })
    }

    /// Opens a `.PML` capture for offline reading.
    pub fn from_pml(path: impl AsRef<Path>) -> Result<Self> {
        let reader = Arc::new(PmlReader::open(path)?);
        Ok(Self {
            filter: Arc::new(ArcSwap::from_pointee(FilterSet::default())),
            inner: Source::Pml(reader),
        })
    }

    /// The unified consumption view. Live: a blocking stream over the driver's
    /// channel (consume on a dedicated thread; `stop` via [`as_driver`]). PML: a
    /// finite sequential walk that ends at the last event.
    pub fn events(&self) -> Box<dyn Iterator<Item = Event> + '_> {
        match &self.inner {
            Source::Driver(_, rx) => Box::new(rx.iter()),
            Source::Pml(reader) => Box::new(reader.events()),
        }
    }

    /// Replaces the active filter (pushed to the driver too, for the live source).
    pub fn set_filter(&self, filter: FilterSet) {
        self.filter.store(Arc::new(filter.clone()));
        if let Source::Driver(ctrl, _) = &self.inner {
            ctrl.set_filter(filter);
        }
    }

    /// Whether `ev` passes the active filter.
    pub fn is_visible(&self, ev: &Event) -> bool {
        self.filter.load().matches(ev)
    }

    /// The live controller, for source-specific control (pause/stop/set_monitor).
    pub fn as_driver(&self) -> Option<&MonitorController> {
        match &self.inner {
            Source::Driver(ctrl, _) => Some(ctrl),
            Source::Pml(_) => None,
        }
    }

    /// The PML reader, for source-specific random access (`event_as_event`/`len`).
    pub fn as_pml(&self) -> Option<&Arc<PmlReader>> {
        match &self.inner {
            Source::Pml(reader) => Some(reader),
            Source::Driver(..) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_pml_path() -> std::path::PathBuf {
        use std::io::Read;
        let raw = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/resources")
                .join("CompressedLogFileUTC64FilesystemPML"),
        )
        .expect("read fixture");
        let mut buf = Vec::new();
        flate2::read::ZlibDecoder::new(&raw[..])
            .read_to_end(&mut buf)
            .expect("zlib");
        let tmp =
            std::env::temp_dir().join(format!("openprocmon-source-{}.pml", std::process::id()));
        std::fs::write(&tmp, &buf).expect("write temp pml");
        tmp
    }

    #[test]
    fn from_pml_iterates_and_downcasts() {
        let src = EventSource::from_pml(test_pml_path()).expect("open");
        assert!(src.events().count() > 0);
        assert!(src.as_pml().is_some());
        assert!(src.as_driver().is_none());
    }
}
