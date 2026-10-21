//! Which peers may reach the gateway, and which addresses the Mac offers to a phone.
//! Shared with `agent-link-core`.

pub use agent_link_core::net::{host_name, is_allowed, is_local_range, local_addresses, Cidr};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_local_ranges_are_allowed() {
        for ok in [
            "127.0.0.1",
            "10.0.2.2",
            "10.255.1.1",
            "172.16.0.1",
            "172.31.255.254",
            "192.168.1.20",
            "169.254.10.10",
            "::1",
            "fe80::1",
            "fd12:3456::1",
            "::ffff:192.168.1.5",
        ] {
            assert!(
                is_allowed(ok.parse().unwrap(), &[]),
                "{ok} should be allowed"
            );
        }
        for no in [
            "8.8.8.8",
            "172.32.0.1",
            "172.15.0.1",
            "192.169.0.1",
            "100.64.0.1",
            "1.1.1.1",
            "2001:4860:4860::8888",
            "::ffff:8.8.8.8",
        ] {
            assert!(
                !is_allowed(no.parse().unwrap(), &[]),
                "{no} should be refused"
            );
        }
        let vpn = [Cidr::parse("100.64.0.0/10").unwrap()];
        assert!(is_allowed("100.64.0.1".parse().unwrap(), &vpn));
        assert!(is_allowed("100.127.255.254".parse().unwrap(), &vpn));
        assert!(!is_allowed("100.128.0.1".parse().unwrap(), &vpn));
        assert!(Cidr::parse("1.2.3.4/33").is_none());
        assert!(Cidr::parse("nope").is_none());
    }

    #[test]
    fn the_mac_offers_at_least_loopback() {
        let addrs = local_addresses();
        assert!(addrs.contains(&"127.0.0.1".to_string()));
        assert!(addrs.iter().all(|a| is_local_range(a.parse().unwrap())));
    }
}
