//! Integration tests for LLM backends.

use arcterm_ai::backend::{create_backend, Message};
use arcterm_ai::config::AiConfig;
use arcterm_ai::context::PaneContext;
use arcterm_ai::destructive;
use arcterm_ai::prompts;

#[test]
fn test_create_ollama_backend_from_default_config() {
    let config = AiConfig::default();
    let backend = create_backend(&config);
    assert_eq!(backend.name(), "Ollama");
}

#[test]
fn test_create_claude_backend_from_config() {
    let config = AiConfig {
        api_key: Some("sk-ant-test".to_string()),
        model: "claude-sonnet-4-20250514".to_string(),
        ..Default::default()
    };
    let backend = create_backend(&config);
    assert_eq!(backend.name(), "Claude");
}

#[test]
fn test_ollama_unavailable_on_bad_port() {
    let config = AiConfig {
        endpoint: "http://127.0.0.1:19999".to_string(),
        ..Default::default()
    };
    let backend = create_backend(&config);
    assert!(!backend.is_available());
}

#[test]
fn test_message_construction() {
    let sys = Message::system("You are helpful");
    assert_eq!(sys.role, "system");
    assert_eq!(sys.content, "You are helpful");

    let user = Message::user("Hello");
    assert_eq!(user.role, "user");

    let asst = Message::assistant("Hi there");
    assert_eq!(asst.role, "assistant");
}

#[test]
fn test_context_formatting_roundtrip() {
    let ctx = PaneContext {
        scrollback: "$ cargo build\nerror[E0308]: mismatched types".to_string(),
        cwd: "/home/user/project".to_string(),
        foreground_process: Some("cargo".to_string()),
        dimensions: (24, 80),
    };

    let formatted = ctx.format_for_llm();
    assert!(formatted.contains("/home/user/project"));
    assert!(formatted.contains("cargo"));
    assert!(formatted.contains("error[E0308]"));
}

#[test]
fn test_destructive_detection_integration() {
    // Dangerous commands
    assert!(destructive::is_destructive("sudo rm -rf /"));
    assert!(destructive::is_destructive("DROP TABLE users;"));
    assert!(destructive::is_destructive("git push --force origin main"));

    // Safe commands
    assert!(!destructive::is_destructive("ls -la"));
    assert!(!destructive::is_destructive("git status"));
    assert!(!destructive::is_destructive("cargo build --release"));
}

#[test]
fn test_system_prompts_contain_expected_guidance() {
    assert!(prompts::AI_PANE_SYSTEM_PROMPT.contains("terminal assistant"));
    assert!(prompts::AI_PANE_SYSTEM_PROMPT.contains("DESTRUCTIVE"));
    assert!(prompts::COMMAND_OVERLAY_SYSTEM_PROMPT.contains("shell command"));
    assert!(prompts::COMMAND_OVERLAY_SYSTEM_PROMPT.contains("No explanation"));
}

#[test]
fn test_maybe_warn_destructive() {
    let warned = destructive::maybe_warn("rm -rf /tmp/data");
    assert!(warned.starts_with(destructive::WARNING_LABEL));

    let safe = destructive::maybe_warn("echo hello");
    assert_eq!(safe, "echo hello");
}
