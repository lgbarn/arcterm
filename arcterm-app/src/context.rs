//! Per-pane context model.
//!
//! `PaneContext` captures the AI agent type, last shell command, last exit
//! code, and a ring buffer of recent output lines for each terminal pane.
//! It is populated via OSC 133 shell integration sequences processed in the
//! VT layer and drained into `AppState` after each PTY processing batch.

use crate::ai_detect::AiAgentKind;
use std::collections::VecDeque;

// ── ErrorContext ──────────────────────────────────────────────────────────────

/// Snapshot of context useful for diagnosing a non-zero exit code.
#[derive(Debug, Clone)]
pub struct ErrorContext {
    /// The command that produced the error.
    pub command: String,
    /// The exit code returned by the command.
    pub exit_code: i32,
    /// Up to 20 lines of output immediately preceding the exit.
    pub output_tail: Vec<String>,
}

// ── PaneContext ───────────────────────────────────────────────────────────────

/// Per-pane context: AI agent type, last shell command, exit code, and output.
pub struct PaneContext {
    /// The AI agent running in this pane, if any.
    pub ai_type: Option<AiAgentKind>,
    /// The last command entered at the shell prompt (via OSC 133 B).
    pub last_command: Option<String>,
    /// The exit code of the last command (via OSC 133 D).
    pub last_exit_code: Option<i32>,
    /// Ring buffer of recent output lines.
    pub output_ring: VecDeque<String>,
    /// Maximum number of lines retained in `output_ring`.
    pub ring_capacity: usize,
}

impl PaneContext {
    /// Create a new `PaneContext` with the given ring-buffer capacity.
    pub fn new(capacity: usize) -> Self {
        PaneContext {
            ai_type: None,
            last_command: None,
            last_exit_code: None,
            output_ring: VecDeque::with_capacity(capacity),
            ring_capacity: capacity,
        }
    }

    /// Push a line into the output ring buffer, evicting the oldest when full.
    pub fn push_output_line(&mut self, line: String) {
        if self.output_ring.len() >= self.ring_capacity {
            self.output_ring.pop_front();
        }
        self.output_ring.push_back(line);
    }

    /// Record the command entered at the shell prompt.
    pub fn set_command(&mut self, cmd: String) {
        self.last_command = Some(cmd);
    }

    /// Record the exit code of the last completed command.
    pub fn set_exit_code(&mut self, code: i32) {
        self.last_exit_code = Some(code);
    }

    /// Return an `ErrorContext` when the last exit code is non-zero.
    ///
    /// Returns `None` when no exit code has been recorded or when the code
    /// is zero (success).
    pub fn error_context(&self) -> Option<ErrorContext> {
        let code = self.last_exit_code?;
        if code == 0 {
            return None;
        }
        let command = self.last_command.clone().unwrap_or_default();
        let tail_start = self.output_ring.len().saturating_sub(20);
        let output_tail: Vec<String> = self.output_ring.iter().skip(tail_start).cloned().collect();
        Some(ErrorContext { command, exit_code: code, output_tail })
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// A new PaneContext has an empty ring buffer.
    #[test]
    fn context_new_empty() {
        let ctx = PaneContext::new(200);
        assert!(ctx.output_ring.is_empty());
        assert_eq!(ctx.ring_capacity, 200);
        assert!(ctx.last_command.is_none());
        assert!(ctx.last_exit_code.is_none());
    }

    /// push_output_line appends lines up to capacity.
    #[test]
    fn context_ring_buffer_fills() {
        let mut ctx = PaneContext::new(3);
        ctx.push_output_line("a".to_string());
        ctx.push_output_line("b".to_string());
        ctx.push_output_line("c".to_string());
        assert_eq!(ctx.output_ring.len(), 3);
        assert_eq!(ctx.output_ring[0], "a");
        assert_eq!(ctx.output_ring[2], "c");
    }

    /// push_output_line evicts the oldest line when the ring is full.
    #[test]
    fn context_ring_buffer_evicts_oldest() {
        let mut ctx = PaneContext::new(3);
        ctx.push_output_line("a".to_string());
        ctx.push_output_line("b".to_string());
        ctx.push_output_line("c".to_string());
        ctx.push_output_line("d".to_string()); // "a" should be evicted
        assert_eq!(ctx.output_ring.len(), 3);
        assert_eq!(ctx.output_ring[0], "b");
        assert_eq!(ctx.output_ring[2], "d");
    }

    /// error_context returns None when exit code is zero.
    #[test]
    fn context_error_context_none_on_success() {
        let mut ctx = PaneContext::new(200);
        ctx.set_command("ls".to_string());
        ctx.set_exit_code(0);
        assert!(ctx.error_context().is_none());
    }

    /// error_context returns None when no exit code has been recorded.
    #[test]
    fn context_error_context_none_when_no_exit_code() {
        let mut ctx = PaneContext::new(200);
        ctx.set_command("ls".to_string());
        assert!(ctx.error_context().is_none());
    }

    /// error_context returns Some with last command and exit code on failure.
    #[test]
    fn context_error_context_some_on_failure() {
        let mut ctx = PaneContext::new(200);
        ctx.set_command("bad-cmd".to_string());
        ctx.set_exit_code(127);
        for i in 0..5 {
            ctx.push_output_line(format!("line{i}"));
        }
        let ec = ctx.error_context().expect("should have error context");
        assert_eq!(ec.exit_code, 127);
        assert_eq!(ec.command, "bad-cmd");
        assert_eq!(ec.output_tail.len(), 5);
    }

    /// error_context output_tail is capped at 20 lines.
    #[test]
    fn context_error_context_tail_capped_at_20() {
        let mut ctx = PaneContext::new(200);
        ctx.set_exit_code(1);
        for i in 0..25 {
            ctx.push_output_line(format!("line{i}"));
        }
        let ec = ctx.error_context().expect("should have error context");
        assert_eq!(ec.output_tail.len(), 20);
        // The tail should contain the last 20 lines (line5..line24).
        assert_eq!(ec.output_tail[0], "line5");
        assert_eq!(ec.output_tail[19], "line24");
    }
}
