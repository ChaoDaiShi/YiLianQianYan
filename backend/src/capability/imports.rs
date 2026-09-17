//! Managed packages are inert data. Inspection never extracts or invokes code.
use std::net::IpAddr;

pub fn normalize_package_path(path: &str) -> Result<String, String> {
    if path.is_empty()
        || path.len() > 240
        || !path.is_ascii()
        || path.chars().any(|c| {
            c.is_control()
                || matches!(
                    c,
                    '\\' | ':' | '%' | '?' | '#' | '*' | '<' | '>' | '"' | '|'
                )
        })
    {
        return Err("unsafe_package_path".into());
    }
    for component in path.split('/') {
        let stem = component
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_uppercase();
        if component.is_empty()
            || component == "."
            || component == ".."
            || component.ends_with(['.', ' '])
            || component.starts_with(' ')
            || matches!(stem.as_str(), "CON" | "PRN" | "AUX" | "NUL")
            || (stem.len() == 4
                && (stem.starts_with("COM") || stem.starts_with("LPT"))
                && stem.as_bytes()[3].is_ascii_digit())
        {
            return Err("unsafe_package_path".into());
        }
    }
    Ok(path.to_string())
}

pub fn github_asset_url(repository: &str, commit: &str, path: &str) -> Result<String, String> {
    let url = url::Url::parse(repository).map_err(|_| "invalid_github_source")?;
    let segments = url
        .path()
        .trim_start_matches('/')
        .split('/')
        .collect::<Vec<_>>();
    if url.scheme() != "https"
        || url.host_str() != Some("github.com")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some()
        || segments.len() != 2
        || segments.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || !part
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
        || repository != format!("https://github.com/{}/{}", segments[0], segments[1])
        || commit.len() != 40
        || !commit.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("immutable_public_github_source_required".into());
    }
    let path = normalize_package_path(path)?;
    Ok(format!(
        "https://raw.githubusercontent.com/{}/{}/{commit}/{path}",
        segments[0], segments[1]
    ))
}

pub fn public_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(ip) => {
            let [a, b, _, _] = ip.octets();
            !ip.is_private()
                && !ip.is_loopback()
                && !ip.is_link_local()
                && !ip.is_documentation()
                && !ip.is_unspecified()
                && !ip.is_broadcast()
                && !ip.is_multicast()
                && a != 0
                && a < 224
                && !(a == 100 && (64..=127).contains(&b))
                && !(a == 192 && b == 0)
                && !(a == 198 && (18..=19).contains(&b))
        }
        IpAddr::V6(ip) => {
            let s = ip.segments();
            (s[0] & 0xe000) == 0x2000 && !(s[0] == 0x2001 && s[1] == 0x0db8)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn import_paths_reject_cross_platform_escape_and_aliases() {
        for path in [
            "../x", "a/../b", "/etc/x", "C:/x", "a\\b", "//host/x", "a//b", "a/./b", "a:stream",
            "CON.txt", "x. ", "a\0b",
        ] {
            assert!(normalize_package_path(path).is_err(), "unsafe: {path:?}");
        }
        assert_eq!(
            normalize_package_path("docs/SKILL.md").unwrap(),
            "docs/SKILL.md"
        );
    }

    #[test]
    fn github_import_requires_exact_public_repository_and_immutable_ref() {
        let sha = "a".repeat(40);
        for repository in [
            "http://github.com/a/b",
            "https://user:pass@github.com/a/b",
            "https://localhost/a/b",
            "https://github.com.evil/a/b",
            "https://github.com/a/b?token=secret",
            "https://github.com/a/b/tree/main",
        ] {
            assert!(github_asset_url(repository, &sha, "SKILL.md").is_err());
        }
        assert!(github_asset_url("https://github.com/a/b", "main", "SKILL.md").is_err());
        assert!(github_asset_url("https://github.com/a/b", &sha, "../x").is_err());
        assert_eq!(
            github_asset_url("https://github.com/a/b", &sha, "docs/SKILL.md").unwrap(),
            format!("https://raw.githubusercontent.com/a/b/{sha}/docs/SKILL.md")
        );
    }

    #[test]
    fn github_dns_rejects_nonpublic_addresses() {
        for address in [
            "127.0.0.1",
            "10.0.0.1",
            "169.254.169.254",
            "100.64.0.1",
            "192.168.1.2",
            "::1",
            "fe80::1",
            "fc00::1",
            "::ffff:127.0.0.1",
            "2001:db8::1",
        ] {
            assert!(
                !public_address(address.parse().unwrap()),
                "address: {address}"
            );
        }
        assert!(public_address("185.199.108.133".parse().unwrap()));
    }
}
