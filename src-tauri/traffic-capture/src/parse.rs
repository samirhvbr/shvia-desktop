//! Turns one captured frame into the fields the Traffic table shows (source, destination,
//! protocol, length, one-line summary) and a Wireshark-style hex dump. Deliberately shallow: no
//! protocol-specific payload dissection beyond L2-L4 headers, matching the documented scope of
//! this version (see docs/TRAFFIC/README.md).
//!
//! The frame's link layer depends on the interface: Ethernet on most, BSD loopback framing on
//! macOS/Windows loopback, Linux "cooked" framing on `any`, raw IP on tunnels. `LinkKind` names the
//! ones this file decodes; anything else is still captured and exported byte for byte, only the
//! table row says it is not decoded.

use etherparse::{
    EtherType, LaxNetSlice, LaxSlicedPacket, LinkSlice, TransportSlice,
};
use serde::Serialize;

/// libpcap link types (`pcap_datalink`) this sidecar decodes. The numbers are the stable
/// DLT/LINKTYPE values, identical on Linux, macOS and Npcap.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkKind {
    Ethernet,
    /// BSD loopback (`lo0` on macOS, Npcap loopback): a 4-byte address family, then an IP packet.
    Null,
    /// A bare IPv4/IPv6 packet (tunnels).
    Raw,
    /// Linux "cooked" capture v1 (`any`): a 16-byte pseudo header.
    LinuxSll,
    /// Linux "cooked" capture v2: a 20-byte pseudo header.
    LinuxSll2,
    Other(i32),
}

impl LinkKind {
    pub fn from_dlt(dlt: i32) -> Self {
        match dlt {
            1 => Self::Ethernet,
            0 => Self::Null,
            12 | 101 => Self::Raw,
            113 => Self::LinuxSll,
            276 => Self::LinuxSll2,
            other => Self::Other(other),
        }
    }
}

/// What the table needs from one frame; the sidecar adds the packet number, the time and the hex.
#[derive(Debug, PartialEq, Eq)]
pub struct Summary {
    pub src: String,
    pub dst: String,
    pub protocol: String,
    pub summary: String,
}

/// One row as the page receives it: the exact shape `public/js/traffic.js` reads.
#[derive(Serialize, Clone, Debug)]
pub struct PacketView {
    pub id: String,
    pub time: String,
    pub src: String,
    pub dst: String,
    pub protocol: String,
    pub length: u32,
    pub summary: String,
    pub hex: String,
}

/// How many bytes of a frame the page gets as hex. The whole frame is always in the capture file;
/// the stream to the page is bounded so a flood of jumbo frames cannot make the webview choke.
pub const HEX_BYTES_SENT: usize = 512;

pub fn summarize(link: LinkKind, raw: &[u8], wire_len: u32) -> Summary {
    let undecoded = |protocol: &str, why: String| Summary {
        src: "?".into(),
        dst: "?".into(),
        protocol: protocol.into(),
        summary: format!("{wire_len} bytes, {why}"),
    };

    let mut src = "?".to_string();
    let mut dst = "?".to_string();
    let mut link_ether_type: Option<EtherType> = None;

    let parsed = match link {
        LinkKind::Ethernet => {
            let Ok(p) = LaxSlicedPacket::from_ethernet(raw) else {
                return undecoded("unknown", "shorter than an Ethernet header".into());
            };
            if let Some(LinkSlice::Ethernet2(eth)) = &p.link {
                src = mac(&eth.source());
                dst = mac(&eth.destination());
                link_ether_type = Some(eth.ether_type());
            }
            p
        }
        LinkKind::Null => {
            if raw.len() < 4 {
                return undecoded("unknown", "shorter than the loopback header".into());
            }
            match LaxSlicedPacket::from_ip(&raw[4..]) {
                Ok(p) => p,
                Err(_) => return undecoded("unknown", "loopback frame that is not IP".into()),
            }
        }
        LinkKind::Raw => match LaxSlicedPacket::from_ip(raw) {
            Ok(p) => p,
            Err(_) => return undecoded("unknown", "raw frame that is not IP".into()),
        },
        LinkKind::LinuxSll | LinkKind::LinuxSll2 => {
            let (header_len, type_at, addr_at) = if link == LinkKind::LinuxSll { (16, 14, 6) } else { (20, 0, 12) };
            if raw.len() < header_len {
                return undecoded("unknown", "shorter than the cooked-capture header".into());
            }
            let ether_type = EtherType(u16::from_be_bytes([raw[type_at], raw[type_at + 1]]));
            link_ether_type = Some(ether_type);
            // The pseudo header carries the link-layer address of the OTHER end only.
            let addr_len = if link == LinkKind::LinuxSll {
                usize::from(u16::from_be_bytes([raw[4], raw[5]]))
            } else {
                usize::from(raw[11])
            };
            if addr_len == 6 {
                src = mac(&raw[addr_at..addr_at + 6]);
            }
            LaxSlicedPacket::from_ether_type(ether_type, &raw[header_len..])
        }
        LinkKind::Other(n) => {
            return undecoded(
                &format!("linktype {n}"),
                format!("link type {n} is not decoded here; the exported file has the bytes"),
            );
        }
    };

    let mut protocol = "ethernet".to_string();
    match &parsed.net {
        Some(LaxNetSlice::Ipv4(ip)) => {
            src = ip.header().source_addr().to_string();
            dst = ip.header().destination_addr().to_string();
            protocol = "IPv4".into();
        }
        Some(LaxNetSlice::Ipv6(ip)) => {
            src = ip.header().source_addr().to_string();
            dst = ip.header().destination_addr().to_string();
            protocol = "IPv6".into();
        }
        // etherparse decodes ARP since 0.15; before that it reached `None` below and was named
        // from its EtherType. The row stays the same: the Ethernet addresses, named ARP.
        Some(LaxNetSlice::Arp(_)) => protocol = "ARP".into(),
        None => {
            if let Some(name) = link_ether_type.and_then(ether_type_name) {
                protocol = name.into();
            } else if let Some(t) = link_ether_type {
                protocol = format!("ethertype 0x{:04x}", t.0);
            }
        }
    }

    let mut detail = String::new();
    match &parsed.transport {
        Some(TransportSlice::Tcp(tcp)) => {
            protocol = "TCP".into();
            detail = format!(
                ":{} -> :{} [{}] seq={} ack={} win={}",
                tcp.source_port(),
                tcp.destination_port(),
                tcp_flags(tcp.syn(), tcp.ack(), tcp.fin(), tcp.rst(), tcp.psh(), tcp.urg()),
                tcp.sequence_number(),
                tcp.acknowledgment_number(),
                tcp.window_size()
            );
        }
        Some(TransportSlice::Udp(udp)) => {
            protocol = "UDP".into();
            detail = format!(":{} -> :{} len={}", udp.source_port(), udp.destination_port(), udp.length());
        }
        Some(TransportSlice::Icmpv4(_)) => protocol = "ICMPv4".into(),
        Some(TransportSlice::Icmpv6(_)) => protocol = "ICMPv6".into(),
        Some(TransportSlice::Igmp(_)) => protocol = "IGMP".into(),
        None => {}
    }

    let summary = if detail.is_empty() {
        format!("{protocol} {src} -> {dst}, {wire_len} bytes")
    } else {
        format!("{protocol} {src}{detail}, {wire_len} bytes")
    };
    Summary { src, dst, protocol, summary }
}

fn ether_type_name(t: EtherType) -> Option<&'static str> {
    match t.0 {
        0x0806 => Some("ARP"),
        0x8100 | 0x88a8 => Some("VLAN"),
        0x88cc => Some("LLDP"),
        0x888e => Some("EAPOL"),
        0x8847 | 0x8848 => Some("MPLS"),
        _ => None,
    }
}

fn tcp_flags(syn: bool, ack: bool, fin: bool, rst: bool, psh: bool, urg: bool) -> String {
    let names: Vec<&str> = [
        (syn, "SYN"),
        (ack, "ACK"),
        (fin, "FIN"),
        (rst, "RST"),
        (psh, "PSH"),
        (urg, "URG"),
    ]
    .into_iter()
    .filter_map(|(set, name)| set.then_some(name))
    .collect();
    if names.is_empty() { "none".into() } else { names.join(", ") }
}

fn mac(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect::<Vec<_>>().join(":")
}

/// `HH.uuuuuu`-style time since the first packet of the capture, the default of Wireshark's "Time"
/// column. Microseconds, because that is the resolution the capture file records.
pub fn relative_time(first_us: u64, now_us: u64) -> String {
    let d = now_us.saturating_sub(first_us);
    format!("{}.{:06}", d / 1_000_000, d % 1_000_000)
}

/// Wireshark's hex view: offset, two groups of eight bytes, then the printable characters.
/// At most `limit` bytes are shown; a final line says how many more the capture file holds.
pub fn hex_dump(data: &[u8], limit: usize) -> String {
    let shown = &data[..data.len().min(limit)];
    let mut out = String::with_capacity(shown.len() * 4 + 64);
    for (line, chunk) in shown.chunks(16).enumerate() {
        out.push_str(&format!("{:04x}  ", line * 16));
        for i in 0..16 {
            match chunk.get(i) {
                Some(b) => out.push_str(&format!("{b:02x} ")),
                None => out.push_str("   "),
            }
            if i == 7 {
                out.push(' ');
            }
        }
        out.push(' ');
        for b in chunk {
            out.push(if (0x20..0x7f).contains(b) { char::from(*b) } else { '.' });
        }
        out.push('\n');
    }
    if data.len() > shown.len() {
        out.push_str(&format!("... {} more bytes are in the capture file\n", data.len() - shown.len()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ipv4_header(total_len: u16, protocol: u8) -> Vec<u8> {
        let mut h = vec![0x45, 0x00];
        h.extend_from_slice(&total_len.to_be_bytes());
        h.extend_from_slice(&[0, 0, 0, 0, 64, protocol, 0, 0]);
        h.extend_from_slice(&[10, 0, 0, 1]);
        h.extend_from_slice(&[10, 0, 0, 2]);
        h
    }

    fn udp_ip_packet() -> Vec<u8> {
        let mut p = ipv4_header(20 + 8, 17);
        p.extend_from_slice(&1234u16.to_be_bytes());
        p.extend_from_slice(&53u16.to_be_bytes());
        p.extend_from_slice(&8u16.to_be_bytes());
        p.extend_from_slice(&[0, 0]);
        p
    }

    fn tcp_ip_packet(flags: u8) -> Vec<u8> {
        let mut p = ipv4_header(20 + 20, 6);
        p.extend_from_slice(&40000u16.to_be_bytes());
        p.extend_from_slice(&443u16.to_be_bytes());
        p.extend_from_slice(&7u32.to_be_bytes()); // seq
        p.extend_from_slice(&9u32.to_be_bytes()); // ack
        p.extend_from_slice(&[0x50, flags]); // data offset 5 words, flags
        p.extend_from_slice(&1000u16.to_be_bytes()); // window
        p.extend_from_slice(&[0, 0, 0, 0]); // checksum, urgent
        p
    }

    fn ethernet(ether_type: u16, payload: &[u8]) -> Vec<u8> {
        let mut f = vec![0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66];
        f.extend_from_slice(&ether_type.to_be_bytes());
        f.extend_from_slice(payload);
        f
    }

    #[test]
    fn a_udp_frame_yields_addresses_ports_and_length() {
        let frame = ethernet(0x0800, &udp_ip_packet());
        let s = summarize(LinkKind::Ethernet, &frame, frame.len() as u32);
        assert_eq!((s.src.as_str(), s.dst.as_str(), s.protocol.as_str()), ("10.0.0.1", "10.0.0.2", "UDP"));
        assert!(s.summary.contains(":1234 -> :53"), "{}", s.summary);
    }

    #[test]
    fn a_tcp_frame_names_its_flags_and_numbers() {
        let frame = ethernet(0x0800, &tcp_ip_packet(0b0001_0010)); // SYN + ACK
        let s = summarize(LinkKind::Ethernet, &frame, frame.len() as u32);
        assert_eq!(s.protocol, "TCP");
        assert!(s.summary.contains("[SYN, ACK]"), "{}", s.summary);
        assert!(s.summary.contains("seq=7 ack=9 win=1000"), "{}", s.summary);
    }

    #[test]
    fn arp_and_unknown_ether_types_are_named_not_hidden() {
        let arp = ethernet(0x0806, &[0u8; 28]);
        let s = summarize(LinkKind::Ethernet, &arp, arp.len() as u32);
        assert_eq!(s.protocol, "ARP");
        assert_eq!(s.src, "11:22:33:44:55:66");
        let odd = ethernet(0x1234, &[0u8; 10]);
        assert_eq!(summarize(LinkKind::Ethernet, &odd, 24).protocol, "ethertype 0x1234");
    }

    #[test]
    fn a_decoded_arp_request_keeps_the_ethernet_addresses_and_is_named_arp() {
        let mut arp = vec![0, 1, 0x08, 0x00, 6, 4, 0, 1]; // Ethernet, IPv4, request
        arp.extend_from_slice(&[0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 10, 0, 0, 1]);
        arp.extend_from_slice(&[0, 0, 0, 0, 0, 0, 10, 0, 0, 2]);
        let frame = ethernet(0x0806, &arp);
        let s = summarize(LinkKind::Ethernet, &frame, frame.len() as u32);
        assert_eq!((s.src.as_str(), s.dst.as_str(), s.protocol.as_str()), ("11:22:33:44:55:66", "aa:bb:cc:dd:ee:ff", "ARP"));
    }

    #[test]
    fn an_igmp_packet_is_named_igmp_with_its_ip_addresses() {
        let mut p = ipv4_header(20 + 8, 2);
        p.extend_from_slice(&[0x11, 100, 0, 0, 0, 0, 0, 0]); // membership query
        let frame = ethernet(0x0800, &p);
        let s = summarize(LinkKind::Ethernet, &frame, frame.len() as u32);
        assert_eq!((s.src.as_str(), s.dst.as_str(), s.protocol.as_str()), ("10.0.0.1", "10.0.0.2", "IGMP"));
    }

    #[test]
    fn an_offloaded_frame_with_ip_length_zero_is_still_decoded() {
        // A NIC doing TCP segmentation offload hands libpcap one big frame whose IPv4 total length
        // field is 0. The strict parser refuses it; the table must still say what it is.
        let mut ip = tcp_ip_packet(0b0001_1000); // PSH + ACK
        ip[2] = 0;
        ip[3] = 0;
        ip.extend_from_slice(&[0x41; 3000]);
        let frame = ethernet(0x0800, &ip);
        let s = summarize(LinkKind::Ethernet, &frame, frame.len() as u32);
        assert_eq!(s.protocol, "TCP", "{s:?}");
    }

    #[test]
    fn loopback_and_raw_framings_decode_the_ip_packet() {
        let mut null = vec![2, 0, 0, 0]; // AF_INET, host order
        null.extend_from_slice(&udp_ip_packet());
        assert_eq!(summarize(LinkKind::Null, &null, 32).protocol, "UDP");
        assert_eq!(summarize(LinkKind::Raw, &udp_ip_packet(), 28).protocol, "UDP");
    }

    #[test]
    fn cooked_captures_decode_both_header_versions() {
        let mut sll = vec![0, 0, 0, 1, 0, 6, 1, 2, 3, 4, 5, 6, 0, 0, 0x08, 0x00]; // v1, 16 bytes
        sll.extend_from_slice(&udp_ip_packet());
        let s = summarize(LinkKind::LinuxSll, &sll, sll.len() as u32);
        assert_eq!((s.protocol.as_str(), s.dst.as_str()), ("UDP", "10.0.0.2"));
        let mut sll2 = vec![0x08, 0x00, 0, 0, 0, 0, 0, 1, 0, 1, 0, 6, 1, 2, 3, 4, 5, 6, 0, 0]; // v2, 20 bytes
        sll2.extend_from_slice(&udp_ip_packet());
        assert_eq!(summarize(LinkKind::LinuxSll2, &sll2, sll2.len() as u32).protocol, "UDP");
    }

    #[test]
    fn garbage_and_unknown_link_types_fall_back_instead_of_panicking() {
        assert_eq!(summarize(LinkKind::Ethernet, &[0u8, 1, 2], 3).protocol, "unknown");
        assert_eq!(summarize(LinkKind::Null, &[0u8, 1], 2).protocol, "unknown");
        let s = summarize(LinkKind::Other(147), &[0u8; 20], 20);
        assert_eq!(s.protocol, "linktype 147");
        assert!(s.summary.contains("not decoded"));
        for len in 0..40 {
            let _ = summarize(LinkKind::Ethernet, &vec![0xff; len], len as u32);
            let _ = summarize(LinkKind::LinuxSll, &vec![0xff; len], len as u32);
        }
    }

    #[test]
    fn the_hex_dump_matches_the_wireshark_layout() {
        let data: Vec<u8> = (0u8..20).collect();
        assert_eq!(
            hex_dump(&data, 512),
            "0000  00 01 02 03 04 05 06 07  08 09 0a 0b 0c 0d 0e 0f  ................\n\
             0010  10 11 12 13                                       ....\n"
        );
        let text = hex_dump(b"GET / HTTP/1.1", 512);
        assert!(text.ends_with("GET / HTTP/1.1\n"), "{text:?}");
    }

    #[test]
    fn a_long_frame_is_cut_with_a_note_that_says_how_much_is_missing() {
        let data = vec![0xabu8; 700];
        let text = hex_dump(&data, HEX_BYTES_SENT);
        assert!(text.ends_with("... 188 more bytes are in the capture file\n"), "{text:?}");
        assert_eq!(text.lines().count(), 512 / 16 + 1);
    }

    #[test]
    fn the_time_column_counts_from_the_first_packet() {
        assert_eq!(relative_time(5_000_000, 5_000_000), "0.000000");
        assert_eq!(relative_time(5_000_000, 6_250_001), "1.250001");
        assert_eq!(relative_time(9, 3), "0.000000", "a clock step backwards never underflows");
    }
}
