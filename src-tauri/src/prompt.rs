//! Trigger detection, routing, and response validation for spoken prompt generation.

use anyhow::Result;

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
        let inner = inner
            .strip_prefix("markdown")
            .or_else(|| inner.strip_prefix("md"))
            .unwrap_or(inner);
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
    fn keeps_a_fence_that_does_not_wrap_the_response() {
        let text = validate_response("Here you go:\n```\n# Objective\nDo it.\n```").unwrap();
        assert!(text.contains("```"));
    }
}
