//! RFC 4648 base32 without padding, for the pairing code. Shared with `agent-link-core`.

pub use agent_link_core::base32::{decode, encode};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base32_round_trips_and_matches_rfc_vectors() {
        assert_eq!(encode(b""), "");
        assert_eq!(encode(b"f"), "MY");
        assert_eq!(encode(b"fo"), "MZXQ");
        assert_eq!(encode(b"foo"), "MZXW6");
        assert_eq!(encode(b"foob"), "MZXW6YQ");
        assert_eq!(encode(b"fooba"), "MZXW6YTB");
        assert_eq!(encode(b"foobar"), "MZXW6YTBOI");
        for n in 0..70usize {
            let bytes: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
        }
        assert_eq!(decode("mzxw-6ytb oi").unwrap(), b"foobar");
        assert!(decode("MZXW1").is_none());
    }
}
