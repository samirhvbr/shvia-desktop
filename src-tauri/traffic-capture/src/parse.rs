//! Turns one raw Ethernet frame into the handful of fields the Traffic table shows
//! (source, destination, protocol, length, one-line summary). Deliberately shallow: no
//! protocol-specific payload dissection beyond L2-L4 headers, matching the documented scope
//! of this first version (see docs/TRAFFIC/README.md).

use etherparse::{LinkSlice, NetSlice, SlicedPacket, TransportSlice};
use serde::Serialize;

#[derive(Serialize, Clone)]
pub struct PacketSummary {
    pub seq: u64,
    pub ts_unix_ms: u64,
    pub src: String,
    pub dst: String,
    pub protocol: String,
    pub length: u32,
    pub summary: String,
}

pub fn summarize(seq: u64, ts_unix_ms: u64, length: u32, raw: &[u8]) -> PacketSummary {
    let fallback = || PacketSummary {
        seq,
        ts_unix_ms,
        src: "?".into(),
        dst: "?".into(),
        protocol: "unknown".into(),
        length,
        summary: format!("{length} bytes, unrecognized framing"),
    };

    let Ok(sliced) = SlicedPacket::from_ethernet(raw) else {
        return fallback();
    };

    let (mut src, mut dst, mut protocol) = ("?".to_string(), "?".to_string(), "ethernet".to_string());

    if let Some(LinkSlice::Ethernet2(eth)) = &sliced.link {
        src = mac_to_string(eth.source());
        dst = mac_to_string(eth.destination());
    }

    match &sliced.net {
        Some(NetSlice::Ipv4(ip)) => {
            src = ip.header().source_addr().to_string();
            dst = ip.header().destination_addr().to_string();
            protocol = "IPv4".into();
        }
        Some(NetSlice::Ipv6(ip)) => {
            src = ip.header().source_addr().to_string();
            dst = ip.header().destination_addr().to_string();
            protocol = "IPv6".into();
        }
        _ => {}
    }

    let mut ports = String::new();
    match &sliced.transport {
        Some(TransportSlice::Tcp(tcp)) => {
            protocol = "TCP".into();
            ports = format!(":{} -> :{}", tcp.source_port(), tcp.destination_port());
        }
        Some(TransportSlice::Udp(udp)) => {
            protocol = "UDP".into();
            ports = format!(":{} -> :{}", udp.source_port(), udp.destination_port());
        }
        Some(TransportSlice::Icmpv4(_)) => protocol = "ICMPv4".into(),
        Some(TransportSlice::Icmpv6(_)) => protocol = "ICMPv6".into(),
        _ => {}
    }

    let summary = if ports.is_empty() {
        format!("{protocol} {src} -> {dst}, {length} bytes")
    } else {
        format!("{protocol} {src} {ports}, {length} bytes")
    };

    PacketSummary { seq, ts_unix_ms, src, dst, protocol, length, summary }
}

fn mac_to_string(mac: [u8; 6]) -> String {
    mac.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal Ethernet-II + IPv4 + UDP frame, built by hand so this test does not depend
    /// on ever opening a real device. Header fields matter here, not the (empty) payload.
    fn udp_ipv4_frame() -> Vec<u8> {
        let mut f = Vec::new();
        f.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff]); // dst mac
        f.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66]); // src mac
        f.extend_from_slice(&[0x08, 0x00]); // ethertype IPv4
        let udp_len = 8u16;
        let ip_total_len = 20u16 + udp_len;
        f.extend_from_slice(&[0x45, 0x00]); // version/ihl, dscp
        f.extend_from_slice(&ip_total_len.to_be_bytes());
        f.extend_from_slice(&[0, 0, 0, 0]); // id, flags/frag
        f.push(64); // ttl
        f.push(17); // protocol: UDP
        f.extend_from_slice(&[0, 0]); // checksum (unchecked by etherparse's sliced reader)
        f.extend_from_slice(&[10, 0, 0, 1]); // src ip
        f.extend_from_slice(&[10, 0, 0, 2]); // dst ip
        f.extend_from_slice(&1234u16.to_be_bytes()); // src port
        f.extend_from_slice(&53u16.to_be_bytes()); // dst port
        f.extend_from_slice(&udp_len.to_be_bytes());
        f.extend_from_slice(&[0, 0]); // udp checksum
        f
    }

    #[test]
    fn a_udp_ipv4_frame_yields_ip_and_port_summary() {
        let frame = udp_ipv4_frame();
        let s = summarize(1, 0, frame.len() as u32, &frame);
        assert_eq!(s.src, "10.0.0.1");
        assert_eq!(s.dst, "10.0.0.2");
        assert_eq!(s.protocol, "UDP");
        assert!(s.summary.contains(":1234"));
        assert!(s.summary.contains(":53"));
    }

    #[test]
    fn garbage_bytes_fall_back_instead_of_panicking() {
        let s = summarize(1, 0, 3, &[0u8, 1, 2]);
        assert_eq!(s.protocol, "unknown");
    }
}
