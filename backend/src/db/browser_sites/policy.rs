//! Shared URL syntax policy. Runtime network enforcement remains mandatory.
use serde::Serialize;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NormalizedSiteUrl {
    pub url: String,
    pub origin: String,
    pub port: u16,
}

/// Validate a saved entry URL without silently changing its meaning. Secret
/// query parameters cannot be saved as a bookmark; history strips all queries.
pub fn normalize_site_url(raw: &str) -> Result<NormalizedSiteUrl, String> {
    let parsed = parse_browser_url(raw)?;
    if parsed.query_pairs().any(|(key, _)| sensitive_key(&key))
        || parsed.fragment().is_some_and(|fragment| {
            url::form_urlencoded::parse(fragment.as_bytes()).any(|(key, _)| sensitive_key(&key))
        })
    {
        return Err("站点入口不能保存 token、密码或授权码参数；请使用不含凭据的入口地址".into());
    }
    Ok(NormalizedSiteUrl {
        origin: parsed.origin().ascii_serialization(),
        port: parsed.port_or_known_default().ok_or("网址缺少端口")?,
        url: parsed.to_string(),
    })
}

/// Page titles and visits are observations, never proof of service ownership.
/// Even valid navigation query strings may contain credentials, so none persist.
pub fn redacted_visit_url(raw: &str) -> Result<String, String> {
    let mut parsed = parse_browser_url(raw)?;
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok(parsed.to_string())
}

fn parse_browser_url(raw: &str) -> Result<url::Url, String> {
    if raw.is_empty() || raw.len() > 4096 || raw.chars().any(char::is_control) || raw.trim() != raw
    {
        return Err("网址不能为空、包含控制字符或超过4096字节".into());
    }
    let parsed = url::Url::parse(raw).map_err(|_| "网址格式无效".to_owned())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("只支持不含用户名和密码的HTTP或HTTPS网址".into());
    }
    let host = parsed
        .host_str()
        .unwrap_or_default()
        .trim_end_matches('.')
        .to_ascii_lowercase();
    if host == "tauri.localhost"
        || host.ends_with(".tauri.localhost")
        || host == "0.0.0.0"
        || host == "[::]"
    {
        return Err("不能访问助手内部来源或未指定的监听地址".into());
    }
    let ip = host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .parse::<std::net::IpAddr>()
        .ok();
    let local_ip = ip.is_some_and(|ip| match ip {
        std::net::IpAddr::V4(ip) => ip.is_loopback(),
        std::net::IpAddr::V6(ip) => {
            ip.is_loopback() || ip.to_ipv4_mapped().is_some_and(|ip| ip.is_loopback())
        }
    });
    let loopback = host == "localhost" || host.ends_with(".localhost") || local_ip;
    if parsed.port_or_known_default() == Some(0) {
        return Err("网址端口必须介于1和65535之间".into());
    }
    if loopback && matches!(parsed.port_or_known_default(), Some(1420 | 9420)) {
        return Err("该地址由助手控制界面或本地控制服务保留".into());
    }
    Ok(parsed)
}

fn sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase().replace(['-', '_'], "");
    matches!(
        key.as_str(),
        "token"
            | "accesstoken"
            | "refreshtoken"
            | "idtoken"
            | "apikey"
            | "password"
            | "passwd"
            | "secret"
            | "clientsecret"
            | "authorization"
            | "auth"
            | "code"
            | "session"
            | "sessionid"
            | "signature"
            | "sig"
            | "credential"
    )
}
