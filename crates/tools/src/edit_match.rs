//! Robust string matching for the Edit tool.
//!
//! Mirrors Claude Code's `findActualString` + `desanitizeMatchString`.
//! When an exact match fails, tries: (1) curly-quote normalization, then
//! (2) API-token de-sanitization (`<fnr>` → `<function_results>` etc.).

/// Curly quote constants.
const LEFT_SINGLE: char = '\u{2018}';
const RIGHT_SINGLE: char = '\u{2019}';
const LEFT_DOUBLE: char = '\u{201C}';
const RIGHT_DOUBLE: char = '\u{201D}';

/// Normalize curly quotes to straight quotes.
pub fn normalize_quotes(s: &str) -> String {
    s.replace([LEFT_SINGLE, RIGHT_SINGLE], "'")
        .replace([LEFT_DOUBLE, RIGHT_DOUBLE], "\"")
}

/// API-token desanitization table. The API sanitizes certain XML-ish tags;
/// the model emits the sanitized form, so we map them back when matching.
const DESANITIZATIONS: &[(&str, &str)] = &[
    ("<fnr>", "<function_results>"),
    ("<n>", "<name>"),
    ("</n>", "</name>"),
    ("<o>", "<output>"),
    ("</o>", "</output>"),
    ("<e>", "<error>"),
    ("</e>", "</error>"),
    ("<s>", "<system>"),
    ("</s>", "</system>"),
    ("<r>", "<result>"),
    ("</r>", "</result>"),
];

fn desanitize(s: &str) -> String {
    let mut result = s.to_string();
    for (from, to) in DESANITIZATIONS {
        result = result.replace(from, to);
    }
    result
}

/// Find the actual substring in `file_content` matching `search`, accounting
/// for quote normalization and token desanitization. Returns the exact
/// substring as it appears in the file, or `None` if no match.
pub fn find_actual_string(file_content: &str, search: &str) -> Option<String> {
    // 1. Exact match.
    if file_content.contains(search) {
        return Some(search.to_string());
    }
    // 2. Quote-normalized match.
    let norm_search = normalize_quotes(search);
    let norm_file = normalize_quotes(file_content);
    if let Some(idx) = norm_file.find(&norm_search) {
        // Map the normalized index back to the original file's bytes.
        // Since normalize_quotes is a char-for-char replacement that can
        // change byte length (curly quotes are 3 bytes, straight 1), we
        // recover by char-counting. Simpler + robust: walk chars.
        let char_start = norm_file[..idx].chars().count();
        let char_len = norm_search.chars().count();
        let actual: String = file_content
            .chars()
            .skip(char_start)
            .take(char_len)
            .collect();
        return Some(actual);
    }
    // 3. Desanitized match.
    let desan_search = desanitize(search);
    if file_content.contains(&desan_search) {
        return Some(desan_search);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_match() {
        assert_eq!(
            find_actual_string("hello world", "world"),
            Some("world".into())
        );
    }

    #[test]
    fn no_match_returns_none() {
        assert_eq!(find_actual_string("hello", "xyz"), None);
    }

    #[test]
    fn curly_quote_match() {
        // File has curly quotes, search uses straight quotes.
        let file = "let s = \u{201C}hi\u{201D};";
        let found = find_actual_string(file, "\"hi\"");
        assert!(found.is_some());
        // The returned actual string should be the curly-quoted version from the file.
        assert!(found.unwrap().contains('\u{201C}'));
    }

    #[test]
    fn desanitize_match() {
        let file = "see <function_results> here";
        let found = find_actual_string(file, "<fnr>");
        assert_eq!(found, Some("<function_results>".to_string()));
    }

    #[test]
    fn normalize_quotes_works() {
        assert_eq!(normalize_quotes("\u{201C}x\u{201D}"), "\"x\"");
        assert_eq!(normalize_quotes("\u{2018}y\u{2019}"), "'y'");
    }
}
