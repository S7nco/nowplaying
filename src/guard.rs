// Checks every request before it reaches the server's routes:
// - Host: only localhost or an IP address (blocks "DNS rebinding", where a website pretends to be this PC)
// - Origin: /ws and changes only from the app's own pages (other websites can't listen in or change things)
// - Changes (anything but GET/HEAD) only from this PC, even when bind_address lets other devices connect

use axum::extract::{ConnectInfo, Request};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};
use std::net::{IpAddr, SocketAddr};

pub async fn guard(
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let origin = req
        .headers()
        .get(header::ORIGIN)
        .map(|v| v.to_str().unwrap_or("").to_string());
    let changes = !matches!(*req.method(), Method::GET | Method::HEAD);
    let websocket = req.uri().path() == "/ws";

    if !host_allowed(&host) {
        return refuse("Unknown host");
    }
    // Browsers always send Origin for WebSockets and changes. It must be this server's own address.
    if changes || websocket {
        if let Some(origin) = origin {
            if origin != format!("http://{host}") {
                return refuse("Not allowed from other websites");
            }
        }
    }
    if changes && !peer.ip().is_loopback() {
        return refuse("Changes can only be made on the PC running Now Playing");
    }
    let mut response = next.run(req).await;
    if let Ok(policy) = HeaderValue::from_str(&content_policy(&host)) {
        response
            .headers_mut()
            .insert(header::CONTENT_SECURITY_POLICY, policy);
    }
    response
}

/// What pages may load: only this app's own files, so a theme can't contact other websites.
/// Other ports on this host are allowed because the Restart button may have to find a new port.
fn content_policy(host: &str) -> String {
    let name = host_name(host);
    // CSP can't write IPv6 addresses like ::1, so those only get 'self'
    let connect = if name.contains(':') {
        "'self'".to_string()
    } else {
        format!("'self' http://{name}:* ws://{name}:*")
    };
    format!(
        "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'self' data:; \
        font-src 'self' data:; connect-src {connect}; frame-src 'self'; frame-ancestors 'self'; \
        base-uri 'none'; form-action 'none'"
    )
}

fn refuse(why: &'static str) -> Response {
    (StatusCode::FORBIDDEN, why).into_response()
}

/// "name:5000" -> "name", "[::1]:5000" -> "::1". Also accepts a full address like "http://name:5000/".
fn host_name(host: &str) -> &str {
    let host = host
        .trim()
        .trim_start_matches("http://")
        .trim_start_matches("https://")
        .trim_end_matches('/');
    match host.strip_prefix('[') {
        Some(rest) => rest.split(']').next().unwrap_or(""), // [::1]:5000
        None => host.rsplit_once(':').map_or(host, |(name, _)| name), // name:5000
    }
}

/// "localhost:5000", "127.0.0.1:5000", "192.168.1.20:5000" and "[::1]:5000" are fine;
/// this PC's own name (e.g. "DESKTOP-ABC:5000" or "desktop-abc.local:5000"), plus any names
/// listed in allowed_hosts. Other names like "evil.com", aren't
fn host_allowed(host: &str) -> bool {
    let name = host_name(host);
    if name.is_empty() {
        return false;
    }
    if name.eq_ignore_ascii_case("localhost") || name.parse::<IpAddr>().is_ok() {
        return true;
    }
    // This PC's own name, for OBS on a second PC. A website can't use it as its address,
    // so allowing it doesn't weaken the DNS rebinding protection.
    if let Ok(pc) = std::env::var("COMPUTERNAME") {
        if name.eq_ignore_ascii_case(&pc) || name.eq_ignore_ascii_case(&format!("{pc}.local")) {
            return true;
        }
    }
    in_allowed_hosts(name, &crate::config::get().allowed_hosts)
}

/// Is this name one of the user's allowed_hosts? Entries may include a port or a full address
fn in_allowed_hosts(name: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|entry| {
        let entry = host_name(entry);
        !entry.is_empty() && name.eq_ignore_ascii_case(entry)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_name_strips_port_and_address_parts() {
        assert_eq!(host_name("localhost:5000"), "localhost");
        assert_eq!(host_name("127.0.0.1:5000"), "127.0.0.1");
        assert_eq!(host_name("[::1]:5000"), "::1");
        assert_eq!(host_name("http://desktop-abc.lan:5000/"), "desktop-abc.lan");
        assert_eq!(host_name("  desktop-abc.lan  "), "desktop-abc.lan");
        assert_eq!(host_name("desktop-abc.lan"), "desktop-abc.lan");
    }

    #[test]
    fn this_pc_and_ip_addresses_are_allowed() {
        for host in [
            "localhost:5000",
            "LOCALHOST:5000",
            "127.0.0.1:5000",
            "[::1]:5000",
            "192.168.1.20:5000",
        ] {
            assert!(host_allowed(host), "{host} should be allowed");
        }
        if let Ok(pc) = std::env::var("COMPUTERNAME") {
            assert!(host_allowed(&format!("{pc}:5000")));
            assert!(host_allowed(&format!("{}.local:5000", pc.to_lowercase())));
        }
    }

    #[test]
    fn other_names_are_refused() {
        for host in [
            "",
            "evil.com",
            "evil.com:5000",
            "localhost.evil.com",
            "localhost@evil.com",
            "127.0.0.1.evil.com",
        ] {
            assert!(!host_allowed(host), "{host:?} should be refused");
        }
    }

    // 0.2.4 claimed entries with a port worked, but they were refused (fixed in 0.3.0)
    #[test]
    fn allowed_hosts_entries_may_include_a_port_or_full_address() {
        let list = vec![
            "desktop-abc.lan:5000".to_string(),
            "http://other.lan:5000/".to_string(),
            "plain.lan".to_string(),
            "".to_string(),
        ];
        assert!(in_allowed_hosts("desktop-abc.lan", &list));
        assert!(in_allowed_hosts("other.lan", &list));
        assert!(in_allowed_hosts("PLAIN.lan", &list));
        assert!(!in_allowed_hosts("", &list));
        assert!(!in_allowed_hosts("evil.com", &list));
    }

    // A typo in this text once blocked every stylesheet and request (caught before release)
    #[test]
    fn content_policy_allows_the_apps_own_files() {
        let p = content_policy("localhost:5000");
        for part in [
            "default-src 'none';",
            "script-src 'self';",
            "style-src 'self';",
            "img-src 'self' data:;",
            "font-src 'self' data:;",
            "connect-src 'self' http://localhost:* ws://localhost:*;",
            "frame-src 'self';",
            "frame-ancestors 'self';",
        ] {
            assert!(p.contains(part), "missing {part:?} in {p}");
        }
        assert!(content_policy("[::1]:5000").contains("connect-src 'self';"));
        assert!(HeaderValue::from_str(&p).is_ok());
    }
}
