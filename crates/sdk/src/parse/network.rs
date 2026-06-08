//! Network event decoding and rendering (cf. C++ `netopt`/Procmon's network row).
//!
//! Network events have two on-disk shapes that decode to the same [`NetworkEvent`]
//! model — they are NOT the same bytes (unlike file/registry/process, whose PML
//! detail blob is literally the driver `EventData`):
//!
//! * **ETW (live):** classic TcpIp/UdpIp MOF `TypeGroup1` — protocol from the
//!   provider GUID, IPv4/IPv6 and operation from the opcode, ports big-endian.
//!   [`parse_group1_v4`]/[`parse_group1_v6`]/[`classify_etw`] handle it; the ETW
//!   session itself lives in [`crate::network`].
//! * **PML (file):** Procmon's own re-serialization — a flags word (src/dst v4,
//!   tcp), length, 16-byte src/dst, ports little-endian, plus host/port name
//!   tables. [`decode_pml`] handles it.
//!
//! Both feed [`NetView`], the single place that renders the Path (`local -> remote`,
//! resolved names when present) and Detail, so live and PML stay consistent.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::sync::Arc;

use crate::event::Event;
use crate::network::{NetOp, NetworkEvent};
use crate::parse::OperationView;

// Classic TcpIp/UdpIp event types (EVENT_DESCRIPTOR.Opcode). These are the IPv4
// opcodes; the IPv6 variants are the same value plus `IPV6_OPCODE_OFFSET`
// (e.g. Send=10, SendIPV6=26).
const ET_SEND: u8 = 10;
const ET_RECV: u8 = 11;
const ET_CONNECT: u8 = 12;
const ET_DISCONNECT: u8 = 13;
const ET_ACCEPT: u8 = 15;
const IPV6_OPCODE_OFFSET: u8 = 16;

/// Procmon-style operation label, e.g. `TCP Connect` / `UDP Send` — the base
/// [`NetOp::name`] prefixed with the protocol. Returns `&'static str` so it can
/// flow through [`crate::strings::operation`].
pub(crate) fn op_label(is_tcp: bool, op: NetOp) -> &'static str {
    match (is_tcp, op) {
        (true, NetOp::Unknown) => "TCP Unknown",
        (true, NetOp::Other) => "TCP Other",
        (true, NetOp::Send) => "TCP Send",
        (true, NetOp::Receive) => "TCP Receive",
        (true, NetOp::Accept) => "TCP Accept",
        (true, NetOp::Connect) => "TCP Connect",
        (true, NetOp::Disconnect) => "TCP Disconnect",
        (true, NetOp::Reconnect) => "TCP Reconnect",
        (true, NetOp::Retransmit) => "TCP Retransmit",
        (true, NetOp::TcpCopy) => "TCP TCPCopy",
        (false, NetOp::Unknown) => "UDP Unknown",
        (false, NetOp::Other) => "UDP Other",
        (false, NetOp::Send) => "UDP Send",
        (false, NetOp::Receive) => "UDP Receive",
        (false, NetOp::Accept) => "UDP Accept",
        (false, NetOp::Connect) => "UDP Connect",
        (false, NetOp::Disconnect) => "UDP Disconnect",
        (false, NetOp::Reconnect) => "UDP Reconnect",
        (false, NetOp::Retransmit) => "UDP Retransmit",
        (false, NetOp::TcpCopy) => "UDP TCPCopy",
    }
}

/// Renders a network event's Path/Detail. Construct from a decoded
/// [`NetworkEvent`] (live or PML); the same instance backs `Event::path`/`detail`.
pub(crate) struct NetView<'a> {
    net: &'a NetworkEvent,
}

impl<'a> NetView<'a> {
    pub(crate) fn new(net: &'a NetworkEvent) -> Self {
        Self { net }
    }

    /// The operation label (`TCP Connect`, …).
    #[allow(dead_code)] // used by the PML detail decode path (round-trip / comparison tests)
    pub(crate) fn op_label(&self) -> &'static str {
        op_label(self.net.is_tcp, self.net.op)
    }

    fn endpoint(name: &Option<Arc<str>>, addr: &SocketAddr) -> String {
        match name {
            Some(n) if !n.is_empty() => n.to_string(),
            _ => addr.to_string(),
        }
    }
}

impl OperationView for NetView<'_> {
    fn path(&self) -> Option<String> {
        Some(format!(
            "{} -> {}",
            Self::endpoint(&self.net.local_name, &self.net.local),
            Self::endpoint(&self.net.remote_name, &self.net.remote),
        ))
    }

    fn detail(&self) -> String {
        format!("Length: {}", self.net.length)
    }
}

// ---------------------------------------------------------------------------
// ETW (live) MOF decoding.
// ---------------------------------------------------------------------------

/// Maps a classic TcpIp/UdpIp opcode to its `(operation, is_ipv6)`. IPv6 events
/// carry the IPv4 opcode plus [`IPV6_OPCODE_OFFSET`], so an opcode in the IPv6
/// range is shifted back down before matching. Returns `None` for opcodes we
/// don't model.
pub(crate) fn classify_etw(is_tcp: bool, raw_opcode: u8) -> Option<(NetOp, bool)> {
    let ipv6_lo = ET_SEND + IPV6_OPCODE_OFFSET;
    let ipv6_hi = ET_ACCEPT + IPV6_OPCODE_OFFSET;
    let is_ipv6 = (ipv6_lo..=ipv6_hi).contains(&raw_opcode);
    let opcode = if is_ipv6 {
        raw_opcode - IPV6_OPCODE_OFFSET
    } else {
        raw_opcode
    };
    let op = match (is_tcp, opcode) {
        (true, ET_CONNECT) => NetOp::Connect,
        (true, ET_SEND) => NetOp::Send,
        (true, ET_RECV) => NetOp::Receive,
        (true, ET_DISCONNECT) => NetOp::Disconnect,
        (true, ET_ACCEPT) => NetOp::Accept,
        (false, ET_SEND) => NetOp::Send,
        (false, ET_RECV) => NetOp::Receive,
        _ => return None,
    };
    Some((op, is_ipv6))
}

/// IPv4 `TypeGroup1`: PID(4) size(4) daddr(4) saddr(4) dport(2) sport(2).
pub(crate) fn parse_group1_v4(d: &[u8]) -> Option<(SocketAddr, SocketAddr, u32)> {
    let size = u32::from_le_bytes(d.get(4..8)?.try_into().ok()?);
    let daddr = Ipv4Addr::new(d[8], d[9], d[10], d[11]);
    let saddr = Ipv4Addr::new(d[12], d[13], d[14], d[15]);
    let dport = u16::from_be_bytes(d.get(16..18)?.try_into().ok()?);
    let sport = u16::from_be_bytes(d.get(18..20)?.try_into().ok()?);
    let local = SocketAddr::new(IpAddr::V4(saddr), sport);
    let remote = SocketAddr::new(IpAddr::V4(daddr), dport);
    Some((local, remote, size))
}

/// IPv6 `TypeGroup1`: PID(4) size(4) daddr(16) saddr(16) dport(2) sport(2).
pub(crate) fn parse_group1_v6(d: &[u8]) -> Option<(SocketAddr, SocketAddr, u32)> {
    let size = u32::from_le_bytes(d.get(4..8)?.try_into().ok()?);
    let daddr = Ipv6Addr::from(<[u8; 16]>::try_from(d.get(8..24)?).ok()?);
    let saddr = Ipv6Addr::from(<[u8; 16]>::try_from(d.get(24..40)?).ok()?);
    let dport = u16::from_be_bytes(d.get(40..42)?.try_into().ok()?);
    let sport = u16::from_be_bytes(d.get(42..44)?.try_into().ok()?);
    let local = SocketAddr::new(IpAddr::V6(saddr), sport);
    let remote = SocketAddr::new(IpAddr::V6(daddr), dport);
    Some((local, remote, size))
}

// ---------------------------------------------------------------------------
// PML detail-blob decoding.
// ---------------------------------------------------------------------------

/// Decodes a PML network detail blob into a [`NetworkEvent`], resolving the
/// endpoints' `host:port` display strings from the file's name/port tables.
///
/// Blob layout: flags(2) [bit0 src-v4, bit1 dst-v4, bit2 tcp], skip(2), len(4),
/// src(16), dst(16), src-port(2), dst-port(2). Addresses are always 16 bytes
/// (IPv4 in the first 4); ports are little-endian (already host order).
pub(crate) fn decode_pml(
    op_code: u16,
    blob: &[u8],
    hosts: &HashMap<[u8; 16], Arc<str>>,
    ports: &HashMap<(u16, bool), Arc<str>>,
) -> Option<NetworkEvent> {
    let flags = u16::from_le_bytes(blob.get(0..2)?.try_into().ok()?);
    let is_src_v4 = flags & 1 != 0;
    let is_dst_v4 = flags & 2 != 0;
    let is_tcp = flags & 4 != 0;
    let length = u32::from_le_bytes(blob.get(4..8)?.try_into().ok()?);
    let src: [u8; 16] = blob.get(8..24)?.try_into().ok()?;
    let dst: [u8; 16] = blob.get(24..40)?.try_into().ok()?;
    let src_port = u16::from_le_bytes(blob.get(40..42)?.try_into().ok()?);
    let dst_port = u16::from_le_bytes(blob.get(42..44)?.try_into().ok()?);

    let local = SocketAddr::new(ip_from(&src, is_src_v4), src_port);
    let remote = SocketAddr::new(ip_from(&dst, is_dst_v4), dst_port);
    let local_name = Some(Arc::from(
        format!(
            "{}:{}",
            host(hosts, &src, is_src_v4),
            port(ports, src_port, is_tcp)
        )
        .as_str(),
    ));
    let remote_name = Some(Arc::from(
        format!(
            "{}:{}",
            host(hosts, &dst, is_dst_v4),
            port(ports, dst_port, is_tcp)
        )
        .as_str(),
    ));

    Some(NetworkEvent {
        pid: 0,
        is_tcp,
        op: NetOp::from_pml(op_code),
        local,
        remote,
        local_name,
        remote_name,
        length,
        time: 0,
    })
}

/// Serializes a network `Event`'s detail into PML form, mirroring the other
/// categories' `pml_detail`. Network has no driver `EventData`; this encodes the
/// decoded [`NetworkEvent`] via [`encode_pml`]. `None` if not a network event.
pub(crate) fn pml_detail(ev: &Event) -> Option<Vec<u8>> {
    ev.network().map(|n| encode_pml(n))
}

/// Encodes a [`NetworkEvent`] into a PML network detail blob (inverse of
/// [`decode_pml`]). Endpoints are written numerically; name resolution is the
/// PML host/port tables' job (a live capture has none, so the reader renders
/// numeric — faithful to what was observed). The operation code goes in the
/// PML event's `operation` field (`NetOp::to_pml`), not here.
pub(crate) fn encode_pml(net: &NetworkEvent) -> Vec<u8> {
    let (src_v4, src_ip) = ip_bytes(&net.local);
    let (dst_v4, dst_ip) = ip_bytes(&net.remote);
    let flags = (src_v4 as u16) | ((dst_v4 as u16) << 1) | ((net.is_tcp as u16) << 2);
    let mut b = Vec::with_capacity(44);
    b.extend_from_slice(&flags.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes()); // skip
    b.extend_from_slice(&net.length.to_le_bytes());
    b.extend_from_slice(&src_ip);
    b.extend_from_slice(&dst_ip);
    b.extend_from_slice(&net.local.port().to_le_bytes());
    b.extend_from_slice(&net.remote.port().to_le_bytes());
    b
}

/// Splits a socket address into `(is_v4, 16-byte buffer)` — IPv4 in the first 4.
fn ip_bytes(addr: &SocketAddr) -> (bool, [u8; 16]) {
    match addr.ip() {
        IpAddr::V4(a) => {
            let mut o = [0u8; 16];
            o[..4].copy_from_slice(&a.octets());
            (true, o)
        }
        IpAddr::V6(a) => (false, a.octets()),
    }
}

fn ip_from(ip: &[u8; 16], is_v4: bool) -> IpAddr {
    if is_v4 {
        IpAddr::V4(Ipv4Addr::new(ip[0], ip[1], ip[2], ip[3]))
    } else {
        IpAddr::V6(Ipv6Addr::from(*ip))
    }
}

/// Resolved host name from the PML host table, falling back to the numeric IP.
fn host(hosts: &HashMap<[u8; 16], Arc<str>>, ip: &[u8; 16], is_v4: bool) -> String {
    if let Some(h) = hosts.get(ip) {
        if !h.is_empty() {
            return h.to_string();
        }
    }
    match ip_from(ip, is_v4) {
        IpAddr::V4(a) => a.to_string(),
        IpAddr::V6(a) => a.to_string(),
    }
}

/// Resolved service name from the PML port table, falling back to the number.
fn port(ports: &HashMap<(u16, bool), Arc<str>>, port: u16, is_tcp: bool) -> String {
    match ports.get(&(port, is_tcp)) {
        Some(name) if !name.is_empty() => name.to_string(),
        _ => port.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ipv4_group1_offsets() {
        // PID=0x1234, size=100, daddr=1.2.3.4, saddr=10.0.0.5, dport=443, sport=51000.
        let mut d = Vec::new();
        d.extend(0x1234u32.to_le_bytes());
        d.extend(100u32.to_le_bytes());
        d.extend([1, 2, 3, 4]);
        d.extend([10, 0, 0, 5]);
        d.extend(443u16.to_be_bytes());
        d.extend(51000u16.to_be_bytes());
        let (local, remote, len) = parse_group1_v4(&d).unwrap();
        assert_eq!(len, 100);
        assert_eq!(remote, "1.2.3.4:443".parse().unwrap());
        assert_eq!(local, "10.0.0.5:51000".parse().unwrap());
    }

    #[test]
    fn decodes_ipv6_group1_offsets() {
        // PID=0x55, size=200, daddr=::1, saddr=fe80::1, dport=80, sport=40000.
        let mut d = Vec::new();
        d.extend(0x55u32.to_le_bytes());
        d.extend(200u32.to_le_bytes());
        d.extend(Ipv6Addr::LOCALHOST.octets());
        d.extend("fe80::1".parse::<Ipv6Addr>().unwrap().octets());
        d.extend(80u16.to_be_bytes());
        d.extend(40000u16.to_be_bytes());
        let (local, remote, len) = parse_group1_v6(&d).unwrap();
        assert_eq!(len, 200);
        assert_eq!(remote, "[::1]:80".parse().unwrap());
        assert_eq!(local, "[fe80::1]:40000".parse().unwrap());
    }

    #[test]
    fn classify_picks_op_and_family_from_opcode() {
        assert_eq!(
            classify_etw(true, ET_CONNECT),
            Some((NetOp::Connect, false))
        );
        assert_eq!(classify_etw(false, ET_SEND), Some((NetOp::Send, false)));
        // IPv6 variants are opcode + 16, and must be flagged as IPv6.
        assert_eq!(
            classify_etw(true, ET_CONNECT + IPV6_OPCODE_OFFSET),
            Some((NetOp::Connect, true))
        );
        assert_eq!(
            classify_etw(true, ET_ACCEPT + IPV6_OPCODE_OFFSET),
            Some((NetOp::Accept, true))
        );
        assert_eq!(
            classify_etw(false, ET_RECV + IPV6_OPCODE_OFFSET),
            Some((NetOp::Receive, true))
        );
        // Unmodeled opcodes (e.g. retransmit=14 and its IPv6 form) are dropped.
        assert_eq!(classify_etw(true, 14), None);
        assert_eq!(classify_etw(true, 14 + IPV6_OPCODE_OFFSET), None);
    }

    #[test]
    fn op_labels() {
        assert_eq!(op_label(true, NetOp::Connect), "TCP Connect");
        assert_eq!(op_label(false, NetOp::Send), "UDP Send");
    }

    #[test]
    fn encode_decode_pml_round_trip() {
        let net = NetworkEvent {
            pid: 0,
            is_tcp: false,
            op: NetOp::Send,
            local: "[fe80::1]:1234".parse().unwrap(),
            remote: "[::1]:53".parse().unwrap(),
            local_name: None,
            remote_name: None,
            length: 64,
            time: 0,
        };
        let blob = encode_pml(&net);
        let hosts = HashMap::new();
        let ports = HashMap::new();
        let back = decode_pml(net.op.to_pml(), &blob, &hosts, &ports).expect("decode");
        assert_eq!(back.is_tcp, net.is_tcp);
        assert_eq!(back.op, net.op);
        assert_eq!(back.local, net.local);
        assert_eq!(back.remote, net.remote);
        assert_eq!(back.length, net.length);
    }

    #[test]
    fn pml_blob_decodes_and_renders() {
        // flags = src-v4|dst-v4|tcp = 0b111; len=512; src 10.0.0.1:1234 -> dst 1.2.3.4:443.
        let mut b = Vec::new();
        b.extend(0b111u16.to_le_bytes());
        b.extend(0u16.to_le_bytes()); // skip
        b.extend(512u32.to_le_bytes());
        let mut src = [0u8; 16];
        src[..4].copy_from_slice(&[10, 0, 0, 1]);
        let mut dst = [0u8; 16];
        dst[..4].copy_from_slice(&[1, 2, 3, 4]);
        b.extend_from_slice(&src);
        b.extend_from_slice(&dst);
        b.extend(1234u16.to_le_bytes());
        b.extend(443u16.to_le_bytes());

        let hosts = HashMap::new();
        let ports = HashMap::new();
        let net = decode_pml(5 /* Connect */, &b, &hosts, &ports).expect("decode");
        let view = NetView::new(&net);
        assert_eq!(view.op_label(), "TCP Connect");
        assert_eq!(view.path().as_deref(), Some("10.0.0.1:1234 -> 1.2.3.4:443"));
        assert_eq!(view.detail(), "Length: 512");
    }
}
