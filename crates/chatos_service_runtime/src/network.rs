use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => is_public_ipv4(ip),
        IpAddr::V6(ip) => is_public_ipv6(ip),
    }
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let octets = ip.octets();
    !(ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_broadcast()
        || ip.is_documentation()
        || ip.is_unspecified()
        || ip.is_multicast()
        || octets[0] == 0
        || (octets[0] == 100 && (64..=127).contains(&octets[1]))
        || (octets[0] == 198 && matches!(octets[1], 18 | 19))
        || octets[0] >= 240)
}

fn is_public_ipv6(ip: Ipv6Addr) -> bool {
    let segments = ip.segments();
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_public_ipv4(mapped);
    }
    !(ip.is_loopback()
        || ip.is_unspecified()
        || ip.is_multicast()
        || (segments[0] & 0xfe00) == 0xfc00
        || (segments[0] & 0xffc0) == 0xfe80
        || (segments[0] == 0x2001 && segments[1] == 0x0db8))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_ip_rejects_non_routable_ranges() {
        for value in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.1.1",
            "100.64.0.1",
            "198.18.0.1",
            "::1",
            "fc00::1",
            "fe80::1",
            "2001:db8::1",
            "::ffff:10.0.0.1",
        ] {
            assert!(!is_public_ip(value.parse().expect("valid IP")), "{value}");
        }
        assert!(is_public_ip("8.8.8.8".parse().expect("public IPv4")));
        assert!(is_public_ip(
            "2606:4700:4700::1111".parse().expect("public IPv6")
        ));
    }
}
