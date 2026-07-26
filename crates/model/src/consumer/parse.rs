use serde_json::Value;

/// Extract incremental or full assistant text from a consumer-web JSON payload.
///
/// Supports several unofficial shapes used by ChatGPT/Grok-style streams and
/// simple fixture bodies:
/// - `message.content.parts[0]` (ChatGPT conversation events)
/// - `delta` / `delta.content` string
/// - `v` string patch
/// - `content` string
/// - OpenAI-style `choices[0].delta.content`
#[must_use]
pub fn extract_text_delta(value: &Value) -> Option<String> {
    if let Some(parts) = value
        .pointer("/message/content/parts")
        .and_then(Value::as_array)
    {
        if let Some(Value::String(s)) = parts.first() {
            if !s.is_empty() {
                return Some(s.clone());
            }
        }
    }

    if let Some(s) = value.get("delta").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value.pointer("/delta/content").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value
        .pointer("/choices/0/delta/content")
        .and_then(Value::as_str)
    {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value.get("v").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value.get("content").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value.pointer("/message/content").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    if let Some(s) = value.pointer("/result/message").and_then(Value::as_str) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }

    None
}

/// Whether this JSON event looks like a cumulative full-text snapshot (ChatGPT parts).
fn is_cumulative_snapshot(value: &Value) -> bool {
    value.pointer("/message/content/parts").is_some()
}

/// Parse an SSE body (or newline-delimited data lines) into ordered text deltas.
#[must_use]
pub fn parse_sse_text_deltas(body: &str) -> Vec<String> {
    let mut deltas = Vec::new();
    let mut last_cumulative: Option<String> = None;

    for line in body.lines() {
        let line = line.trim_end_matches('\r');
        let Some(data) = line.strip_prefix("data:") else {
            continue;
        };
        let data = data.trim();
        if data.is_empty() || data == "[DONE]" {
            continue;
        }
        let Ok(value) = serde_json::from_str::<Value>(data) else {
            continue;
        };
        let Some(text) = extract_text_delta(&value) else {
            continue;
        };

        if is_cumulative_snapshot(&value) {
            if let Some(prev) = &last_cumulative {
                if text == *prev {
                    continue;
                }
                if text.starts_with(prev.as_str()) {
                    deltas.push(text[prev.len()..].to_string());
                    last_cumulative = Some(text);
                    continue;
                }
            }
            // First snapshot or non-prefix reset: treat whole string as a delta.
            deltas.push(text.clone());
            last_cumulative = Some(text);
            continue;
        }

        // Pure delta events (content/delta fields): append as-is.
        deltas.push(text);
    }

    deltas
}

/// Parse a non-stream JSON body into full content (and optional single delta list).
#[must_use]
pub fn parse_json_content(body: &str) -> Option<String> {
    let value: Value = serde_json::from_str(body).ok()?;
    extract_text_delta(&value).or_else(|| {
        value
            .pointer("/choices/0/message/content")
            .and_then(Value::as_str)
            .map(str::to_string)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extracts_chatgpt_parts() {
        let v = json!({"message":{"content":{"parts":["hello"]}}});
        assert_eq!(extract_text_delta(&v).as_deref(), Some("hello"));
    }

    #[test]
    fn parses_sse_cumulative_parts() {
        let body = "\
data: {\"message\":{\"content\":{\"parts\":[\"Hel\"]}}}\n\n\
data: {\"message\":{\"content\":{\"parts\":[\"Hello\"]}}}\n\n\
data: [DONE]\n\n";
        let deltas = parse_sse_text_deltas(body);
        assert_eq!(deltas, vec!["Hel".to_string(), "lo".to_string()]);
    }

    #[test]
    fn parses_pure_content_deltas() {
        let body = "\
data: {\"content\":\"grok\"}\n\n\
data: {\"content\":\"-web\"}\n\n\
data: [DONE]\n\n";
        let deltas = parse_sse_text_deltas(body);
        assert_eq!(deltas, vec!["grok".to_string(), "-web".to_string()]);
    }
}
