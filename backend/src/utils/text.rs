/// Truncate text by Unicode scalar values without splitting UTF-8 characters.
pub fn truncate_chars(text: &str, max_chars: usize) -> String {
    let mut chars = text.chars();
    let prefix: String = chars.by_ref().take(max_chars).collect();

    if chars.next().is_some() {
        format!("{}...", prefix)
    } else {
        prefix
    }
}

#[cfg(test)]
mod tests {
    use super::truncate_chars;

    #[test]
    fn truncate_ascii_without_panicking() {
        assert_eq!(truncate_chars("hello world", 5), "hello...");
    }

    #[test]
    fn truncate_chinese_without_splitting_characters() {
        assert_eq!(truncate_chars("你好世界测试", 5), "你好世界测...");
    }

    #[test]
    fn truncate_mixed_text_without_splitting_characters() {
        assert_eq!(truncate_chars("hello你好world", 7), "hello你好...");
    }

    #[test]
    fn truncate_emoji_without_splitting_characters() {
        assert_eq!(truncate_chars("😀你好", 2), "😀你...");
    }

    #[test]
    fn truncate_empty_text_stays_empty() {
        assert_eq!(truncate_chars("", 5), "");
    }

    #[test]
    fn truncate_long_text_has_expected_character_length() {
        let result = truncate_chars(&"a".repeat(100), 10);
        assert_eq!(result, "aaaaaaaaaa...");
        assert_eq!(result.chars().count(), 13);
        assert!(result.is_char_boundary(result.len()));
    }
}
