//! Upload policy: the bounds a resource must satisfy before it is stored.
//!
//! This is the first guardrail on untrusted uploads. It runs before anything is
//! written, and runs again when a preview is derived from bytes read back out
//! of storage — stored bytes are not trusted either.

pub const MAX_INPUT_FILES: usize = 8;
pub const MAX_INPUT_BYTES: usize = 25 * 1024 * 1024;
pub const MAX_PREVIEW_CHARS: usize = 12_000;

/// Returns the canonical MIME type for an accepted upload, or a refusal reason.
///
/// The declared MIME must agree with the extension: a `.png` offered as
/// `text/plain` is rejected rather than stored under a lying content type.
pub fn validate_upload(
    name: &str,
    mime: &str,
    size: usize,
    count: usize,
) -> Result<String, String> {
    if count == 0 || count > MAX_INPUT_FILES || size == 0 || size > MAX_INPUT_BYTES {
        return Err("文件数量或大小超过限制".into());
    }
    if name.is_empty()
        || name.chars().count() > 255
        || name
            .chars()
            .any(|c| c.is_control() || matches!(c, '/' | '\\'))
    {
        return Err("文件名必须是安全的文件名".into());
    }
    let extension = name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    let canonical = match extension.as_str() {
        "txt" | "rs" | "ts" | "tsx" | "js" | "jsx" | "json" | "py" | "css" | "html" | "yaml"
        | "yml" | "toml" | "xml" | "sql" | "log" => "text/plain",
        "md" | "markdown" => "text/markdown",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "pdf" => "application/pdf",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        _ => return Err("不支持此文件类型".into()),
    };
    let supplied = mime
        .split(';')
        .next()
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase();
    let text_compatible = canonical.starts_with("text/")
        && (supplied.starts_with("text/")
            || matches!(
                supplied.as_str(),
                "application/json" | "application/xml" | "application/javascript"
            ));
    if !supplied.is_empty()
        && supplied != "application/octet-stream"
        && supplied != canonical
        && !text_compatible
    {
        return Err("文件类型与 MIME 不一致".into());
    }
    Ok(canonical.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upload_policy_rejects_bounds_and_mime_mismatch_before_storage() {
        assert!(validate_upload("a.txt", "text/plain", MAX_INPUT_BYTES + 1, 1).is_err());
        assert!(validate_upload("a.txt", "text/plain", 1, 9).is_err());
        assert!(validate_upload("a.exe", "application/octet-stream", 1, 1).is_err());
        assert!(validate_upload("a.png", "text/plain", 1, 1).is_err());
        assert_eq!(
            validate_upload("a.rs", "application/octet-stream", 1, 1).unwrap(),
            "text/plain"
        );
    }
}
