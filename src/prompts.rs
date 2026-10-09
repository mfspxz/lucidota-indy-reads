//! Prompt templates (minijinja).
//!
//! SECURITY INVARIANT: template *sources* ship in the binary as `const`
//! string literals below. LLM output, evidence content, and user utterances
//! are template *data* — they flow through rendering, never into template
//! source. Rendering a template authored from untrusted content would be
//! server-side template injection; this module makes that unrepresentable
//! by construction (there is no API here that accepts a template string).

use minijinja::{context, Environment};
use std::sync::OnceLock;

const USER_TURN_TMPL: &str = "Governor epistemic hint for this turn: {{ hint }}. The user said: {{ utterance }}";

const SYSTEM_WRAPPER_TMPL: &str = r#"{{ persona }}

---
Contract: reply with a single JSON object and nothing else. The object must
match this shape: {"intent": "...", "claim": null|string, "hypothesis": null|string,
"requested_action": null|string, "response_style": null|string, "reply": "..."}.
Prose outside the JSON object is a contract violation."#;

static ENV: OnceLock<Environment<'static>> = OnceLock::new();

fn env() -> &'static Environment<'static> {
    ENV.get_or_init(|| {
        let mut env = Environment::new();
        env.add_template("user_turn", USER_TURN_TMPL)
            .expect("in-binary template cannot fail to parse");
        env.add_template("system_wrapper", SYSTEM_WRAPPER_TMPL)
            .expect("in-binary template cannot fail to parse");
        env
    })
}

/// Render the user message for this turn: the governor's epistemic hint
/// plus the raw utterance. Both are data, never template source.
pub fn render_user_turn(hint: &str, utterance: &str) -> String {
    env()
        .get_template("user_turn")
        .expect("registered above")
        .render(context! { hint => hint, utterance => utterance })
        .expect("context values are strings; cannot fail")
}

/// Wrap a persona's system prompt with the JSON-envelope contract.
pub fn render_system_prompt(persona_prompt: &str) -> String {
    env()
        .get_template("system_wrapper")
        .expect("registered above")
        .render(context! { persona => persona_prompt })
        .expect("context values are strings; cannot fail")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_turn_renders_both_fields() {
        let out = render_user_turn("trust nothing", "hello?");
        assert!(out.contains("trust nothing"));
        assert!(out.contains("hello?"));
    }

    #[test]
    fn user_input_is_data_not_template() {
        // Jinja syntax inside user input must NOT be evaluated.
        let out = render_user_turn("{{ 7*7 }}", "{% if true %}x{% endif %}");
        assert!(out.contains("{{ 7*7 }}"));
        assert!(out.contains("{% if true %}"));
        assert!(!out.contains("49"));
    }

    #[test]
    fn system_wrapper_keeps_persona_and_adds_contract() {
        let out = render_system_prompt("You are Indy.");
        assert!(out.contains("You are Indy."));
        assert!(out.contains("JSON object"));
    }
}
