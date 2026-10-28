# Spoken Prompt Generation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When a dictation starts with “insert prompt”, send the rest to a configured OpenAI-compatible endpoint, structure it into a token-efficient Markdown prompt, and paste that instead of the raw words.

**Architecture:** A new pure-and-testable Rust module (`prompt.rs`) owns trigger detection, the meta-prompt, response validation, and the HTTP client. `lib.rs` routes every finished transcript through one `finish()` helper before the existing `deliver()`; nothing about transcription, model management, or clipboard insertion changes. A new Prompt settings tab stores the endpoint, model, and key (Keychain).

**Tech Stack:** Rust 2024, Tauri 2, `ureq` 3.5 (no `json` feature — serialize with `serde_json` and send a `String`), `serde_json`, `keyring` 4.2, `anyhow`; TypeScript + Vite frontend.

**Spec:** `docs/superpowers/specs/2026-10-28-spoken-prompt-generation-design.md`

## Global Constraints

- Work in `/Users/reza/dictation-app` on branch `spoken-prompt-generation`.
- Rust edition is 2024; the library crate is `dictation_app_lib`. Run commands from `/Users/reza/dictation-app/src-tauri`.
- Do not add dependencies. `ureq` is used **without** the `json` feature: build the body with `serde_json::to_string` and call `.send(String)`.
- The trigger phrases are fixed in code: `insert prompt`, `insert a prompt`. No user-editable phrase.
- The feature is off by default (`prompt_enabled = false`).
- The API key lives only in the Keychain (`dictation-app` / `prompt-api-key`). Never write it to `settings.json`, never log it, never return it to the frontend.
- Never log the payload or the response text. Log only lengths, error kinds, and HTTP status codes.
- A non-localhost endpoint must be `https`; `http` is allowed only for `localhost`, `127.0.0.1`, `[::1]`.
- Every failure path inserts the payload (trigger removed). The only path that inserts nothing is an empty payload.
- Output is plain Markdown with no provider-specific syntax.
- Commit at the end of every task; use imperative commit messages matching the repo’s style (no `feat:` prefixes).

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/prompt.rs` (new) | Trigger detection, `plan()`, meta-prompt, response validation, URL validation, `PromptClient`, Keychain helpers, all tests |
| `src-tauri/src/config.rs` | Three new settings fields and their defaults |
| `src-tauri/src/lib.rs` | Command wiring, `finish()` routing, `prompt-status` event |
| `src/main.ts` | Prompt panel behavior, key state, save/remove, event listener |
| `index.html` | Prompt nav entry and panel markup |
| `README.md` | Trigger docs, setup, privacy |

`src/styles.css` needs **no change**: `.field`, `.toggle`, `.problem`, `.hint`, and the `input` rules already cover the new panel.

---

### Task 1: Settings fields

**Files:**
- Modify: `src-tauri/src/config.rs` (the `Settings` struct at lines 14–22 and `Default` at lines 24–36)
- Test: `src-tauri/src/config.rs` (new `#[cfg(test)]` module at the end)

**Interfaces:**
- Consumes: nothing.
- Produces: `Settings.prompt_enabled: bool`, `Settings.prompt_base_url: String`, `Settings.prompt_model: String`; defaults `false`, `https://api.goedelmachines.com/v1`, `deepseek/deepseek-v4.1-flash`.

- [ ] **Step 1: Write the failing test**

Append to the end of `src-tauri/src/config.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prompt_settings_fall_back_to_defaults() {
        let settings: Settings = serde_json::from_str("{}").unwrap();
        assert!(!settings.prompt_enabled);
        assert_eq!(settings.prompt_base_url, "https://api.goedelmachines.com/v1");
        assert_eq!(settings.prompt_model, "deepseek/deepseek-v4.1-flash");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test config::tests::prompt_settings_fall_back_to_defaults`
Expected: FAIL to compile — `no field prompt_enabled on type Settings`.

- [ ] **Step 3: Add the fields and defaults**

In the `Settings` struct, after `cleanup_model`:

```rust
    pub prompt_enabled: bool,
    pub prompt_base_url: String,
    pub prompt_model: String,
```

In `Settings::default()`, after `cleanup_model`:

```rust
            prompt_enabled: false,
            prompt_base_url: "https://api.goedelmachines.com/v1".into(),
            prompt_model: "deepseek/deepseek-v4.1-flash".into(),
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test config::tests::prompt_settings_fall_back_to_defaults`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/config.rs
git commit -m "Add prompt settings fields"
```

---

### Task 2: Detection, routing plan, and validation

**Files:**
- Create: `src-tauri/src/prompt.rs`
- Modify: `src-tauri/src/lib.rs` (add `pub mod prompt;` in the module list at the top)
- Test: `src-tauri/src/prompt.rs` (`#[cfg(test)]` modules)

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `pub fn trigger_payload(transcript: &str) -> Option<&str>`
  - `pub enum PromptAction { Plain, EmptyPayload, NoKey { payload: String }, Structure { payload: String } }`
  - `pub fn plan(transcript: &str, enabled: bool, has_key: bool) -> PromptAction`
  - `pub fn validate_response(raw: &str) -> anyhow::Result<String>`
  - `pub const META_PROMPT: &str`

- [ ] **Step 1: Create the module with tests and `todo!()` stubs**

Create `src-tauri/src/prompt.rs`:

```rust
//! Trigger detection, routing, and response validation for spoken prompt generation.

use anyhow::Result;

pub const META_PROMPT: &str = "";

const TRIGGERS: [&str; 2] = ["insert a prompt", "insert prompt"];

pub fn trigger_payload(_transcript: &str) -> Option<&str> {
    todo!()
}

#[derive(Debug, PartialEq, Eq)]
pub enum PromptAction {
    Plain,
    EmptyPayload,
    NoKey { payload: String },
    Structure { payload: String },
}

pub fn plan(_transcript: &str, _enabled: bool, _has_key: bool) -> PromptAction {
    todo!()
}

pub fn validate_response(_raw: &str) -> Result<String> {
    todo!()
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
}
```

Add `pub mod prompt;` to the module list at the top of `src-tauri/src/lib.rs`:

```rust
pub mod prompt;
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test prompt::tests`
Expected: the tests compile but fail with `not yet implemented`.

- [ ] **Step 3: Implement detection, planning, and validation**

Replace the stubs in `src-tauri/src/prompt.rs` above the `#[cfg(test)]` module with:

```rust
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
        let rest = rest.trim_start_matches([' ', '\t', ',', ':', '.', '—', '-']);
        return Some(rest.trim());
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
        if let Some(newline) = text.find('\n') {
            return text[newline + 1..text.len() - 3].trim();
        }
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test prompt::tests`
Expected: PASS (all 10 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/prompt.rs src-tauri/src/lib.rs
git commit -m "Detect the insert prompt trigger and plan routing"
```

---

### Task 3: HTTP client, URL validation, and Keychain helpers

**Files:**
- Modify: `src-tauri/src/prompt.rs` (add imports, client, helpers, and a network test module)
- Test: `src-tauri/src/prompt.rs` (`mod http_tests`)

**Interfaces:**
- Consumes: `validate_response`, `META_PROMPT` from Task 2.
- Produces:
  - `pub fn validate_base_url(base: &str) -> anyhow::Result<()>`
  - `pub struct PromptClient` with `pub fn new(base_url: String, model: String, key: String, timeout: Duration) -> Self` and `pub fn structure(&self, payload: &str) -> anyhow::Result<String>`
  - `pub fn stored_key() -> Option<String>`
  - `pub fn save_key(key: &str) -> anyhow::Result<()>`

- [ ] **Step 1: Write the failing tests**

Add at the end of `src-tauri/src/prompt.rs` (after the existing `tests` module):

```rust
#[cfg(test)]
mod http_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::time::Duration;

    fn serve(response: String) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let handle = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut buffer = [0u8; 8192];
            let _ = stream.read(&mut buffer);
            let _ = stream.write_all(response.as_bytes());
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
        let body = r#"{"choices":[{"message":{"content":"# Objective\nWrite a poem.\n\n# Output\nA poem."}}]}"#;
        let (base, server) = serve(http("200 OK", body));
        let client = PromptClient::new(base, "test-model".into(), "test-key".into(), Duration::from_secs(2));
        let result = client.structure("write a poem").unwrap();
        assert!(result.starts_with("# Objective"));
        server.join().unwrap();
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
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test prompt::http_tests`
Expected: FAIL to compile — `PromptClient` and `validate_base_url` are not defined.

- [ ] **Step 3: Implement the client and helpers**

Change the import at the top of `src-tauri/src/prompt.rs` from `use anyhow::Result;` to:

```rust
use anyhow::{Context, Result};
use std::time::Duration;
```

Add above the `#[cfg(test)]` modules:

```rust
const KEYCHAIN_SERVICE: &str = "dictation-app";
const KEYCHAIN_ACCOUNT: &str = "prompt-api-key";

pub fn validate_base_url(base: &str) -> Result<()> {
    let lower = base.trim().to_ascii_lowercase();
    if lower.starts_with("https://") && lower.len() > "https://".len() {
        return Ok(());
    }
    if let Some(rest) = lower.strip_prefix("http://") {
        let host = rest.split(['/', ':']).next().unwrap_or("");
        let local = matches!(host, "localhost" | "127.0.0.1") || rest.starts_with("[::1]");
        if local {
            return Ok(());
        }
    }
    anyhow::bail!("the endpoint must use https, except for localhost")
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
        let url = format!("{}/chat/completions", self.base_url.trim_end_matches('/'));
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
        let _ = entry.delete_credential();
        Ok(())
    } else {
        entry.set_password(key)?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test prompt::`
Expected: PASS (16 tests: 10 from Task 2 plus 6 here). The timeout test takes about 2 seconds.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/prompt.rs
git commit -m "Add the prompt completion client and keychain helpers"
```

---

### Task 4: Route finished transcripts through the prompt path

**Files:**
- Modify: `src-tauri/src/lib.rs` (structs near line 15, `save_settings` at 73–78, commands at 125–144, `toggle` at 195–223, `deliver` at 227–240, handler list at 304–319)

**Interfaces:**
- Consumes: `prompt::plan`, `prompt::PromptClient`, `prompt::stored_key`, `prompt::save_key`, `prompt::validate_base_url` from Tasks 2–3.
- Produces: Tauri commands `has_prompt_key() -> bool` and `save_prompt_key(key: String) -> Result<(), String>`; event `prompt-status` carrying `{ state, message }`; `fn finish(app: &AppHandle, text: String) -> String`.

- [ ] **Step 1: Add the key commands**

In `src-tauri/src/lib.rs`, after the existing `save_api_key` command:

```rust
#[tauri::command]
fn has_prompt_key() -> bool {
    prompt::stored_key().is_some()
}

#[tauri::command]
fn save_prompt_key(key: String) -> Result<(), String> {
    prompt::save_key(&key).map_err(|e| format!("{e:#}"))
}
```

- [ ] **Step 2: Validate the endpoint when settings are saved**

In `save_settings`, before `config::save`:

```rust
    if settings.prompt_enabled || !settings.prompt_base_url.trim().is_empty() {
        prompt::validate_base_url(&settings.prompt_base_url).map_err(|e| format!("{e:#}"))?;
    }
```

- [ ] **Step 3: Add the status payload and the routing helper**

After the `DownloadProgress` struct:

```rust
#[derive(Serialize, Clone)]
struct PromptStatus {
    state: &'static str,
    message: Option<String>,
}
```

After the `deliver` function:

```rust
/// Routes a finished transcript through prompt mode, then inserts the result.
/// Returns the text that was actually delivered.
fn finish(app: &tauri::AppHandle, text: String) -> String {
    let settings = config::load();
    let key = prompt::stored_key();
    match prompt::plan(&text, settings.prompt_enabled, key.is_some()) {
        prompt::PromptAction::Plain => {
            deliver(app, text.clone());
            text
        }
        prompt::PromptAction::EmptyPayload => {
            logfile::line("prompt trigger with an empty payload");
            let _ = app.emit(
                "prompt-status",
                PromptStatus {
                    state: "failed",
                    message: Some("Nothing after \u{201c}insert prompt\u{201d}.".into()),
                },
            );
            String::new()
        }
        prompt::PromptAction::NoKey { payload } => {
            logfile::line("prompt trigger without a saved key");
            let _ = app.emit(
                "prompt-status",
                PromptStatus {
                    state: "failed",
                    message: Some("No API key saved — inserted the raw text.".into()),
                },
            );
            deliver(app, payload.clone());
            payload
        }
        prompt::PromptAction::Structure { payload } => {
            let _ = app.emit(
                "prompt-status",
                PromptStatus { state: "structuring", message: None },
            );
            let client = prompt::PromptClient::new(
                settings.prompt_base_url,
                settings.prompt_model,
                key.unwrap_or_default(),
                std::time::Duration::from_secs(12),
            );
            match client.structure(&payload) {
                Ok(structured) => {
                    logfile::line(format!("structured prompt: {} chars", structured.chars().count()));
                    let _ = app.emit(
                        "prompt-status",
                        PromptStatus { state: "done", message: None },
                    );
                    deliver(app, structured.clone());
                    structured
                }
                Err(error) => {
                    logfile::line(format!("prompt structuring failed: {error:#}"));
                    let _ = app.emit(
                        "prompt-status",
                        PromptStatus {
                            state: "failed",
                            message: Some("Couldn't structure the prompt — inserted the raw text.".into()),
                        },
                    );
                    deliver(app, payload.clone());
                    payload
                }
            }
        }
    }
}
```

- [ ] **Step 4: Use `finish` from the shortcut path**

In `toggle`, replace `Ok(text) if !text.is_empty() => deliver(&app, text),` with:

```rust
                Ok(text) if !text.is_empty() => {
                    finish(&app, text);
                }
```

- [ ] **Step 5: Use `finish` from the window button path**

Replace the whole `stop_dictation` command with:

```rust
#[tauri::command]
async fn stop_dictation(app: tauri::AppHandle) -> Result<String, String> {
    let handle = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let text = {
            let dictation = handle.state::<Dictation>();
            dictation.stop().map_err(|e| format!("{e:#}"))?
        };
        let _ = handle.emit("recording", false);
        if text.is_empty() {
            return Ok(String::new());
        }
        Ok(finish(&handle, text))
    })
    .await
    .map_err(|e| format!("{e}"))?
}
```

- [ ] **Step 6: Register the commands**

In the `generate_handler!` list, after `save_api_key,`:

```rust
            has_prompt_key,
            save_prompt_key,
```

- [ ] **Step 7: Run the whole suite and build**

Run: `cargo test`
Expected: PASS, including the existing `tests::captured_shortcuts_parse` and `tests::default_shortcut_is_valid`.

Run: `cargo build`
Expected: compiles with no errors.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/lib.rs
git commit -m "Route dictated triggers through the prompt structuring path"
```

---

### Task 5: Prompt settings panel

**Files:**
- Modify: `index.html` (nav at lines 27–31, new panel after the model panel ending at line 81)
- Modify: `src/main.ts` (`Settings` interface at 6–14, `load()` at 241–257, listeners at 361–400, save/remove handlers near line 346)

**Interfaces:**
- Consumes: `has_prompt_key`, `save_prompt_key`, `save_settings` commands; `prompt-status` event.
- Produces: UI only.

- [ ] **Step 1: Add the nav entry**

In `index.html`, after the Model nav button:

```html
          <button class="nav" data-panel="prompt">Prompt</button>
```

- [ ] **Step 2: Add the panel**

In `index.html`, after the `panel-model` section:

```html
        <section class="panel" id="panel-prompt">
          <h2>Prompt generation</h2>
          <p class="hint">Start a dictation with “insert prompt” to convert the rest into a structured prompt.</p>

          <label class="toggle">
            <input type="checkbox" id="prompt-enabled" />
            <span>Structure dictated prompts</span>
          </label>

          <label class="field">
            <span>Endpoint</span>
            <input type="url" id="prompt-base-url" placeholder="https://api.example.com/v1" />
          </label>

          <label class="field">
            <span>Model</span>
            <input type="text" id="prompt-model" placeholder="deepseek/deepseek-v4.1-flash" />
          </label>

          <label class="field">
            <span>API key</span>
            <input type="password" id="prompt-key" autocomplete="off" />
          </label>
          <p class="hint" id="prompt-key-state"></p>
          <p class="hint">Audio stays on this machine. Only the words after the trigger are sent to this endpoint.</p>

          <div class="field">
            <button class="primary" id="save-prompt">Save</button>
            <button class="secondary" id="remove-prompt-key">Remove key</button>
          </div>
          <p id="prompt-problem" class="problem hidden"></p>
        </section>
```

- [ ] **Step 3: Extend the TypeScript settings interface**

In `src/main.ts`, add to `interface Settings`:

```ts
  prompt_enabled: boolean;
  prompt_base_url: string;
  prompt_model: string;
```

- [ ] **Step 4: Add the key-state helpers**

After the `showHotkeyProblem` function in `src/main.ts`:

```ts
async function renderPromptKeyState() {
  const saved = await invoke<boolean>("has_prompt_key");
  el("#prompt-key-state").textContent = saved
    ? "A key is saved in the Keychain."
    : "No key saved.";
  el<HTMLInputElement>("#prompt-key").placeholder = saved
    ? "Saved — type to replace"
    : "Paste an API key";
}

function showPromptProblem(problem: string | null) {
  const line = el("#prompt-problem");
  line.textContent = problem ?? "";
  line.classList.toggle("hidden", !problem);
}
```

- [ ] **Step 5: Populate the panel on load**

In `load()` after `showHotkeyProblem(...)`:

```ts
  el<HTMLInputElement>("#prompt-enabled").checked = settings.prompt_enabled;
  el<HTMLInputElement>("#prompt-base-url").value = settings.prompt_base_url;
  el<HTMLInputElement>("#prompt-model").value = settings.prompt_model;
  await renderPromptKeyState();
```

- [ ] **Step 6: Wire the save and remove buttons**

In the `DOMContentLoaded` handler, after the `#save-general` listener:

```ts
  el("#save-prompt").addEventListener("click", async () => {
    const keyInput = el<HTMLInputElement>("#prompt-key");
    settings.prompt_enabled = el<HTMLInputElement>("#prompt-enabled").checked;
    settings.prompt_base_url = el<HTMLInputElement>("#prompt-base-url").value.trim();
    settings.prompt_model = el<HTMLInputElement>("#prompt-model").value.trim();
    try {
      await invoke("save_settings", { settings });
      if (keyInput.value) {
        await invoke("save_prompt_key", { key: keyInput.value });
        keyInput.value = "";
      }
      showPromptProblem(null);
      setStatus("Saved");
      await renderPromptKeyState();
    } catch (error) {
      showPromptProblem(String(error));
      setStatus("Prompt settings not saved");
    }
  });

  el("#remove-prompt-key").addEventListener("click", async () => {
    await invoke("save_prompt_key", { key: "" });
    await renderPromptKeyState();
    setStatus("API key removed");
  });
```

- [ ] **Step 7: Show structuring status**

After the `dictation-error` listener in `src/main.ts`:

```ts
  await listen<{ state: string; message?: string | null }>("prompt-status", (event) => {
    const { state, message } = event.payload;
    if (state === "structuring") {
      transcriptState.textContent = "Structuring prompt…";
      setStatus("Structuring prompt…");
    } else if (state === "failed") {
      setStatus(message ?? "Prompt structuring failed");
    } else {
      setStatus("Prompt ready");
    }
  });
```

- [ ] **Step 8: Build the frontend**

Run: `npm run build`
Expected: `tsc` and vite complete with no type errors.

- [ ] **Step 9: Commit**

```bash
git add index.html src/main.ts
git commit -m "Add the prompt settings panel"
```

---

### Task 6: Documentation and end-to-end verification

**Files:**
- Modify: `README.md`

**Interfaces:**
- Consumes: everything above.
- Produces: user-facing documentation.

- [ ] **Step 1: Document the feature**

In `README.md`, add a section before the privacy section:

```markdown
## Prompt generation

Say **“insert prompt”** at the start of a dictation and the rest is rewritten into a structured Markdown
prompt (Objective, Context, Steps, Constraints, Output) and pasted at the cursor instead of the raw words.
The result is plain Markdown, so it works in any model’s chat box.

Enable it in **Prompt** and configure an OpenAI-compatible endpoint, model, and API key. The key is stored in
the OS keychain. The feature is off by default; if it is enabled without a key, the trigger still works but
only the raw text is inserted.

When a request fails or times out, the raw words after the trigger are inserted, so nothing is lost.
```

Add to the privacy list:

```markdown
- Prompt generation sends only the text after the trigger, only when enabled.
```

- [ ] **Step 2: Run the full test suite**

Run from `/Users/reza/dictation-app/src-tauri`: `cargo test`
Expected: PASS (all unit and HTTP tests).

- [ ] **Step 3: Build the frontend**

Run from `/Users/reza/dictation-app`: `npm run build`
Expected: no errors.

- [ ] **Step 4: Manual end-to-end check**

Run: `npm run tauri dev`

Then:

1. Open the **Prompt** tab. The toggle is off, and the key state says “No key saved.”
2. Turn the toggle on, leave the default endpoint and model, paste a working API key, and press **Save**.
   Expected: “Saved”, and the key state reads “A key is saved in the Keychain.”
3. Quit and reopen the app. Expected: the toggle is still on and the key still shows as saved.
4. Click into a text editor. Dictate an ordinary sentence without the trigger. Expected: it is inserted
   exactly as spoken, with no extra latency.
5. Dictate “insert prompt, write a haiku about autumn.” Expected: after a pause, a Markdown prompt with an
   `# Objective` heading is pasted; the raw trigger is not present.
6. Open `~/Library/Application Support/dictation-app/dictation.log` (the path is `config_dir()/dictation-app/dictation.log`).
   Expected: a `structured prompt: N chars` line, and no prompt text.
7. Remove the key, then dictate a triggered sentence again. Expected: the words after the trigger are
   pasted, and the app reports that no key is saved.
8. Set the endpoint to `http://example.com/v1` and press Save. Expected: an inline error, and the settings
   are not saved.

- [ ] **Step 5: Commit**

```bash
git add README.md
git commit -m "Document spoken prompt generation"
```

---

## Self-Review Notes

- Every spec section maps to a task: settings (§9 → Task 1), detection (§4 → Task 2), routing and failure
  behaviour (§3, §11 → Tasks 2 + 4), request contract and meta-prompt (§6, §7 → Tasks 2 + 3), validation
  (§8 → Task 2), secrets and URL rules (§9 → Tasks 1 + 3), UI (§10 → Task 5), concurrency (§12 → Task 4),
  testing (§13 → Tasks 2, 3, 6), docs → Task 6.
- `PromptAction` and `finish` use the same field names produced by Task 2 (`NoKey { payload }`,
  `Structure { payload }`).
- The API key never crosses the boundary into `settings.json` or the frontend; `has_prompt_key` returns only
  a boolean.
- No task logs payload or response content.
