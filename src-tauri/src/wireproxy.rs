//! Client-side wire helpers for the relay path: an in-place DNS answer
//! rewrite and a TLS ClientHello fragmentation planner.
//!
//! std-only on purpose — the unit tests below run under plain
//! `rustc --edition 2021 --test wireproxy.rs` without the Tauri/GTK
//! dependency tree that `cargo check` needs on this machine.

/// Skip one (possibly compressed) DNS name; returns the index just past it.
/// A pointer ends the walk here — both callers only need the end of the
/// name as *written*, never the name it points at.
fn skip_name(msg: &[u8], mut i: usize) -> Option<usize> {
    loop {
        let len = *msg.get(i)?;
        if len == 0 {
            return Some(i + 1);
        }
        if len & 0xC0 == 0xC0 {
            msg.get(i + 1)?;
            return Some(i + 2);
        }
        i = i + 1 + (len as usize & 0x3F);
    }
}

/// Rewrite every A-record RDATA equal to `from` into `to`, in place.
/// Returns how many records changed.
///
/// Only type-A RDATA in the answer/authority/additional sections is
/// touched: a question, a label, a CNAME target, or an A record holding a
/// different address stays byte-identical. The message never changes
/// length, so no offset anywhere in the packet can move — that is what
/// makes an in-place 4-byte swap safe next to compression pointers.
pub fn rewrite_relay_a(msg: &mut [u8], from: [u8; 4], to: [u8; 4]) -> usize {
    if msg.len() < 12 {
        return 0;
    }
    let qd = u16::from_be_bytes([msg[4], msg[5]]) as usize;
    let sections = u16::from_be_bytes([msg[6], msg[7]]) as usize
        + u16::from_be_bytes([msg[8], msg[9]]) as usize
        + u16::from_be_bytes([msg[10], msg[11]]) as usize;
    let mut i = 12usize;
    let mut changed = 0usize;
    for _ in 0..qd {
        match skip_name(msg, i) {
            Some(x) if x + 4 <= msg.len() => i = x + 4,
            _ => return changed,
        }
    }
    for _ in 0..sections {
        i = match skip_name(msg, i) {
            Some(x) => x,
            None => return changed,
        };
        if i + 10 > msg.len() {
            return changed;
        }
        let typ = u16::from_be_bytes([msg[i], msg[i + 1]]);
        let rdlen = u16::from_be_bytes([msg[i + 8], msg[i + 9]]) as usize;
        i += 10;
        if i + rdlen > msg.len() {
            return changed;
        }
        if typ == 1 && rdlen == 4 && [msg[i], msg[i + 1], msg[i + 2], msg[i + 3]] == from {
            msg[i..i + 4].copy_from_slice(&to);
            changed += 1;
        }
        i += rdlen;
    }
    changed
}

/// Payload length from a TLS record's 5-byte header, or `None` when these
/// bytes are not a record worth fragmenting: wrong content type, not a
/// version-3 record layer, or a length outside what RFC 8446 allows
/// (2^14 plaintext + 2048 overhead). The minor byte is deliberately
/// unchecked — ClientHello's legacy_record_version is 0x0301 on some
/// stacks and 0x0303 on others, both perfectly valid.
pub fn tls_record_payload(header: &[u8]) -> Option<usize> {
    if header.len() < 5 {
        return None;
    }
    if header[0] != 0x16 {
        return None; // not a handshake — the record carries no SNI
    }
    if header[1] != 0x03 {
        return None;
    }
    let len = u16::from_be_bytes([header[3], header[4]]) as usize;
    if len == 0 || len > 16384 + 2048 {
        return None;
    }
    Some(len)
}

/// Chunk sizes for sending a `payload`-byte TLS body one piece at a time
/// after its header: the writer's schedule under TCP_NODELAY. The kernel
/// reassembles; the relay's ssl_preread sees the same bytes it always did,
/// just in more segments than a DPI box that only reassembles in order
/// within a segment expects to see.
pub fn fragment_sizes(payload: usize, chunk: usize) -> Vec<usize> {
    if payload == 0 || chunk == 0 {
        return Vec::new();
    }
    let mut sizes = vec![chunk; payload / chunk];
    let rem = payload % chunk;
    if rem > 0 {
        sizes.push(rem);
    }
    sizes
}

/// True for 127.0.0.0/8 — an answer our own rewrite just produced.
/// `resolve_relay_ip` must never adopt it as the relay's address: that
/// would point the whole proxy back at itself.
pub fn is_loopback_v4(ip: [u8; 4]) -> bool {
    ip[0] == 127
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELAY: [u8; 4] = [92, 42, 207, 101];
    const LOCAL: [u8; 4] = [127, 0, 0, 1];

    /// One question + one answer whose RDATA is `rdata`, `typ` typed.
    fn message(typ: u16, rdata: &[u8]) -> Vec<u8> {
        let mut m = vec![
            0x12, 0x34, // tid
            0x81, 0x80, // response, rd, rcode=0
            0x00, 0x01, // qd
            0x00, 0x01, // an
            0x00, 0x00, // ns
            0x00, 0x00, // ar
        ];
        // question: example.com A IN
        m.extend_from_slice(b"\x07example\x03com\x00\x00\x01\x00\x01");
        // answer: compressed name -> question, ttl 60, rdlen, rdata
        m.extend_from_slice(&[0xC0, 0x0C]);
        m.extend_from_slice(&typ.to_be_bytes());
        m.extend_from_slice(&[0x00, 0x01]); // class IN
        m.extend_from_slice(&[0, 0, 0, 60]); // ttl
        m.extend_from_slice(&(rdata.len() as u16).to_be_bytes());
        m.extend_from_slice(rdata);
        m
    }

    #[test]
    fn rewrites_the_answer_a_record() {
        let mut m = message(1, &RELAY);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 1);
        assert_eq!(&m[m.len() - 4..], &LOCAL);
    }

    #[test]
    fn leaves_other_addresses_alone() {
        let mut m = message(1, &[8, 8, 8, 8]);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 0);
        assert_eq!(&m[m.len() - 4..], &[8, 8, 8, 8]);
    }

    #[test]
    fn touches_only_type_a() {
        // CNAME whose RDATA happens to equal the relay bytes must not move.
        let mut m = message(5, &RELAY);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 0);
        assert_eq!(&m[m.len() - 4..], &RELAY);
    }

    #[test]
    fn question_name_bytes_are_not_rdata() {
        // A label made of the relay's four bytes: the walker must not treat
        // it as an answer. Build the question by hand to get them in.
        let mut m = vec![0x00, 0x01, 0x81, 0x80, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00];
        m.extend_from_slice(&[4]); // label length
        m.extend_from_slice(&RELAY); // the four bytes, as a label
        m.extend_from_slice(&[0, 0x00, 0x01, 0x00, 0x01]);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 0);
        assert_eq!(&m[13..17], &RELAY);
    }

    #[test]
    fn truncated_message_does_not_panic() {
        // Cut inside the answer's fixed fields — a mangled UDP reply must
        // cost nothing worse than a no-op.
        let full = message(1, &RELAY);
        for cut in [0, 11, 12, 30, full.len() - 1, full.len()] {
            let mut m = full[..cut].to_vec();
            let _ = rewrite_relay_a(&mut m, RELAY, LOCAL);
        }
        // …and the cut that lands exactly on the RDATA still rewrites it.
        let mut m = full[..full.len() - 4].to_vec();
        m.extend_from_slice(&RELAY);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 1);
    }

    #[test]
    fn a_record_in_additional_section_is_rewritten() {
        let mut m = vec![0x00, 0x01, 0x81, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01];
        m.extend_from_slice(&[0xC0, 0x0C]); // name -> question (none here, still legal)
        m.extend_from_slice(&[0x00, 0x01, 0x00, 0x01, 0, 0, 0, 60, 0x00, 0x04]);
        m.extend_from_slice(&RELAY);
        assert_eq!(rewrite_relay_a(&mut m, RELAY, LOCAL), 1);
        assert_eq!(&m[m.len() - 4..], &LOCAL);
    }

    #[test]
    fn fragment_sizes_split_with_and_without_remainder() {
        assert_eq!(fragment_sizes(100, 16), vec![16, 16, 16, 16, 16, 16, 4]);
        assert_eq!(fragment_sizes(32, 16), vec![16, 16]);
        assert_eq!(fragment_sizes(5, 16), vec![5]);
        assert_eq!(fragment_sizes(0, 16), Vec::<usize>::new());
        assert_eq!(fragment_sizes(100, 0), Vec::<usize>::new());
        // The pieces must tile the payload exactly — a lost byte desyncs
        // the TLS stream at the relay.
        let sizes = fragment_sizes(777, 16);
        assert_eq!(sizes.iter().sum::<usize>(), 777);
        assert!(sizes.iter().all(|&s| s <= 16));
    }

    #[test]
    fn tls_header_recognises_a_real_client_hello() {
        // handshake, 0x0301 legacy version, 128-byte body
        assert_eq!(tls_record_payload(&[0x16, 0x03, 0x01, 0x00, 0x80]), Some(128));
        assert_eq!(tls_record_payload(&[0x16, 0x03, 0x03, 0x01, 0x00]), Some(256));
    }

    #[test]
    fn tls_header_rejects_everything_else() {
        assert_eq!(tls_record_payload(&[0x17, 0x03, 0x03, 0x00, 0x10]), None); // app data
        assert_eq!(tls_record_payload(&[0x16, 0x04, 0x03, 0x00, 0x10]), None); // not v3
        assert_eq!(tls_record_payload(&[0x16, 0x03, 0x03, 0x00, 0x00]), None); // empty
        assert_eq!(tls_record_payload(&[0x16, 0x03, 0x03, 0x50, 0x01]), None); // > 2^14+2048
        assert_eq!(tls_record_payload(&[0x16, 0x03]), None); // short header
        assert_eq!(tls_record_payload(&[]), None);
    }

    #[test]
    fn loopback_gate() {
        assert!(is_loopback_v4([127, 0, 0, 1]));
        assert!(is_loopback_v4([127, 1, 2, 3]));
        assert!(!is_loopback_v4(RELAY));
        assert!(!is_loopback_v4([0, 0, 0, 0]));
        assert!(!is_loopback_v4([8, 8, 8, 8]));
    }
}
