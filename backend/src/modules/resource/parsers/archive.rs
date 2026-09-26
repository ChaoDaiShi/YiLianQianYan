//! Bounded reading of OOXML containers (`.docx`, `.xlsx`).
//!
//! An Office file is a ZIP archive, so opening one is an untrusted-input
//! problem: entry count, per-entry size and total expanded size are all capped
//! before any entry is decompressed. Only XML parts are kept — embedded media
//! is never expanded.

use std::io::{Cursor, Read};

const MAX_ZIP_ENTRIES: usize = 512;
const MAX_ENTRY_BYTES: u64 = 8 * 1024 * 1024;
const MAX_EXPANDED_BYTES: u64 = 64 * 1024 * 1024;

pub(crate) fn zip_entries(
    bytes: &[u8],
) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
    let mut zip =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| "无法读取 Office 文档容器")?;
    if zip.len() > MAX_ZIP_ENTRIES {
        return Err("文档条目数超过限制".into());
    }
    let mut total = 0u64;
    let mut entries = std::collections::BTreeMap::new();
    for index in 0..zip.len() {
        let file = zip.by_index(index).map_err(|_| "Office 文档条目无效")?;
        if file.is_dir() {
            continue;
        }
        total = total.checked_add(file.size()).ok_or("文档展开大小无效")?;
        if file.size() > MAX_ENTRY_BYTES || total > MAX_EXPANDED_BYTES {
            return Err("文档展开大小超过限制".into());
        }
        if !file.name().ends_with(".xml") && !file.name().ends_with(".rels") {
            continue;
        }
        let name = file.name().to_string();
        let mut content = Vec::new();
        file.take(MAX_ENTRY_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|_| "Office 文档读取失败")?;
        if content.len() as u64 > MAX_ENTRY_BYTES {
            return Err("文档条目超过限制".into());
        }
        if entries.insert(name, content).is_some() {
            return Err("文档有重复条目".into());
        }
    }
    Ok(entries)
}
