//! Reverse-DNS address resolution (cf. design §11.1, Procmon's "Resolve Network
//! Addresses").
//!
//! Resolution is slow, fallible, and highly cacheable, and many events share one
//! IP — the same shape as process metadata. A background worker runs `GetNameInfoW`
//! and stores results in a per-IP cache; lookups are non-blocking and the raw IP
//! is always available, so resolution is a pure overlay.

use parking_lot::RwLock;
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use windows::Win32::Networking::WinSock::{
    socklen_t, GetNameInfoW, WSAStartup, ADDRESS_FAMILY, AF_INET, AF_INET6, IN6_ADDR, IN6_ADDR_0,
    IN_ADDR, IN_ADDR_0, NI_NAMEREQD, SOCKADDR, SOCKADDR_IN, SOCKADDR_IN6, WSADATA,
};

/// One cache slot: filled once by the worker (`None` = resolution failed).
type Slot = Arc<OnceLock<Option<String>>>;

struct Inner {
    enabled: AtomicBool,
    cache: RwLock<HashMap<IpAddr, Slot>>,
}

/// Resolves IP addresses to host names off the hot path, sharing results by IP.
pub struct AddressResolver {
    inner: Arc<Inner>,
    tx: crossbeam_channel::Sender<IpAddr>,
}

impl AddressResolver {
    /// Creates a resolver with a single background worker. Resolution is off until
    /// [`set_enabled(true)`](Self::set_enabled).
    pub fn new() -> Self {
        let inner = Arc::new(Inner {
            enabled: AtomicBool::new(false),
            cache: RwLock::new(HashMap::new()),
        });
        let (tx, rx) = crossbeam_channel::unbounded::<IpAddr>();
        let worker_inner = Arc::clone(&inner);
        std::thread::Builder::new()
            .name("procmon-resolver".into())
            .spawn(move || worker(worker_inner, rx))
            .expect("spawn resolver worker");
        Self { inner, tx }
    }

    /// Enables or disables resolution. When disabled, only raw IPs are returned.
    pub fn set_enabled(&self, on: bool) {
        self.inner.enabled.store(on, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// Requests resolution of `ip` if enabled and not already pending/cached.
    pub fn request(&self, ip: IpAddr) {
        if !self.is_enabled() {
            return;
        }
        let mut cache = self.inner.cache.write();
        if cache.contains_key(&ip) {
            return;
        }
        cache.insert(ip, Arc::new(OnceLock::new()));
        drop(cache);
        let _ = self.tx.send(ip);
    }

    /// Returns the resolved host name for `ip`, or `None` if disabled, not yet
    /// resolved, or unresolvable.
    pub fn host(&self, ip: IpAddr) -> Option<String> {
        self.inner.cache.read().get(&ip)?.get().cloned().flatten()
    }

    /// Formats a socket address, substituting the resolved host when available.
    pub fn format(&self, addr: SocketAddr) -> String {
        match self.host(addr.ip()) {
            Some(host) => format!("{host}:{}", addr.port()),
            None => addr.to_string(),
        }
    }
}

impl Default for AddressResolver {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker loop: resolve each requested IP and fill its cache slot.
fn worker(inner: Arc<Inner>, rx: crossbeam_channel::Receiver<IpAddr>) {
    ensure_winsock();
    while let Ok(ip) = rx.recv() {
        let name = reverse_lookup(ip);
        if let Some(slot) = inner.cache.read().get(&ip) {
            let _ = slot.set(name);
        }
    }
}

/// Initializes Winsock once for the process.
fn ensure_winsock() {
    static STARTED: OnceLock<()> = OnceLock::new();
    STARTED.get_or_init(|| {
        let mut data = WSADATA::default();
        // SAFETY: `data` is a valid out-parameter; request Winsock 2.2.
        unsafe { WSAStartup(0x0202, &mut data) };
    });
}

/// Performs a blocking reverse lookup; `None` if the name cannot be resolved.
fn reverse_lookup(ip: IpAddr) -> Option<String> {
    let mut host = [0u16; 256];
    let status = match ip {
        IpAddr::V4(v4) => {
            let sa = SOCKADDR_IN {
                sin_family: AF_INET,
                sin_port: 0,
                sin_addr: IN_ADDR {
                    S_un: IN_ADDR_0 {
                        S_addr: u32::from_ne_bytes(v4.octets()),
                    },
                },
                sin_zero: [0; 8],
            };
            // SAFETY: `sa` is a valid SOCKADDR_IN; length matches its size.
            unsafe {
                GetNameInfoW(
                    &sa as *const SOCKADDR_IN as *const SOCKADDR,
                    socklen_t(size_of::<SOCKADDR_IN>() as i32),
                    Some(&mut host),
                    None,
                    NI_NAMEREQD as i32,
                )
            }
        }
        IpAddr::V6(v6) => {
            let sa = SOCKADDR_IN6 {
                sin6_family: AF_INET6,
                sin6_port: 0,
                sin6_flowinfo: 0,
                sin6_addr: IN6_ADDR {
                    u: IN6_ADDR_0 { Byte: v6.octets() },
                },
                Anonymous: Default::default(),
            };
            // SAFETY: `sa` is a valid SOCKADDR_IN6; length matches its size.
            unsafe {
                GetNameInfoW(
                    &sa as *const SOCKADDR_IN6 as *const SOCKADDR,
                    socklen_t(size_of::<SOCKADDR_IN6>() as i32),
                    Some(&mut host),
                    None,
                    NI_NAMEREQD as i32,
                )
            }
        }
    };
    if status != 0 {
        return None;
    }
    let end = host.iter().position(|&c| c == 0).unwrap_or(host.len());
    let name = String::from_utf16_lossy(&host[..end]);
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

use core::mem::size_of;
// Silence unused import warnings if a target trims address-family usage.
const _: ADDRESS_FAMILY = AF_INET;
