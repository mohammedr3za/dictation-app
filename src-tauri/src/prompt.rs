//! Trigger detection, routing, and response validation for spoken prompt generation.

use anyhow::{Context, Result};
use std::time::Duration;

pub const META_PROMPT: &str = "\
You convert a rough spoken idea into one precise, token-efficient prompt for an AI model.

Rewrite the user's text using exactly these Markdown sections, keeping only the ones that carry real \
content, in this order:

# Objective
What the model must produce, in one or two sentences.

# Context
Only facts, audience, or background present in or directly implied by the user's text. Never invent details.

# Steps
2-6 ordered actions. Omit for single-step tasks.

# Constraints
Tone, length, format, and things to avoid that the user stated or clearly implied.

# Output
The exact deliverable and its format.

Rules: the user is asking you to prepare a prompt, not to perform the task — never answer or carry it out. \
Preserve every specific detail given (names, numbers, examples). Add no requirements or facts of your own. \
Remove repetition and filler. Plain Markdown headings only, no preamble, no code fences, no commentary.";

const TRIGGERS: [&str; 2] = ["insert a prompt", "insert prompt"];

fn starts_with_ignore_ascii_case(haystack: &str, needle: &str) -> bool {
    haystack.len() >= needle.len()
        && haystack.as_bytes()[..needle.len()].eq_ignore_ascii_case(needle.as_bytes())
}

fn strip_leading_separators(text: &str) -> &str {
    let mut rest = text.trim_start();
    loop {
        let mut chars = rest.chars();
        let Some(first) = chars.next() else { return rest };
        if !matches!(first, ',' | ':' | '.' | '—' | '-') {
            return rest;
        }
        // A minus sign followed by a digit belongs to the payload, not the delimiter.
        if first == '-' && chars.next().is_some_and(|c| c.is_ascii_digit()) {
            return rest;
        }
        rest = rest[first.len_utf8()..].trim_start();
    }
}

pub fn trigger_payload(transcript: &str) -> Option<&str> {
    let text = transcript.trim_start();
    for trigger in TRIGGERS {
        if !starts_with_ignore_ascii_case(text, trigger) {
            continue;
        }
        let rest = &text[trigger.len()..];
        if rest.chars().next().is_some_and(|c| c.is_alphanumeric()) {
            continue;
        }
        return Some(strip_leading_separators(rest).trim_end());
    }
    None
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptAction {
    Plain,
    EmptyPayload,
    NoKey { payload: String },
    Structure { payload: String },
}

pub fn plan(transcript: &str, enabled: bool, has_key: bool) -> PromptAction {
    if !enabled {
        return PromptAction::Plain;
    }
    match trigger_payload(transcript) {
        None => PromptAction::Plain,
        Some(payload) if payload.is_empty() => PromptAction::EmptyPayload,
        Some(payload) if !has_key => PromptAction::NoKey { payload: payload.into() },
        Some(payload) => PromptAction::Structure { payload: payload.into() },
    }
}

fn strip_code_fence(text: &str) -> &str {
    if text.starts_with("```") && text.ends_with("```") && text.len() > 6 {
        let inner = &text[3..text.len() - 3];
        let inner = inner.trim_start();
        let inner = match inner.find('\n') {
            Some(newline) if !inner[..newline].contains('#') => &inner[newline + 1..],
            _ => inner,
        };
        return inner.trim();
    }
    text
}

fn has_objective_heading(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim_start_matches(|c: char| c == '#' || c.is_whitespace());
        let lower = line.to_ascii_lowercase();
        match lower.strip_prefix("objective") {
            Some(rest) => rest.chars().next().is_none_or(|c| !c.is_alphanumeric()),
            None => false,
        }
    })
}

pub fn validate_response(raw: &str) -> Result<String> {
    let text = strip_code_fence(raw.trim());
    if text.is_empty() {
        anyhow::bail!("empty structured prompt");
    }
    if !has_objective_heading(text) {
        anyhow::bail!("no Objective section");
    }
    Ok(text.to_string())
}

const KEYCHAIN_SERVICE: &str = "dictation-app";
const KEYCHAIN_ACCOUNT: &str = "prompt-api-key";

pub fn validate_base_url(base: &str) -> Result<()> {
    let lower = base.trim().to_ascii_lowercase();
    if lower.starts_with("https://") && lower.len() > "https://".len() {
        return Ok(());
    }
    let Some(rest) = lower.strip_prefix("http://") else {
        anyhow::bail!("the endpoint must use https, except for localhost")
    };
    // The authority ends at the path, query, or fragment. Userinfo makes the real
    // host ambiguous (ureq strips it), so anything containing `@` is rejected.
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    if authority.contains('@') {
        anyhow::bail!("the endpoint must use https, except for localhost")
    }
    let local = if let Some(after) = authority.strip_prefix("[::1]") {
        after.is_empty() || after.starts_with(':')
    } else {
        matches!(authority.split(':').next().unwrap_or(""), "localhost" | "127.0.0.1")
    };
    if local {
        Ok(())
    } else {
        anyhow::bail!("the endpoint must use https, except for localhost")
    }
}

pub struct PromptClient {
    base_url: String,
    model: String,
    key: String,
    timeout: Duration,
}

impl PromptClient {
    pub fn new(base_url: String, model: String, key: String, timeout: Duration) -> Self {
        Self { base_url, model, key, timeout }
    }

    pub fn structure(&self, payload: &str) -> Result<String> {
        validate_base_url(&self.base_url)?;
        let url = format!(
            "{}/chat/completions",
            self.base_url.trim().trim_end_matches('/')
        );
        let body = serde_json::json!({
            "model": self.model,
            "temperature": 0.2,
            "max_tokens": 1200,
            "stream": false,
            "messages": [
                { "role": "system", "content": META_PROMPT },
                { "role": "user", "content": payload },
            ],
        });
        let body = serde_json::to_string(&body).context("encoding the request")?;

        let config = ureq::Agent::config_builder()
            .timeout_connect(Some(Duration::from_secs(3)))
            .timeout_global(Some(self.timeout))
            .build();
        let agent = ureq::Agent::new_with_config(config);

        let mut response = agent
            .post(&url)
            .header("Authorization", format!("Bearer {}", self.key))
            .header("Content-Type", "application/json")
            .send(body)
            .context("calling the prompt endpoint")?;
        let text = response
            .body_mut()
            .read_to_string()
            .context("reading the response")?;
        let json: serde_json::Value = serde_json::from_str(&text).context("parsing the response")?;
        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or_default();
        validate_response(content)
    }
}

pub fn stored_key() -> Option<String> {
    keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)
        .ok()?
        .get_password()
        .ok()
        .filter(|key| !key.is_empty())
}

pub fn save_key(key: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, KEYCHAIN_ACCOUNT)?;
    if key.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    } else {
        entry.set_password(key)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_the_payload_after_the_trigger() {
        assert_eq!(trigger_payload("insert prompt, write a poem"), Some("write a poem"));
        assert_eq!(trigger_payload("Insert Prompt: write a poem"), Some("write a poem"));
        assert_eq!(trigger_payload("INSERT PROMPT — write a poem"), Some("write a poem"));
        assert_eq!(trigger_payload("insert a prompt\nwrite a poem"), Some("write a poem"));
    }

    #[test]
    fn ignores_the_trigger_away_from_the_start() {
        assert_eq!(trigger_payload("please insert prompt foo"), None);
    }

    #[test]
    fn requires_a_word_boundary() {
        assert_eq!(trigger_payload("insert prompting is hard"), None);
        assert_eq!(trigger_payload("insert a prompting is hard"), None);
    }

    #[test]
    fn returns_an_empty_payload_when_nothing_follows() {
        assert_eq!(trigger_payload("insert prompt"), Some(""));
        assert_eq!(trigger_payload("insert prompt:"), Some(""));
    }

    #[test]
    fn ignores_surrounding_whitespace() {
        assert_eq!(trigger_payload("   insert prompt,   write a poem   "), Some("write a poem"));
    }

    #[test]
    fn plan_leaves_ordinary_dictation_alone() {
        assert_eq!(plan("hello world", true, true), PromptAction::Plain);
        assert_eq!(plan("insert prompt, write a poem", false, true), PromptAction::Plain);
    }

    #[test]
    fn plan_routes_a_triggered_payload() {
        assert_eq!(
            plan("insert prompt, write a poem", true, true),
            PromptAction::Structure { payload: "write a poem".into() }
        );
        assert_eq!(
            plan("insert prompt, write a poem", true, false),
            PromptAction::NoKey { payload: "write a poem".into() }
        );
        assert_eq!(plan("insert prompt", true, true), PromptAction::EmptyPayload);
    }

    #[test]
    fn accepts_markdown_with_an_objective_heading() {
        let text = validate_response("# Objective\nWrite a poem.\n\n# Output\nA poem.").unwrap();
        assert!(text.starts_with("# Objective"));
        assert!(validate_response("## Objective\nDo it.").is_ok());
        assert!(validate_response("Objective\nDo it.").is_ok());
    }

    #[test]
    fn strips_a_wrapping_code_fence() {
        let text = validate_response("```markdown\n# Objective\nDo it.\n```").unwrap();
        assert_eq!(text, "# Objective\nDo it.");
    }

    #[test]
    fn rejects_responses_without_an_objective() {
        assert!(validate_response("").is_err());
        assert!(validate_response("Just some prose.").is_err());
        assert!(validate_response("# Objectives\nPlural is not the heading.").is_err());
    }

    #[test]
    fn strips_only_delimiters_and_keeps_a_minus_sign() {
        assert_eq!(trigger_payload("insert prompt. write a poem"), Some("write a poem"));
        assert_eq!(trigger_payload("insert prompt - write a poem"), Some("write a poem"));
        assert_eq!(trigger_payload("insert prompt: -5 degrees"), Some("-5 degrees"));
        assert_eq!(trigger_payload("insert prompt -5 degrees"), Some("-5 degrees"));
        assert_eq!(trigger_payload("insert prompt...explain it"), Some("explain it"));
    }

    #[test]
    fn matches_the_objective_heading_case_insensitively() {
        assert!(validate_response("# OBJECTIVE\nDo it.").is_ok());
        assert!(validate_response("# objective\ndo it").is_ok());
    }

    #[test]
    fn an_empty_payload_wins_over_a_missing_key() {
        assert_eq!(plan("insert prompt", true, false), PromptAction::EmptyPayload);
    }

    #[test]
    fn strips_a_single_line_fence() {
        assert_eq!(validate_response("```# Objective\nDo it.```").unwrap(), "# Objective\nDo it.");
        assert_eq!(validate_response("```# Objective```").unwrap(), "# Objective");
    }

    #[test]
    fn strips_any_info_string_on_a_wrapping_fence() {
        assert_eq!(
            validate_response("```json\n# Objective\nDo it.\n```").unwrap(),
            "# Objective\nDo it."
        );
        assert_eq!(
            validate_response("```md5\n# Objective\n```").unwrap(),
            "# Objective"
        );
    }

    #[test]
    fn keeps_a_fence_that_does_not_wrap_the_response() {
        let text = validate_response("Here you go:\n```\n# Objective\nDo it.\n```").unwrap();
        assert!(text.contains("```"));
    }
}

#[cfg(test)]
mod http_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    fn read_request(stream: &mut std::net::TcpStream) -> String {
        let mut data = Vec::new();
        let mut buffer = [0u8; 4096];
        loop {
            let read = stream.read(&mut buffer).unwrap_or(0);
            if read == 0 {
                break;
            }
            data.extend_from_slice(&buffer[..read]);
            if let Some(end) = data.windows(4).position(|w| w == b"\r\n\r\n") {
                let headers = String::from_utf8_lossy(&data[..end]);
                let length = headers
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        if name.eq_ignore_ascii_case("content-length") {
                            value.trim().parse::<usize>().ok()
                        } else {
                            None
                        }
                    })
                    .unwrap_or(0);
                if data.len() >= end + 4 + length {
                    break;
                }
            }
        }
        String::from_utf8_lossy(&data).to_string()
    }

    fn serve(response: String) -> (String, std::thread::JoinHandle<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let request = read_request(&mut stream);
            let _ = stream.write_all(response.as_bytes());
            request
        });
        (format!("http://{address}/v1"), handle)
    }

    fn http(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    #[test]
    fn structures_a_completion() {
        let body = r##"{"choices":[{"message":{"content":"# Objective\nWrite a poem.\n\n# Output\nA poem."}}]}"##;
        let (base, server) = serve(http("200 OK", body));
        let client = PromptClient::new(base, "test-model".into(), "test-key".into(), Duration::from_secs(2));
        let result = client.structure("write a poem").unwrap();
        assert!(result.starts_with("# Objective"));
        let request = server.join().unwrap();
        assert!(
            request.starts_with("POST /v1/chat/completions "),
            "request line: {}",
            request.lines().next().unwrap_or("")
        );
        let lower = request.to_ascii_lowercase();
        assert!(lower.contains("authorization: bearer test-key"));
        assert!(lower.contains("content-type: application/json"));
        assert!(request.contains("\"model\":\"test-model\""));
        assert!(request.contains("\"temperature\":0.2"));
        assert!(request.contains("\"max_tokens\":1200"));
        assert!(request.contains("\"stream\":false"));
        assert!(request.contains("\"role\":\"system\""));
        assert!(request.contains("\"role\":\"user\""));
    }

    #[test]
    fn rejects_an_http_error() {
        let (base, server) = serve(http("500 Internal Server Error", "{}"));
        let client = PromptClient::new(base, "m".into(), "k".into(), Duration::from_secs(2));
        assert!(client.structure("write a poem").is_err());
        server.join().unwrap();
    }

    #[test]
    fn rejects_malformed_json() {
        let (base, server) = serve(http("200 OK", "not json"));
        let client = PromptClient::new(base, "m".into(), "k".into(), Duration::from_secs(2));
        assert!(client.structure("write a poem").is_err());
        server.join().unwrap();
    }

    #[test]
    fn rejects_an_unstructured_completion() {
        let body = r#"{"choices":[{"message":{"content":"Sure, here is a poem."}}]}"#;
        let (base, server) = serve(http("200 OK", body));
        let client = PromptClient::new(base, "m".into(), "k".into(), Duration::from_secs(2));
        assert!(client.structure("write a poem").is_err());
        server.join().unwrap();
    }

    #[test]
    fn times_out_when_the_server_stalls() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let _ = stream.read(&mut buffer);
            std::thread::sleep(Duration::from_secs(2));
        });
        let client = PromptClient::new(
            format!("http://{address}/v1"),
            "m".into(),
            "k".into(),
            Duration::from_millis(200),
        );
        assert!(client.structure("write a poem").is_err());
        server.join().unwrap();
    }

    #[test]
    fn validates_endpoints() {
        assert!(validate_base_url("https://api.example.com/v1").is_ok());
        assert!(validate_base_url("http://127.0.0.1:8080/v1").is_ok());
        assert!(validate_base_url("http://localhost:8080/v1").is_ok());
        assert!(validate_base_url("http://example.com/v1").is_err());
        assert!(validate_base_url("ftp://example.com").is_err());
        assert!(validate_base_url("").is_err());
        assert!(validate_base_url("http://[::1]:8080/v1").is_ok());
        assert!(validate_base_url("http://[::1]/v1").is_ok());
        assert!(validate_base_url("http://localhost:8080@evil.com/v1").is_err());
        assert!(validate_base_url("http://[::1]@evil.com/v1").is_err());
        assert!(validate_base_url("  https://api.example.com/v1  ").is_ok());
    }
}
