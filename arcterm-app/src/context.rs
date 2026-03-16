//! Per-pane context model.
//!
//! `PaneContext` captures the AI agent type, last shell command, last exit
//! code, and a ring buffer of recent output lines for each terminal pane.
//! It is populated via OSC 133 shell integration sequences processed in the
//! VT layer and drained into `AppState` after each PTY processing batch.

use crate::ai_detect::AiAgentKind;
use crate::layout::PaneId;
use crate::terminal::Terminal;
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

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
    /// Working directory of the pane at the time of the error, if known.
    pub cwd: Option<PathBuf>,
    /// The pane that produced this error.
    pub source_pane: PaneId,
}

// ── SiblingContext ────────────────────────────────────────────────────────────

/// Snapshot of a sibling pane's state for cross-pane context queries.
#[derive(Debug, Clone)]
pub struct SiblingContext {
    pub pane_id: PaneId,
    pub cwd: Option<PathBuf>,
    pub last_command: Option<String>,
    pub exit_code: Option<i32>,
    pub ai_type: Option<AiAgentKind>,
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
    /// is zero (success).  The `pane_id` and `cwd` are embedded in the result
    /// so the caller does not need to pass them separately at injection time.
    pub fn error_context(&self) -> Option<ErrorContext> {
        self.error_context_for(PaneId(0), None)
    }

    /// Return an `ErrorContext` with an explicit `source_pane` and `cwd`.
    pub fn error_context_for(
        &self,
        pane_id: PaneId,
        cwd: Option<PathBuf>,
    ) -> Option<ErrorContext> {
        let code = self.last_exit_code?;
        if code == 0 {
            return None;
        }
        let command = self.last_command.clone().unwrap_or_default();
        let tail_start = self.output_ring.len().saturating_sub(20);
        let output_tail: Vec<String> =
            self.output_ring.iter().skip(tail_start).cloned().collect();
        Some(ErrorContext {
            command,
            exit_code: code,
            output_tail,
            cwd,
            source_pane: pane_id,
        })
    }
}

// ── OSC 7770 formatting ───────────────────────────────────────────────────────

/// Format an `ErrorContext` as an OSC 7770 structured error block.
///
/// Produces:
/// ```text
/// ESC ] 7770 ; start ; type=error ; source=pane-{id} ; exit_code={code} ST
/// <output lines, one per line>
/// ESC ] 7770 ; end ST
/// ```
pub fn format_error_osc7770(ctx: &ErrorContext) -> Vec<u8> {
    let pane_num = ctx.source_pane.0;
    let header = format!(
        "\x1b]7770;start;type=error;source=pane-{};exit_code={}\x07",
        pane_num, ctx.exit_code
    );
    let mut out = Vec::with_capacity(header.len() + 64 + ctx.output_tail.len() * 80);
    out.extend_from_slice(header.as_bytes());
    for line in &ctx.output_tail {
        out.extend_from_slice(line.as_bytes());
        out.push(b'\n');
    }
    out.extend_from_slice(b"\x1b]7770;end\x07");
    out
}

/// Collect sibling pane context summaries, excluding `exclude`.
///
/// `panes` provides access to each `Terminal` so we can read the CWD via
/// `terminal.cwd()`.  Pane contexts without a matching `Terminal` entry are
/// still included, but their CWD will be `None`.
pub fn collect_sibling_contexts(
    pane_contexts: &HashMap<PaneId, PaneContext>,
    panes: &HashMap<PaneId, Terminal>,
    exclude: PaneId,
) -> Vec<SiblingContext> {
    pane_contexts
        .iter()
        .filter(|&(&id, _)| id != exclude)
        .map(|(&id, ctx)| {
            let cwd = panes.get(&id).and_then(|t| t.cwd());
            SiblingContext {
                pane_id: id,
                cwd,
                last_command: ctx.last_command.clone(),
                exit_code: ctx.last_exit_code,
                ai_type: ctx.ai_type.clone(),
            }
        })
        .collect()
}

/// Format sibling contexts as an OSC 7770 `type=context` JSON response.
///
/// Produces:
/// ```text
/// ESC ] 7770 ; start ; type=context ST
/// [{"pane_id":N,"cwd":"/path","last_command":"cmd","exit_code":0,"ai_type":null}, ...]
/// ESC ] 7770 ; end ST
/// ```
pub fn format_context_osc7770(siblings: &[SiblingContext]) -> Vec<u8> {
    let header = b"\x1b]7770;start;type=context\x07";
    let mut entries = Vec::with_capacity(siblings.len());
    for s in siblings {
        let cwd_json = match &s.cwd {
            Some(p) => format!("\"{}\"", p.display()),
            None => "null".to_string(),
        };
        let cmd_json = match &s.last_command {
            Some(c) => format!("\"{}\"", c.replace('\\', "\\\\").replace('"', "\\\"")),
            None => "null".to_string(),
        };
        let exit_json = match s.exit_code {
            Some(c) => c.to_string(),
            None => "null".to_string(),
        };
        let ai_json = match &s.ai_type {
            Some(k) => format!("\"{}\"", ai_kind_str(k)),
            None => "null".to_string(),
        };
        entries.push(format!(
            "{{\"pane_id\":{},\"cwd\":{},\"last_command\":{},\"exit_code\":{},\"ai_type\":{}}}",
            s.pane_id.0, cwd_json, cmd_json, exit_json, ai_json
        ));
    }
    let json = format!("[{}]", entries.join(","));

    let mut out = Vec::with_capacity(header.len() + json.len() + 32);
    out.extend_from_slice(header);
    out.extend_from_slice(json.as_bytes());
    out.push(b'\n');
    out.extend_from_slice(b"\x1b]7770;end\x07");
    out
}

fn ai_kind_str(kind: &AiAgentKind) -> &str {
    match kind {
        AiAgentKind::ClaudeCode => "claude-code",
        AiAgentKind::CodexCli => "codex-cli",
        AiAgentKind::GeminiCli => "gemini-cli",
        AiAgentKind::Aider => "aider",
        AiAgentKind::Unknown(s) => s.as_str(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helper ───────────────────────────────────────────────────────────────

    fn make_pane_id(n: u64) -> PaneId {
        PaneId(n)
    }

    // ── existing tests (unchanged) ────────────────────────────────────────────

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

    // ── Task 1 new tests ──────────────────────────────────────────────────────

    /// error_context_for embeds the provided pane_id in the result.
    #[test]
    fn context_error_context_for_embeds_pane_id() {
        let mut ctx = PaneContext::new(200);
        ctx.set_command("make".to_string());
        ctx.set_exit_code(2);
        let id = make_pane_id(42);
        let ec = ctx.error_context_for(id, None).expect("should have error context");
        assert_eq!(ec.source_pane.0, 42);
        assert_eq!(ec.exit_code, 2);
        assert_eq!(ec.command, "make");
    }

    /// error_context_for embeds the provided cwd in the result.
    #[test]
    fn context_error_context_for_embeds_cwd() {
        let mut ctx = PaneContext::new(200);
        ctx.set_exit_code(1);
        let id = make_pane_id(7);
        let cwd = PathBuf::from("/tmp/project");
        let ec = ctx
            .error_context_for(id, Some(cwd.clone()))
            .expect("should have error context");
        assert_eq!(ec.cwd.as_deref(), Some(cwd.as_path()));
    }

    /// Ring buffer overflow: push 250 lines into a ring with capacity 200,
    /// verify output_ring.len() == 200 and oldest lines are evicted.
    #[test]
    fn context_ring_buffer_overflow_evicts_oldest() {
        let mut ctx = PaneContext::new(200);
        for i in 0..250u32 {
            ctx.push_output_line(format!("line{i}"));
        }
        assert_eq!(ctx.output_ring.len(), 200);
        // Oldest surviving line is line50 (250 - 200 = 50 evicted).
        assert_eq!(ctx.output_ring[0], "line50");
        assert_eq!(ctx.output_ring[199], "line249");
    }

    /// format_error_osc7770 produces a valid OSC 7770 start/end sequence
    /// with correct type, source, and exit_code attributes.
    #[test]
    fn format_error_osc7770_valid_sequence() {
        let ctx = ErrorContext {
            command: "cargo build".to_string(),
            exit_code: 1,
            output_tail: vec!["error[E0308]: type mismatch".to_string()],
            cwd: None,
            source_pane: make_pane_id(5),
        };
        let bytes = format_error_osc7770(&ctx);
        let s = String::from_utf8(bytes).expect("valid utf8");

        // Must start with OSC 7770 start header.
        assert!(
            s.starts_with("\x1b]7770;start;type=error;source=pane-5;exit_code=1\x07"),
            "unexpected header: {s:?}"
        );
        // Must contain the output line.
        assert!(s.contains("error[E0308]: type mismatch"));
        // Must end with OSC 7770 end sequence.
        assert!(s.ends_with("\x1b]7770;end\x07"), "missing end sequence: {s:?}");
    }

    /// format_error_osc7770 with multiple output lines includes all of them.
    #[test]
    fn format_error_osc7770_multiple_lines() {
        let ctx = ErrorContext {
            command: "make".to_string(),
            exit_code: 2,
            output_tail: vec!["line A".to_string(), "line B".to_string(), "line C".to_string()],
            cwd: None,
            source_pane: make_pane_id(1),
        };
        let bytes = format_error_osc7770(&ctx);
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("line A\n"));
        assert!(s.contains("line B\n"));
        assert!(s.contains("line C\n"));
    }

    // ── Task 2 new tests ──────────────────────────────────────────────────────

    /// collect_sibling_contexts excludes the requesting pane.
    #[test]
    fn collect_sibling_contexts_excludes_requesting_pane() {
        let mut pane_contexts: HashMap<PaneId, PaneContext> = HashMap::new();
        let id_a = make_pane_id(1);
        let id_b = make_pane_id(2);
        let id_c = make_pane_id(3);
        pane_contexts.insert(id_a, PaneContext::new(200));
        pane_contexts.insert(id_b, PaneContext::new(200));
        pane_contexts.insert(id_c, PaneContext::new(200));

        let panes: HashMap<PaneId, Terminal> = HashMap::new();
        let siblings = collect_sibling_contexts(&pane_contexts, &panes, id_b);

        assert_eq!(siblings.len(), 2);
        assert!(!siblings.iter().any(|s| s.pane_id == id_b));
    }

    /// collect_sibling_contexts returns empty vec when only one pane exists.
    #[test]
    fn collect_sibling_contexts_empty_when_only_one_pane() {
        let mut pane_contexts: HashMap<PaneId, PaneContext> = HashMap::new();
        let id = make_pane_id(10);
        pane_contexts.insert(id, PaneContext::new(200));

        let panes: HashMap<PaneId, Terminal> = HashMap::new();
        let siblings = collect_sibling_contexts(&pane_contexts, &panes, id);

        assert!(siblings.is_empty());
    }

    /// format_context_osc7770 produces valid JSON within OSC 7770 delimiters.
    #[test]
    fn format_context_osc7770_valid_json() {
        let siblings = vec![
            SiblingContext {
                pane_id: make_pane_id(1),
                cwd: Some(PathBuf::from("/home/user/project")),
                last_command: Some("cargo test".to_string()),
                exit_code: Some(0),
                ai_type: None,
            },
            SiblingContext {
                pane_id: make_pane_id(2),
                cwd: None,
                last_command: None,
                exit_code: None,
                ai_type: Some(AiAgentKind::ClaudeCode),
            },
        ];

        let bytes = format_context_osc7770(&siblings);
        let s = String::from_utf8(bytes).expect("valid utf8");

        // Must start with OSC 7770 context start.
        assert!(
            s.starts_with("\x1b]7770;start;type=context\x07"),
            "unexpected start: {s:?}"
        );
        // Must end with OSC 7770 end.
        assert!(s.ends_with("\x1b]7770;end\x07"), "missing end: {s:?}");

        // JSON array markers.
        assert!(s.contains('['), "missing JSON array open");
        assert!(s.contains(']'), "missing JSON array close");

        // Spot-check field values.
        assert!(s.contains("\"pane_id\":1"));
        assert!(s.contains("\"/home/user/project\""));
        assert!(s.contains("\"cargo test\""));
        assert!(s.contains("\"exit_code\":0"));
        assert!(s.contains("\"pane_id\":2"));
        assert!(s.contains("\"ai_type\":\"claude-code\""));
    }
}
