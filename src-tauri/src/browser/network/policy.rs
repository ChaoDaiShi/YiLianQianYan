use super::{Context, CONNECT_TIMEOUT};
use std::{
    collections::HashSet,
    net::{IpAddr, SocketAddr},
};
use tauri::Url;
use tokio::{
    net::{lookup_host, TcpStream},
    time::timeout,
};
#[derive(Default)]
pub(super) struct Policy {
    pub(super) local: HashSet<String>,
    pub(super) forbidden_ports: HashSet<u16>,
    pub(super) revision: u64,
}

pub(super) fn checked_url(raw: &str) -> Result<Url, String> {
    if raw.len() > 8192
        || raw.is_empty()
        || raw.chars().any(|c| c.is_control() || c.is_whitespace())
    {
        return Err("Unsafe browser address".into());
    }
    let mut url = Url::parse(raw).map_err(|_| "Invalid browser address")?;
    match url.scheme() {
        "ws" => {
            let _ = url.set_scheme("http");
        }
        "wss" => {
            let _ = url.set_scheme("https");
        }
        "http" | "https" => {}
        _ => return Err("Only HTTP(S) browser destinations are allowed".into()),
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.host_str().is_none()
        || url.port_or_known_default() == Some(0)
    {
        return Err("Browser address credentials or host are invalid".into());
    }
    Ok(url)
}

pub(super) fn explicit_local_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("localhost")
        || host.trim_matches(['[', ']']).parse::<IpAddr>().is_ok()
}

fn local_or_reserved_host(host: &str) -> bool {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "tauri.localhost"
        || host == "tauri"
        || host == "ipc.localhost"
        || host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .map(|ip| !public_ip(ip))
            .unwrap_or(false)
}

pub(super) fn permitted(policy: &Policy, url: &Url) -> bool {
    let port = url.port_or_known_default().unwrap_or(0);
    if policy.forbidden_ports.contains(&port) {
        return false;
    }
    let host = url.host_str().unwrap_or_default();
    let host_lower = host.trim_end_matches('.').to_ascii_lowercase();
    if ["tauri.localhost", "ipc.localhost", "asset.localhost"]
        .iter()
        .any(|name| host_lower == *name || host_lower.ends_with(&format!(".{name}")))
    {
        return false;
    }
    !local_or_reserved_host(host) || policy.local.contains(&url.origin().ascii_serialization())
}

pub(super) fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => {
            let [a, b, c, _] = ip.octets();
            !(a == 0
                || a == 10
                || a == 127
                || a >= 224
                || (a == 100 && (64..=127).contains(&b))
                || (a == 169 && b == 254)
                || (a == 172 && (16..=31).contains(&b))
                || (a == 192 && (b == 168 || b == 0 || (b == 88 && c == 99)))
                || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
                || (a == 203 && b == 0 && c == 113))
        }
        IpAddr::V6(ip) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return public_ip(IpAddr::V4(v4));
            }
            let s = ip.segments();
            // Global unicast only; exclude documentation, transition, Teredo,
            // 6to4 and protocol-assignment space rather than relay private IPs.
            (s[0] & 0xe000) == 0x2000
                && s[0] != 0x2002
                && !(s[0] == 0x2001 && (s[1] <= 0x01ff || s[1] == 0x0db8))
        }
    }
}

pub(super) async fn destination(url: &Url, context: &Context) -> Result<TcpStream, String> {
    if *context.stop.borrow() {
        return Err("Browser session closed".into());
    }
    if !context
        .policy
        .read()
        .map(|p| permitted(&p, url))
        .unwrap_or(false)
    {
        return Err("Browser destination blocked".into());
    }
    let host = url.host_str().unwrap_or_default().trim_matches(['[', ']']);
    let port = url.port_or_known_default().unwrap_or(0);
    let addresses: Vec<SocketAddr> = timeout(CONNECT_TIMEOUT, lookup_host((host, port)))
        .await
        .map_err(|_| "Browser DNS lookup timed out")?
        .map_err(|_| "Browser DNS lookup failed")?
        .take(16)
        .collect();
    if addresses.is_empty() {
        return Err("Browser destination has no addresses".into());
    }
    let local_allowed = explicit_local_host(host)
        && context
            .policy
            .read()
            .map(|p| p.local.contains(&url.origin().ascii_serialization()))
            .unwrap_or(false);
    if addresses.iter().any(|address| {
        !public_ip(address.ip())
            && !(local_allowed
                && (!host.eq_ignore_ascii_case("localhost") || address.ip().is_loopback()))
    }) {
        return Err("Private browser destination is not approved".into());
    }
    timeout(CONNECT_TIMEOUT, async {
        for address in addresses {
            if !context
                .policy
                .read()
                .map(|p| permitted(&p, url))
                .unwrap_or(false)
            {
                return Err("Browser destination authorization changed".into());
            }
            if let Ok(stream) = TcpStream::connect(address).await {
                if *context.stop.borrow()
                    || !context
                        .policy
                        .read()
                        .map(|p| permitted(&p, url))
                        .unwrap_or(false)
                {
                    return Err("Browser destination authorization changed".into());
                }
                return Ok(stream);
            }
        }
        Err("Browser destination unavailable".into())
    })
    .await
    .map_err(|_| "Browser connection timed out".to_string())?
}
