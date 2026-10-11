use std::time::Duration;

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum CloudTransportError {
    #[error("invalid cloud service URL")]
    InvalidUrl,
    #[error("remote cloud service requires HTTPS")]
    HttpsRequired,
    #[error("cloud HTTP client unavailable")]
    ClientUnavailable,
}

pub fn normalize_cloud_base(raw: &str) -> Result<String, CloudTransportError> {
    let url = url::Url::parse(raw.trim()).map_err(|_| CloudTransportError::InvalidUrl)?;
    if url.host().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(CloudTransportError::InvalidUrl);
    }
    let loopback = match url.host() {
        Some(url::Host::Domain("localhost")) => true,
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        _ => false,
    };
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        "http" => return Err(CloudTransportError::HttpsRequired),
        _ => return Err(CloudTransportError::InvalidUrl),
    }
    Ok(url.as_str().trim_end_matches('/').to_owned())
}

pub fn build_cloud_http(
    raw: &str,
    timeout: Duration,
) -> Result<(reqwest::Client, String), CloudTransportError> {
    let base = normalize_cloud_base(raw)?;
    let http = reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| CloudTransportError::ClientUnavailable)?;
    Ok((http, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_valid_cases() {
        assert_eq!(
            normalize_cloud_base("https://cloud.example.com/api/v1").unwrap(),
            "https://cloud.example.com/api/v1"
        );
        assert_eq!(
            normalize_cloud_base("https://cloud.example.com/api/v1/").unwrap(),
            "https://cloud.example.com/api/v1"
        );
        assert_eq!(
            normalize_cloud_base("http://localhost:8080/api/v1").unwrap(),
            "http://localhost:8080/api/v1"
        );
        assert_eq!(
            normalize_cloud_base("http://127.0.0.1:8080").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            normalize_cloud_base("http://[::1]:8080/api/v1").unwrap(),
            "http://[::1]:8080/api/v1"
        );
    }

    #[test]
    fn test_normalize_https_required() {
        assert_eq!(
            normalize_cloud_base("http://cloud.example.com/api/v1").unwrap_err(),
            CloudTransportError::HttpsRequired
        );
        assert_eq!(
            normalize_cloud_base("http://192.168.1.10:8080/api/v1").unwrap_err(),
            CloudTransportError::HttpsRequired
        );
        assert_eq!(
            normalize_cloud_base("http://localhost.evil.example/api/v1").unwrap_err(),
            CloudTransportError::HttpsRequired
        );
    }

    #[test]
    fn test_normalize_invalid_urls() {
        assert_eq!(
            normalize_cloud_base("http://user:pass@127.0.0.1:8080").unwrap_err(),
            CloudTransportError::InvalidUrl
        );
        assert_eq!(
            normalize_cloud_base("http://127.0.0.1:8080?key=val").unwrap_err(),
            CloudTransportError::InvalidUrl
        );
        assert_eq!(
            normalize_cloud_base("http://127.0.0.1:8080#section").unwrap_err(),
            CloudTransportError::InvalidUrl
        );
        assert_eq!(
            normalize_cloud_base("ftp://localhost:8080").unwrap_err(),
            CloudTransportError::InvalidUrl
        );
        assert_eq!(
            normalize_cloud_base("not a url").unwrap_err(),
            CloudTransportError::InvalidUrl
        );
    }
}
