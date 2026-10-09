use super::import_store::{validate_package, Package, MAX_ENTRIES, MAX_PACKAGE_BYTES};
use super::imports::{github_asset_url, normalize_package_path, public_address};
use std::collections::{BTreeMap, HashSet};
use std::io::{Cursor, Read};
use zip::read::HasZipMetadata;

fn declared_entries(bytes: &[u8]) -> Result<usize, String> {
    let end = bytes
        .windows(4)
        .rposition(|value| value == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("invalid_zip_directory")?;
    if end + 22 > bytes.len() {
        return Err("invalid_zip_directory".into());
    }
    let word = |offset: usize| u16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as usize;
    let count = word(end + 10);
    let central_size = u32::from_le_bytes(bytes[end + 12..end + 16].try_into().unwrap()) as usize;
    let mut cursor = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
    if count == 0
        || count > MAX_ENTRIES
        || word(end + 4) != 0
        || word(end + 6) != 0
        || word(end + 8) != count
        || end + 22 + word(end + 20) != bytes.len()
        || cursor.checked_add(central_size) != Some(end)
    {
        return Err("zip_directory_or_entry_limit".into());
    }
    let mut actual = 0;
    while cursor < end {
        if cursor + 46 > end || bytes[cursor..cursor + 4] != [0x50, 0x4b, 0x01, 0x02] {
            return Err("invalid_zip_directory".into());
        }
        cursor += 46 + word(cursor + 28) + word(cursor + 30) + word(cursor + 32);
        actual += 1;
        if actual > MAX_ENTRIES || cursor > end {
            return Err("zip_directory_or_entry_limit".into());
        }
    }
    if actual != count {
        return Err("zip_directory_count_mismatch".into());
    }
    Ok(count)
}

pub fn inspect_zip(bytes: &[u8], source: &str) -> Result<Package, String> {
    if bytes.is_empty() || bytes.len() > MAX_PACKAGE_BYTES {
        return Err("zip_input_limit".into());
    }
    let declared = declared_entries(bytes)?;
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "invalid_zip")?;
    if zip.is_empty() || zip.len() > MAX_ENTRIES || zip.len() != declared {
        return Err("zip_entry_limit".into());
    }
    let mut seen = HashSet::new();
    let mut entries = Vec::new();
    let mut total = 0u64;
    // No entry is decompressed or written until every path and size is checked.
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(|_| "invalid_zip_entry")?;
        let name = file.name().strip_suffix('/').unwrap_or(file.name());
        let path = normalize_package_path(name)?;
        let mode = file.unix_mode().unwrap_or(0) & 0o170000;
        if file.is_symlink()
            || (file.get_metadata().external_attributes & 0x400) != 0
            || !matches!(mode, 0 | 0o100000 | 0o040000)
            || file.encrypted()
            || !seen.insert(path.to_ascii_lowercase())
        {
            return Err("zip_link_or_duplicate_entry".into());
        }
        if !matches!(
            file.compression(),
            zip::CompressionMethod::Stored | zip::CompressionMethod::Deflated
        ) {
            return Err("zip_compression_unsupported".into());
        }
        total = total.checked_add(file.size()).ok_or("zip_size_limit")?;
        if total > MAX_PACKAGE_BYTES as u64
            || file.size() > 512 * 1024
            || (file.size() > 0
                && (file.compressed_size() == 0
                    || file.size() > file.compressed_size().saturating_mul(100)))
        {
            return Err("zip_expansion_limit".into());
        }
        if !file.is_dir() {
            if !(path.ends_with(".md") || path.ends_with(".json") || path.ends_with(".txt")) {
                return Err("declarative_text_files_only".into());
            }
            entries.push((index, path, file.size()));
        }
    }
    for (_, path, _) in &entries {
        if entries
            .iter()
            .any(|(_, other, _)| other != path && other.starts_with(&format!("{path}/")))
        {
            return Err("zip_file_directory_collision".into());
        }
    }
    let mut files = BTreeMap::new();
    for (index, path, size) in entries {
        let mut text = String::new();
        zip.by_index(index)
            .map_err(|_| "invalid_zip_entry")?
            .take(size + 1)
            .read_to_string(&mut text)
            .map_err(|_| "invalid_package_text")?;
        if text.len() as u64 != size {
            return Err("zip_size_mismatch".into());
        }
        files.insert(path, text);
    }
    let mut package = if let Some(manifest) = files.get("plugin.json") {
        let manifest = crate::plugin::manifest::parse_manifest(manifest)
            .map_err(|_| "invalid_plugin_manifest")?;
        for path in manifest
            .contributes
            .skills
            .iter()
            .chain(&manifest.contributes.agents)
            .chain(&manifest.contributes.workflows)
        {
            normalize_package_path(path)?;
            if !files.contains_key(path) {
                return Err("missing_plugin_contribution".into());
            }
        }
        Package {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            kind: "plugin".into(),
            source: source.into(),
            permissions: manifest.permissions,
            content_hash: String::new(),
            files,
        }
    } else {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct SkillManifest {
            id: String,
            name: String,
            version: String,
        }
        let manifest: SkillManifest =
            serde_json::from_str(files.get("skill.json").ok_or("package_manifest_required")?)
                .map_err(|_| "invalid_skill_manifest")?;
        Package {
            id: manifest.id,
            name: manifest.name,
            version: manifest.version,
            kind: "skill".into(),
            source: source.into(),
            permissions: vec!["skill.instructions.read".into()],
            content_hash: String::new(),
            files,
        }
    };
    validate_package(&mut package)?;
    Ok(package)
}

/// Public, immutable GitHub assets only; redirects and proxy-based rebinding are disabled.
pub async fn fetch_github(repository: &str, commit: &str, path: &str) -> Result<Vec<u8>, String> {
    let url = github_asset_url(repository, commit, path)?;
    let addresses = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        tokio::net::lookup_host(("raw.githubusercontent.com", 443)),
    )
    .await
    .map_err(|_| "github_dns_unavailable")?
    .map_err(|_| "github_dns_unavailable")?
    .collect::<Vec<_>>();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !public_address(address.ip()))
    {
        return Err("github_nonpublic_address".into());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs("raw.githubusercontent.com", &addresses)
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| "github_client_unavailable")?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|_| "github_download_failed")?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length > MAX_PACKAGE_BYTES as u64)
    {
        return Err("github_asset_unavailable_or_oversized".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "github_download_failed")?
    {
        if bytes.len() + chunk.len() > MAX_PACKAGE_BYTES {
            return Err("github_asset_oversized".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;
    fn archive(files: &[(&str, &str)], compression: zip::CompressionMethod) -> Vec<u8> {
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, body) in files {
            zip.start_file(
                *path,
                SimpleFileOptions::default().compression_method(compression),
            )
            .unwrap();
            zip.write_all(body.as_bytes()).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    const MANIFEST: &str = r#"{"schema_version":1,"id":"example","name":"Example","version":"1.0.0","contributes":{"skills":["SKILL.md"]}}"#;
    #[test]
    fn zip_rejects_exact_duplicates_even_when_reader_collapses_names() {
        let mut bytes = archive(
            &[
                ("plugin.json", MANIFEST),
                ("SKILL.md", "one"),
                ("skill.md", "two"),
            ],
            zip::CompressionMethod::Stored,
        );
        let offsets = bytes
            .windows(8)
            .enumerate()
            .filter(|(_, value)| *value == b"skill.md")
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        for offset in offsets {
            bytes[offset..offset + 8].copy_from_slice(b"SKILL.md");
        }
        assert!(inspect_zip(&bytes, "zip").is_err());
    }
    #[test]
    fn zip_is_inert_and_reads_only_after_all_entries_validate() {
        let bytes = archive(
            &[("plugin.json", MANIFEST), ("SKILL.md", "# Inert")],
            zip::CompressionMethod::Stored,
        );
        let package = inspect_zip(&bytes, "zip").unwrap();
        assert_eq!(package.id, "example");
        assert_eq!(package.files.len(), 2);
        assert!(!package.content_hash.is_empty());
    }
    #[test]
    fn zip_rejects_windows_reparse_attributes_before_reading_contents() {
        let mut bytes = archive(
            &[("plugin.json", MANIFEST), ("SKILL.md", "# Inert")],
            zip::CompressionMethod::Stored,
        );
        assert!(inspect_zip(&bytes, "zip").is_ok());
        let header = bytes
            .windows(4)
            .position(|window| window == [0x50, 0x4b, 0x01, 0x02])
            .unwrap();
        bytes[header + 39] |= 0x04; // FILE_ATTRIBUTE_REPARSE_POINT in external attributes.
        assert!(inspect_zip(&bytes, "zip").is_err());
    }
    #[test]
    fn zip_rejects_traversal_case_aliases_symlinks_and_bombs() {
        for path in ["../escape.md", "C:/escape.md", "/escape.md", "a\\escape.md"] {
            let bytes = archive(
                &[("plugin.json", MANIFEST), (path, "x")],
                zip::CompressionMethod::Stored,
            );
            assert!(inspect_zip(&bytes, "zip").is_err());
        }
        let duplicate = archive(
            &[
                ("plugin.json", MANIFEST),
                ("SKILL.md", "one"),
                ("skill.md", "two"),
            ],
            zip::CompressionMethod::Stored,
        );
        assert!(inspect_zip(&duplicate, "zip").is_err());
        let mut zip = zip::ZipWriter::new(Cursor::new(Vec::new()));
        zip.add_symlink("SKILL.md", "../outside", SimpleFileOptions::default())
            .unwrap();
        assert!(inspect_zip(&zip.finish().unwrap().into_inner(), "zip").is_err());
        let bomb = archive(
            &[
                ("plugin.json", MANIFEST),
                ("large.md", &"x".repeat(512 * 1024)),
            ],
            zip::CompressionMethod::Deflated,
        );
        assert!(inspect_zip(&bomb, "zip").is_err());
        let names = (0..65).map(|i| format!("{i}.md")).collect::<Vec<_>>();
        let entries = names
            .iter()
            .map(|name| (name.as_str(), "x"))
            .collect::<Vec<_>>();
        assert!(inspect_zip(&archive(&entries, zip::CompressionMethod::Stored), "zip").is_err());
    }
}
