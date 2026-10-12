//! Which URLs a provider may point us at.
//!
//! A hosted provider hands back a link; the link is untrusted input. We follow it only when it is
//! https (plain http only for a loopback host the caller has allowed), carries no user-info, and
//! does not name a private, link-local or otherwise internal address. Names are not resolved
//! here, so a hostname that later resolves to an internal address is not caught; the transfer
//! still sends no credentials and only keeps bytes that look like real media.

use craft_ai_model::{AiError, ErrorKind};
use std::net::{Ipv4Addr, Ipv6Addr};
use url::{Host, Url};

fn unsafe_link(why: &str) -> AiError {
    AiError::new(ErrorKind::Provider, format!("media link refused: {why}"))
}

/// Checks one URL. `allow_loopback` permits `http` and `https` to a loopback host.
pub fn check(url: &Url, allow_loopback: bool) -> Result<(), AiError> {
    if !url.username().is_empty() || url.password().is_some() {
        return Err(unsafe_link("it carries credentials"));
    }
    let loopback = is_loopback(url);
    match url.scheme() {
        "https" => {}
        "http" if allow_loopback && loopback => {}
        "http" => return Err(unsafe_link("it is not https")),
        _ => return Err(unsafe_link("it is not an http link")),
    }
    if loopback {
        return if allow_loopback {
            Ok(())
        } else {
            Err(unsafe_link("it points at this computer"))
        };
    }
    match url.host() {
        Some(Host::Ipv4(ip)) if internal_v4(ip) => {
            Err(unsafe_link("it points at a private address"))
        }
        Some(Host::Ipv6(ip)) if internal_v6(ip) => {
            Err(unsafe_link("it points at a private address"))
        }
        Some(Host::Domain(name)) if name.ends_with(".local") || name.ends_with(".internal") => {
            Err(unsafe_link("it points at a private name"))
        }
        Some(_) => Ok(()),
        None => Err(unsafe_link("it has no host")),
    }
}

/// Resolves a `Location` header against the current link. A redirect may not leave https; the
/// caller still runs [`check`] on the result before connecting.
pub fn redirect_target(current: &Url, location: &str) -> Result<Url, AiError> {
    let next = current
        .join(location)
        .map_err(|_| AiError::new(ErrorKind::Provider, "media link redirects nowhere"))?;
    if current.scheme() == "https" && next.scheme() != "https" {
        return Err(unsafe_link("a redirect leaves https"));
    }
    Ok(next)
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain(name)) => name == "localhost" || name.ends_with(".localhost"),
        Some(Host::Ipv4(ip)) => ip.is_loopback(),
        Some(Host::Ipv6(ip)) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
        None => false,
    }
}

fn internal_v4(ip: Ipv4Addr) -> bool {
    let [a, b, ..] = ip.octets();
    ip.is_private()
        || ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || ip.is_broadcast()
        || ip.is_documentation()
        || (a == 100 && (64..128).contains(&b))
        || a == 0
        || a >= 240
}

fn internal_v6(ip: Ipv6Addr) -> bool {
    if let Some(v4) = ip.to_ipv4_mapped() {
        return internal_v4(v4);
    }
    let first = ip.segments()[0];
    ip.is_loopback()
        || ip.is_unspecified()
        || (first & 0xfe00) == 0xfc00
        || (first & 0xffc0) == 0xfe80
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(url: &str, loopback: bool) -> bool {
        check(&Url::parse(url).unwrap(), loopback).is_ok()
    }

    #[test]
    fn ordinary_https_links_pass() {
        assert!(ok("https://cdn.example.com/a.png?sig=abc", false));
        assert!(ok("https://8.8.8.8/a.png", false));
        assert!(!ok("https://203.0.113.9/a.png", false)); // documentation range
    }

    #[test]
    fn plain_http_and_other_schemes_are_refused() {
        assert!(!ok("http://cdn.example.com/a.png", false));
        assert!(!ok("http://cdn.example.com/a.png", true));
        assert!(!ok("ftp://cdn.example.com/a.png", false));
        assert!(!ok("file:///etc/passwd", true));
    }

    #[test]
    fn a_redirect_resolves_relative_links_and_never_leaves_https() {
        let here = Url::parse("https://cdn.example.com/a/b.png").unwrap();
        assert_eq!(
            redirect_target(&here, "/c.png").unwrap().as_str(),
            "https://cdn.example.com/c.png"
        );
        assert!(redirect_target(&here, "http://cdn.example.com/c.png").is_err());
        assert!(redirect_target(&here, "http://127.0.0.1/c.png").is_err());
        assert!(redirect_target(&here, "").is_ok()); // same link; the hop limit ends any loop
        let plain = Url::parse("http://127.0.0.1/a").unwrap();
        assert!(redirect_target(&plain, "https://cdn.example.com/c.png").is_ok());
    }

    #[test]
    fn credentials_in_the_link_are_refused() {
        assert!(!ok("https://user:pw@cdn.example.com/a.png", false));
        assert!(!ok("https://user@cdn.example.com/a.png", false));
    }

    #[test]
    fn internal_addresses_are_refused() {
        for u in [
            "https://127.0.0.1/a",
            "https://localhost/a",
            "https://[::1]/a",
            "https://10.0.0.5/a",
            "https://192.168.1.2/a",
            "https://172.16.0.1/a",
            "https://169.254.169.254/latest/meta-data",
            "https://100.64.0.1/a",
            "https://0.0.0.0/a",
            "https://[fd00::1]/a",
            "https://[fe80::1]/a",
            "https://[::ffff:10.0.0.1]/a",
            "https://printer.local/a",
            "https://db.internal/a",
        ] {
            assert!(!ok(u, false), "{u}");
        }
    }

    #[test]
    fn loopback_is_only_allowed_when_the_caller_says_so() {
        assert!(ok("http://127.0.0.1:8080/a.png", true));
        assert!(ok("http://localhost:8080/a.png", true));
        assert!(!ok("http://127.0.0.1:8080/a.png", false));
        // Allowing loopback does not allow the rest of the private ranges.
        assert!(!ok("http://10.0.0.5/a.png", true));
        assert!(!ok("https://169.254.169.254/a", true));
    }
}
