pub(crate) fn findings(raw: &str) -> Vec<String> {
    let uppercase = raw.to_ascii_uppercase();
    let mut findings = Vec::new();
    for pattern in [
        "OPENAI_API_KEY",
        "API_KEY=",
        "API-KEY=",
        "TOKEN=",
        "PASSWORD=",
        "SECRET=",
        "BEARER ",
    ] {
        if uppercase.contains(pattern) {
            findings.push(pattern.to_owned());
        }
    }
    for pattern in ["sk-", "/Users/", "\\Users\\"] {
        if raw.contains(pattern) {
            findings.push(pattern.to_owned());
        }
    }
    findings.sort();
    findings.dedup();
    findings
}

pub(crate) fn finding_codes(findings: &[String]) -> Vec<String> {
    let mut codes = findings
        .iter()
        .map(|finding| match finding.as_str() {
            "OPENAI_API_KEY" => "openai_api_key_name",
            "API_KEY=" | "API-KEY=" => "api_key_assignment",
            "TOKEN=" => "token_assignment",
            "PASSWORD=" => "password_assignment",
            "SECRET=" => "secret_assignment",
            "BEARER " => "bearer_token",
            "sk-" => "secret_key_prefix",
            "/Users/" | "\\Users\\" => "user_path",
            _ => "unknown",
        })
        .map(str::to_owned)
        .collect::<Vec<_>>();
    codes.sort();
    codes.dedup();
    codes
}

pub(crate) fn sanitize_text(raw: &str) -> String {
    let mut sanitized = raw.to_owned();
    for (marker, replacement) in [
        ("OPENAI_API_KEY", "redacted_provider_key_name"),
        ("API_KEY=", "redacted_api_key"),
        ("API-KEY=", "redacted_api_key"),
        ("TOKEN=", "redacted_token"),
        ("PASSWORD=", "redacted_password"),
        ("SECRET=", "redacted_secret"),
        ("BEARER ", "redacted_bearer_token"),
    ] {
        sanitized = redact_marker_value(&sanitized, marker, replacement);
    }
    sanitized = redact_prefix_value(&sanitized, "sk-", "redacted_secret_key");
    sanitized = redact_prefix_value(&sanitized, "/Users/", "redacted_user_path");
    sanitized = redact_prefix_value(&sanitized, "\\Users\\", "redacted_user_path");
    sanitized
}

fn redact_marker_value(raw: &str, marker: &str, replacement: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    loop {
        let haystack = raw[cursor..].to_ascii_uppercase();
        let Some(relative_start) = haystack.find(marker) else {
            output.push_str(&raw[cursor..]);
            break;
        };
        let start = cursor + relative_start;
        output.push_str(&raw[cursor..start]);
        output.push_str(replacement);
        let value_start = start + marker.len();
        cursor = token_end(raw, value_start);
    }
    output
}

fn redact_prefix_value(raw: &str, prefix: &str, replacement: &str) -> String {
    let mut output = String::new();
    let mut cursor = 0;
    loop {
        let Some(relative_start) = raw[cursor..].find(prefix) else {
            output.push_str(&raw[cursor..]);
            break;
        };
        let start = cursor + relative_start;
        output.push_str(&raw[cursor..start]);
        output.push_str(replacement);
        cursor = token_end(raw, start + prefix.len());
    }
    output
}

fn token_end(raw: &str, start: usize) -> usize {
    let mut end = start;
    for (relative_idx, ch) in raw[start..].char_indices() {
        if ch.is_whitespace() || matches!(ch, '"' | '\'' | '`' | ',' | ';' | ')' | ']' | '}') {
            break;
        }
        end = start + relative_idx + ch.len_utf8();
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_text_removes_detected_patterns() {
        let provider_key = format!("{}{}", "OPENAI_", "API_KEY");
        let raw = format!(
            "{}={}test API_KEY=FAKE /Users/example/project",
            provider_key, "sk-"
        );
        let sanitized = sanitize_text(&raw);
        assert!(findings(&raw).len() >= 3);
        assert!(findings(&sanitized).is_empty(), "{sanitized}");
        assert!(sanitized.contains("redacted_provider_key_name"));
        assert!(sanitized.contains("redacted_user_path"));
    }

    #[test]
    fn finding_codes_are_stable_and_deduped() {
        let codes = finding_codes(&[
            "API_KEY=".to_owned(),
            "API_KEY=".to_owned(),
            "sk-".to_owned(),
        ]);
        assert_eq!(codes, vec!["api_key_assignment", "secret_key_prefix"]);
    }
}
