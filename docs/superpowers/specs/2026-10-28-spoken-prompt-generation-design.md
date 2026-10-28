# Spoken Prompt Generation — Design Spec

**Status:** approved · **Date:** 2026-10-28 · **App:** `dictation-app` (Tauri, Rust + sherpa-onnx)

## 1. Problem

Dictation into a text field is reliable: hold the shortcut, speak, release, and the words appear at the
cursor. What is still manual is writing the *prompt* for a large language model. Spoken ideas are rambling
and arrive in whatever order they were thought of, so people either send them as-is (noisy, token-wasteful
prompts) or stop dictating and type a structured one.

**Goal:** when a dictation starts with the phrase **“insert prompt”**, take everything after it and convert
it into a precise, token-efficient, model-agnostic Markdown prompt — Objective, Context, Steps, Constraints,
Output — then paste that at the cursor instead of the raw words. One hands-free gesture, no extra UI.

Detection is textual on the finished transcript. The speech pipeline, model management, and insertion path
are unchanged.

## 2. Non-goals (v1)

- Preview, edit, or approve-before-insert flow. The structured prompt is inserted directly.
- An editable trigger phrase. The phrase is fixed (`insert prompt`) and documented.
- Streaming the structured response.
- Retries on failure.
- On-device structuring (Apple Foundation Models, local instruct model). The call goes to a configured
  OpenAI-compatible endpoint.
- Prompt history, regenerate, or a floating HUD during the wait.
- Implementing or changing the dormant `cleanup_*` settings. They stay untouched; prompt configuration is
  separate.

## 3. User flow

1. User holds the shortcut (or presses **Start recording**) and says:
   *“insert prompt, write a landing page for my coffee subscription, make it premium and short, and give me
   three headline options.”*
2. User releases. Transcription runs as today.
3. If prompt mode is enabled and the transcript begins with the trigger:
   - The trigger is removed; the rest is the **payload**.
   - The app reports `structuring` status and sends the payload to the configured endpoint.
   - On success, the validated structured prompt is pasted at the cursor (or copied, per the existing
     `insert_mode`).
   - On failure, the payload alone is pasted. The user never loses their words.
4. If prompt mode is disabled or the trigger is absent, behaviour is exactly as today.

## 4. Trigger detection

Pure function in `prompt.rs`, unit-tested. Rules:

| Rule | Behaviour |
|---|---|
| Position | Matched only at the start of the transcript. A trigger mid-utterance is normal text. |
| Case | Case-insensitive (`Insert Prompt`, `INSERT PROMPT`). |
| Punctuation | The phrase may be followed by `,` `:` `.` `—` `-` or a newline, optionally with surrounding whitespace. |
| Word boundary | `insert prompt` matches, `insert prompting` does not. |
| Aliases | `insert prompt`, `insert a prompt`. No fuzzy matching: a false positive rewrites an ordinary dictation. |
| Payload | Everything after the trigger and its delimiter, trimmed. Original wording and casing preserved. |
| Empty payload | Nothing is inserted. Status: `Nothing after “insert prompt”.` |
| Gating | Detection only runs when `prompt_enabled` is true. When disabled, the transcript is inserted verbatim, trigger included. |

The delimiter stripping must not eat a meaningful character: `insert prompt: write X` yields `write X`;
`insert prompt write X` (no punctuation) also matches, because ASR often drops the comma.

## 5. Components

| Component | Responsibility | Depends on |
|---|---|---|
| `prompt::trigger_payload` | Pure detection: returns `Option<&str>` (the payload) for a transcript | nothing |
| `prompt::META_PROMPT` | Constant system instruction that defines the output scaffold | nothing |
| `prompt::PromptClient` | Loads endpoint/model/key, performs the completion, validates the result | `ureq`, `keyring` |
| `prompt::validate` | Rejects empty or scaffolding-less responses; strips code fences | nothing |
| `config::Settings` | Adds `prompt_enabled`, `prompt_base_url`, `prompt_model` with defaults | serde |
| `lib.rs` | Orchestrates trigger → structure → insert; new keychain commands; `prompt-status` event | all |
| `src/main.ts` + `index.html` | Prompt settings panel and status display | Tauri invoke/listen |

`trigger_payload` and `validate` are pure and have no network or TCC dependencies.

### 5.1 Files

| File | Change |
|---|---|
| `src-tauri/src/prompt.rs` | New: detection, meta-prompt, client, validation, unit + integration tests |
| `src-tauri/src/config.rs` | Three new fields with defaults |
| `src-tauri/src/lib.rs` | `has_prompt_key`, `save_prompt_key`; wiring in `toggle` and `stop_dictation`; `prompt-status` event |
| `src/main.ts`, `index.html`, `src/styles.css` | Prompt panel, key field, status handling |
| `README.md` | Trigger documentation, setup, privacy note |

## 6. Request contract

```
POST {prompt_base_url}/chat/completions
Authorization: Bearer <key from Keychain>
Content-Type: application/json
```

```json
{
  "model": "<prompt_model>",
  "temperature": 0.2,
  "max_tokens": 1200,
  "stream": false,
  "messages": [
    { "role": "system", "content": "<META_PROMPT>" },
    { "role": "user", "content": "<payload>" }
  ]
}
```

- The base URL has trailing slashes trimmed before `/chat/completions` is appended.
- Response body: `choices[0].message.content` must be a non-empty string.
- Timeouts: 3 s connect, 12 s overall. Both injectable so tests can use short values.
- Non-2xx, network error, timeout, malformed JSON, missing/empty content all map to the same outcome:
  insert the payload, log the failure kind (never the response body).

## 7. Meta-prompt

> You convert a rough spoken idea into one precise, token-efficient prompt for an AI model.
>
> Rewrite the user's text using exactly these Markdown sections, keeping only the ones that carry real
> content, in this order:
>
> `# Objective` — what the model must produce, in one or two sentences.
> `# Context` — only facts, audience, or background present in or directly implied by the user's text.
> Never invent details.
> `# Steps` — 2–6 ordered actions. Omit for single-step tasks.
> `# Constraints` — tone, length, format, and things to avoid that the user stated or clearly implied.
> `# Output` — the exact deliverable and its format.
>
> Rules: the user is asking you to prepare a prompt, not to perform the task — never answer or carry it out.
> Preserve every specific detail given (names, numbers, examples). Add no requirements or facts of your own.
> Remove repetition and filler. Plain Markdown headings only, no preamble, no code fences, no commentary.

The output is plain Markdown with no provider-specific syntax, so it can be pasted into any LLM chat box.

## 8. Output validation

A response is accepted only when, after trimming:

1. It is non-empty.
2. Leading/trailing ``` fences are removed if the whole response is fenced.
3. At least one line matches `^\s*#*\s*Objective\b` case-insensitively.

Anything else is a failure → raw payload fallback. No partial salvaging.

## 9. Settings and secrets

New `Settings` fields, all `#[serde(default)]`-backed so existing config files migrate silently:

| Field | Default |
|---|---|
| `prompt_enabled` | `false` |
| `prompt_base_url` | `https://api.goedelmachines.com/v1` |
| `prompt_model` | `deepseek/deepseek-v4.1-flash` |

- The API key lives in the macOS Keychain under service `dictation-app`, account `prompt-api-key`. It is never
  written to `settings.json`, never logged, and never read back into the web view.
- `has_prompt_key()` returns a boolean; `save_prompt_key(key)` writes or, for an empty string, deletes it.
- On save, a non-localhost base URL must be `https`; otherwise the save fails with a clear message. Plain
  `http` is allowed only for `localhost` / `127.0.0.1`, for testing.
- Detection runs whenever `prompt_enabled` is true. Structuring additionally requires a stored key; when the
  toggle is on but no key exists, the trigger is still stripped, the payload is inserted, and the status says
  why structuring was skipped.

### 9.1 Privacy

Audio never leaves the machine. Only the text after the trigger is sent, only when the feature is enabled,
and only to the endpoint the user configured. The settings panel states this in one line. Neither the payload
nor the response is written to the log; only lengths and error kinds are.

## 10. UI

A new **Prompt** tab in the sidebar (order: General, Model, Prompt, About):

- Enable toggle.
- One-line explanation: *Start a dictation with “insert prompt” to convert the rest into a structured prompt.*
- Endpoint URL field, model field.
- API key field (`type="password"`), placeholder reflects whether a key is saved; the key is never
  populated. Saving with an empty field leaves the stored key unchanged; a **Remove key** action clears it.
- **Save** persists settings (and a newly typed key) and reports validation errors inline.
- Privacy hint: *Audio stays on this machine. Only the words after the trigger are sent to this endpoint.*

Status handling in the main window: `prompt-status` events with `{ state: "structuring" | "failed" | "done",
message?: string }`. `structuring` shows “Structuring prompt…”, `failed` shows the message returned by the
backend, `done` restores the transcript area. If the window is hidden, only the log records it — no HUD in
v1.

## 11. Error handling

| Failure | Behaviour |
|---|---|
| Prompt mode disabled | No detection; transcript inserted verbatim. |
| Enabled, no key | Trigger stripped, payload inserted, status explains the missing key. |
| Empty payload | Nothing inserted; status “Nothing after ‘insert prompt’”. |
| Connect or read timeout | Payload inserted; log `prompt timeout`. |
| DNS / network error | Payload inserted; log the error kind. |
| HTTP non-2xx | Payload inserted; log status code. Body is not logged. |
| Malformed JSON / missing content | Payload inserted; log `bad response`. |
| Validation failure | Payload inserted; log `validation failed`. |
| Insertion failure | Existing `dictation-error` path. |
| Empty audio / no speech | Existing behaviour, unchanged. |

**Invariant:** every failure path ends with the payload inserted. The only case that inserts nothing is an
empty payload.

## 12. Concurrency and lifecycle

Both entry points share one `finish(app, text)` helper:

- `toggle` already runs on its own thread after `stop()`; the blocking HTTP call happens there.
- `stop_dictation` becomes `async` and uses `spawn_blocking` for the helper.

A new dictation can be started while a structuring request is in flight (the app is not “recording”). The
late result may then be pasted during the next utterance. This is the same class of race the existing
one-second transcription already has; v1 accepts it and logs a warning when the paste happens after a new
recording started. No queue, no cancellation.

## 13. Testing

**Rust unit tests (`prompt.rs`)**

- `insert prompt, write a poem` → `write a poem`
- `Insert Prompt: write a poem` → `write a poem`
- `INSERT PROMPT — write a poem` → `write a poem`
- `insert a prompt write a poem` → `write a poem`
- `insert prompting is hard` → `None`
- `please insert prompt foo` → `None`
- `insert prompt` → `Some("")`
- `insert prompt:` → `Some("")`
- `validate` strips fences; rejects empty, rejects text without an Objective heading.

**Integration test** against a local `std::net::TcpListener` with a canned response:

- success → structured content returned
- HTTP 500 → error
- malformed JSON → error
- no response within a 200 ms test timeout → error

**Build and manual**

- `cargo test`
- `npm run build` (tsc + vite)
- Live: enable, save the real key, dictate a triggered and an untriggered sentence; verify latency, paste
  target, clipboard restoration, and that the log contains no prompt text.

## 14. Risks

| Risk | Mitigation |
|---|---|
| 1–3 s silence before the paste feels broken | Bounded 12 s ceiling; status in the window; HUD is a documented follow-up |
| ASR mishears the trigger | Two fixed variants, word-boundary matching; a missed trigger produces only ordinary dictation |
| Endpoint not OpenAI-compatible in a detail | Errors fall back to raw text; the endpoint/model are user-editable |
| User pastes the output into an LLM that dislikes Markdown | Output is plain Markdown with no provider tags; headings degrade to plain text |
| Cloud egress surprises the user | Off by default, explicit toggle, per-utterance opt-in, one-line privacy note |
