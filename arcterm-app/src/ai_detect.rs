//! AI agent detection engine.
//!
//! Identifies common AI coding tools (Claude Code, Codex CLI, Gemini CLI,
//! Aider, etc.) running in terminal panes by inspecting the process name and
//! command-line arguments. Results are cached per-pane with a 5-second TTL,
//! mirroring the `NeovimState` pattern in `neovim.rs`.

use crate::proc::{process_args, process_comm};
use std::time::{Duration, Instant};

/// The TTL for cached AI-detection results (5 seconds per CONTEXT-7.md).
const AI_CACHE_TTL: Duration = Duration::from_secs(5);

// ── AiAgentKind ───────────────────────────────────────────────────────────────

/// The type of AI coding agent detected in a terminal pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AiAgentKind {
    /// Claude Code (Anthropic) — binary name: `claude`.
    ClaudeCode,
    /// Codex CLI (OpenAI) — binary name: `codex`.
    CodexCli,
    /// Gemini CLI (Google) — binary name: `gemini`.
    GeminiCli,
    /// Aider — Python entry point; detected via args ending in `aider`.
    Aider,
    /// Any other recognisable AI tool not covered by the above variants.
    Unknown(String),
}

// ── Detection logic ───────────────────────────────────────────────────────────

/// Match a process name string against known AI binary names.
///
/// This is the pure matching function used by both `detect_ai_agent` (which
/// reads from the OS) and the unit tests (which supply names directly).
pub fn match_ai_name(name: &str) -> Option<AiAgentKind> {
    match name {
        n if n.starts_with("claude") => Some(AiAgentKind::ClaudeCode),
        n if n.starts_with("codex") => Some(AiAgentKind::CodexCli),
        n if n.starts_with("gemini") => Some(AiAgentKind::GeminiCli),
        n if n.starts_with("aider") => Some(AiAgentKind::Aider),
        n if n.starts_with("cursor") => Some(AiAgentKind::Unknown("cursor".to_string())),
        n if n.starts_with("copilot") => Some(AiAgentKind::Unknown("copilot".to_string())),
        _ => None,
    }
}

/// Detect whether the process identified by `pid` is a known AI coding agent.
///
/// 1. Reads the process name via `process_comm`. If the name matches a known
///    AI binary, returns `Some(AiAgentKind)`.
/// 2. If the process name is `python3` (or similar interpreter), falls back to
///    inspecting `process_args`: if `args[0]` ends with `"aider"`, returns
///    `Some(AiAgentKind::Aider)`.
/// 3. Returns `None` on any error or if no known AI agent is detected.
pub fn detect_ai_agent(pid: u32) -> Option<AiAgentKind> {
    let comm = process_comm(pid)?;

    // Direct name match (covers claude, codex, gemini, cursor, copilot).
    if let Some(kind) = match_ai_name(&comm) {
        return Some(kind);
    }

    // Python-based fallback: aider runs as `python3 -m aider` or via a
    // script whose path ends with `aider`.
    if comm.starts_with("python") {
        if let Some(args) = process_args(pid) {
            if let Some(first) = args.first() {
                if first.ends_with("aider") {
                    return Some(AiAgentKind::Aider);
                }
            }
        }
    }

    None
}

// ── AiAgentState — cached detection result ───────────────────────────────────

/// Cached per-pane AI agent detection state.
///
/// Results are valid for `AI_CACHE_TTL` (5 s) to avoid syscall spam.
pub struct AiAgentState {
    /// The detected AI agent kind, or `None` if not an AI tool.
    pub kind: Option<AiAgentKind>,
    /// When this state was last computed.
    pub last_check: Instant,
}

impl AiAgentState {
    /// Detect the AI agent for the process identified by `pid`.
    ///
    /// If `pid` is `None`, returns a state with `kind: None`.
    pub fn check(pid: Option<u32>) -> Self {
        let kind = pid.and_then(detect_ai_agent);
        AiAgentState { kind, last_check: Instant::now() }
    }

    /// Returns `true` if the cached data is still within the TTL window.
    pub fn is_fresh(&self) -> bool {
        self.last_check.elapsed() < AI_CACHE_TTL
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// PID 1 (launchd / init) is not an AI agent.
    #[test]
    fn detect_ai_returns_none_for_pid_1() {
        assert!(
            detect_ai_agent(1).is_none(),
            "PID 1 must not be detected as an AI agent"
        );
    }

    /// AiAgentState::check(None) returns kind: None.
    #[test]
    fn ai_agent_state_check_none_returns_none_kind() {
        let state = AiAgentState::check(None);
        assert!(state.kind.is_none(), "check(None) must return kind: None");
    }

    /// AiAgentState is fresh immediately after creation.
    #[test]
    fn ai_agent_state_is_fresh_after_creation() {
        let state = AiAgentState::check(None);
        assert!(state.is_fresh(), "state must be fresh immediately after check()");
    }

    /// match_ai_name correctly maps "claude" to ClaudeCode.
    #[test]
    fn name_matching_claude() {
        assert_eq!(match_ai_name("claude"), Some(AiAgentKind::ClaudeCode));
        assert_eq!(match_ai_name("claude-code"), Some(AiAgentKind::ClaudeCode));
    }

    /// match_ai_name correctly maps "codex" to CodexCli.
    #[test]
    fn name_matching_codex() {
        assert_eq!(match_ai_name("codex"), Some(AiAgentKind::CodexCli));
    }

    /// match_ai_name correctly maps "gemini" to GeminiCli.
    #[test]
    fn name_matching_gemini() {
        assert_eq!(match_ai_name("gemini"), Some(AiAgentKind::GeminiCli));
    }

    /// match_ai_name correctly maps "aider" to Aider.
    #[test]
    fn name_matching_aider() {
        assert_eq!(match_ai_name("aider"), Some(AiAgentKind::Aider));
    }

    /// match_ai_name correctly maps "cursor" to Unknown("cursor").
    #[test]
    fn name_matching_cursor() {
        assert_eq!(
            match_ai_name("cursor"),
            Some(AiAgentKind::Unknown("cursor".to_string()))
        );
    }

    /// match_ai_name correctly maps "copilot" to Unknown("copilot").
    #[test]
    fn name_matching_copilot() {
        assert_eq!(
            match_ai_name("copilot"),
            Some(AiAgentKind::Unknown("copilot".to_string()))
        );
    }

    /// match_ai_name returns None for an unknown process name.
    #[test]
    fn name_matching_unknown_returns_none() {
        assert!(match_ai_name("bash").is_none());
        assert!(match_ai_name("nvim").is_none());
        assert!(match_ai_name("zsh").is_none());
    }
}
