//! AI Agent mode — multi-step task execution.
//!
//! Breaks a natural language task into numbered steps,
//! each with a shell command and explanation. Steps are
//! executed one at a time with user review between each.

use crate::backend::Message;
use crate::context::PaneContext;

/// System prompt for agent planning — returns JSON array of steps.
const AGENT_SYSTEM_PROMPT: &str = "\
You are a terminal automation agent. Given a task description and terminal context, \
break the task into a sequence of shell commands. Return ONLY a JSON array where each \
element has \"command\" and \"explanation\" fields. No markdown, no backticks, no \
commentary outside the JSON. Example: \
[{\"command\":\"git pull origin main\",\"explanation\":\"Pull latest changes\"}, \
{\"command\":\"cargo build --release\",\"explanation\":\"Build in release mode\"}]";

/// Status of a single agent step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepStatus {
    Pending,
    Running,
    Completed { exit_code: i32 },
    Failed { exit_code: i32 },
    Skipped,
}

/// A single step in an agent execution plan.
#[derive(Debug, Clone)]
pub struct AgentStep {
    pub number: u32,
    pub command: String,
    pub explanation: String,
    pub status: StepStatus,
    pub output: Option<String>,
}

/// Overall state of an agent session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentState {
    Planning,
    Reviewing,
    Executing,
    StepFailed { exit_code: i32 },
    Completed,
    Aborted,
}

/// A multi-step AI execution plan.
#[derive(Debug)]
pub struct AgentSession {
    pub task: String,
    pub steps: Vec<AgentStep>,
    pub current_step: usize,
    pub state: AgentState,
}

impl AgentSession {
    /// Create a new session from a task and planned steps.
    pub fn new(task: String, steps: Vec<AgentStep>) -> Self {
        let state = if steps.is_empty() {
            AgentState::Completed
        } else {
            AgentState::Reviewing
        };
        Self {
            task,
            steps,
            current_step: 0,
            state,
        }
    }

    /// Get the current step (if any).
    pub fn current(&self) -> Option<&AgentStep> {
        self.steps.get(self.current_step)
    }

    /// Execute the current step (transition to Executing).
    /// Only valid when state is Reviewing.
    pub fn execute_current(&mut self) -> Option<&str> {
        if !matches!(self.state, AgentState::Reviewing) {
            return None;
        }
        if let Some(step) = self.steps.get_mut(self.current_step) {
            step.status = StepStatus::Running;
            self.state = AgentState::Executing;
            Some(&step.command)
        } else {
            None
        }
    }

    /// Mark current step as completed and advance.
    /// Only valid when state is Executing.
    pub fn complete_step(&mut self, exit_code: i32) {
        if !matches!(self.state, AgentState::Executing) {
            return;
        }
        if let Some(step) = self.steps.get_mut(self.current_step) {
            if exit_code == 0 {
                step.status = StepStatus::Completed { exit_code };
                self.current_step += 1;
                if self.current_step >= self.steps.len() {
                    self.state = AgentState::Completed;
                } else {
                    self.state = AgentState::Reviewing;
                }
            } else {
                step.status = StepStatus::Failed { exit_code };
                self.state = AgentState::StepFailed { exit_code };
            }
        }
    }

    /// Skip the current step and advance.
    /// Only valid when state is Reviewing.
    pub fn skip_current(&mut self) {
        if !matches!(self.state, AgentState::Reviewing) {
            return;
        }
        if let Some(step) = self.steps.get_mut(self.current_step) {
            step.status = StepStatus::Skipped;
            self.current_step += 1;
            if self.current_step >= self.steps.len() {
                self.state = AgentState::Completed;
            } else {
                self.state = AgentState::Reviewing;
            }
        }
    }

    /// Abort the entire plan. Valid from any non-terminal state.
    pub fn abort(&mut self) {
        if !self.is_finished() {
            self.state = AgentState::Aborted;
        }
    }

    /// Retry the failed step (reset to Reviewing).
    /// Only valid when state is StepFailed.
    pub fn retry_current(&mut self) {
        if !matches!(self.state, AgentState::StepFailed { .. }) {
            return;
        }
        if let Some(step) = self.steps.get_mut(self.current_step) {
            step.status = StepStatus::Pending;
            self.state = AgentState::Reviewing;
        }
    }

    /// Check if the session is finished (completed or aborted).
    pub fn is_finished(&self) -> bool {
        matches!(self.state, AgentState::Completed | AgentState::Aborted)
    }

    /// Summary of completed/skipped/failed steps.
    pub fn summary(&self) -> String {
        let completed = self.steps.iter().filter(|s| matches!(s.status, StepStatus::Completed { .. })).count();
        let skipped = self.steps.iter().filter(|s| s.status == StepStatus::Skipped).count();
        let failed = self.steps.iter().filter(|s| matches!(s.status, StepStatus::Failed { .. })).count();
        format!("{} completed, {} skipped, {} failed out of {} steps",
            completed, skipped, failed, self.steps.len())
    }
}

/// Build the LLM messages for an agent planning query.
pub fn build_agent_query(task: &str, context: &PaneContext) -> Vec<Message> {
    let context_msg = context.format_for_llm();
    vec![
        Message::system(AGENT_SYSTEM_PROMPT),
        Message::user(format!("{}\n\nTask: {}", context_msg, task)),
    ]
}

/// Parse an LLM response into agent steps.
/// Expects a JSON array of `{"command": "...", "explanation": "..."}` objects.
pub fn parse_steps(response: &str) -> Vec<AgentStep> {
    // Try to extract JSON array from response (may have markdown wrapping)
    let json_str = extract_json_array(response);

    let parsed: Vec<serde_json::Value> = match serde_json::from_str(&json_str) {
        Ok(v) => v,
        Err(e) => {
            log::warn!("Failed to parse agent steps: {}", e);
            return vec![];
        }
    };

    parsed
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let command = v.get("command")?.as_str()?.to_string();
            let explanation = v
                .get("explanation")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();
            Some(AgentStep {
                number: (i + 1) as u32,
                command,
                explanation,
                status: StepStatus::Pending,
                output: None,
            })
        })
        .collect()
}

/// Extract a JSON array from a response that may contain markdown wrapping.
fn extract_json_array(response: &str) -> String {
    let trimmed = response.trim();

    // Already a JSON array
    if trimmed.starts_with('[') {
        return trimmed.to_string();
    }

    // Wrapped in code fence
    if let Some(start) = trimmed.find('[') {
        if let Some(end) = trimmed.rfind(']') {
            return trimmed[start..=end].to_string();
        }
    }

    trimmed.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_steps_valid_json() {
        let response = r#"[{"command":"ls -la","explanation":"List files"},{"command":"pwd","explanation":"Show directory"}]"#;
        let steps = parse_steps(response);
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].command, "ls -la");
        assert_eq!(steps[0].number, 1);
        assert_eq!(steps[1].command, "pwd");
        assert_eq!(steps[1].number, 2);
    }

    #[test]
    fn test_parse_steps_with_markdown_wrapping() {
        let response = "```json\n[{\"command\":\"git status\",\"explanation\":\"Check status\"}]\n```";
        let steps = parse_steps(response);
        assert_eq!(steps.len(), 1);
        assert_eq!(steps[0].command, "git status");
    }

    #[test]
    fn test_parse_steps_invalid_json() {
        let steps = parse_steps("not json at all");
        assert!(steps.is_empty());
    }

    #[test]
    fn test_session_execute_and_complete() {
        let steps = vec![
            AgentStep { number: 1, command: "ls".into(), explanation: "list".into(), status: StepStatus::Pending, output: None },
            AgentStep { number: 2, command: "pwd".into(), explanation: "dir".into(), status: StepStatus::Pending, output: None },
        ];
        let mut session = AgentSession::new("test".into(), steps);

        assert_eq!(session.state, AgentState::Reviewing);
        assert_eq!(session.current_step, 0);

        // Execute step 1
        let cmd = session.execute_current().unwrap().to_string();
        assert_eq!(cmd, "ls");
        assert_eq!(session.state, AgentState::Executing);

        // Complete step 1
        session.complete_step(0);
        assert_eq!(session.current_step, 1);
        assert_eq!(session.state, AgentState::Reviewing);

        // Execute step 2
        session.execute_current();
        session.complete_step(0);
        assert_eq!(session.state, AgentState::Completed);
        assert!(session.is_finished());
    }

    #[test]
    fn test_session_skip() {
        let steps = vec![
            AgentStep { number: 1, command: "ls".into(), explanation: "".into(), status: StepStatus::Pending, output: None },
            AgentStep { number: 2, command: "pwd".into(), explanation: "".into(), status: StepStatus::Pending, output: None },
        ];
        let mut session = AgentSession::new("test".into(), steps);

        session.skip_current();
        assert_eq!(session.current_step, 1);
        assert_eq!(session.steps[0].status, StepStatus::Skipped);
    }

    #[test]
    fn test_session_abort() {
        let steps = vec![
            AgentStep { number: 1, command: "ls".into(), explanation: "".into(), status: StepStatus::Pending, output: None },
        ];
        let mut session = AgentSession::new("test".into(), steps);

        session.abort();
        assert_eq!(session.state, AgentState::Aborted);
        assert!(session.is_finished());
    }

    #[test]
    fn test_session_failure_and_retry() {
        let steps = vec![
            AgentStep { number: 1, command: "false".into(), explanation: "fail".into(), status: StepStatus::Pending, output: None },
        ];
        let mut session = AgentSession::new("test".into(), steps);

        session.execute_current();
        session.complete_step(1); // non-zero exit
        assert!(matches!(session.state, AgentState::StepFailed { exit_code: 1 }));

        session.retry_current();
        assert_eq!(session.state, AgentState::Reviewing);
    }

    #[test]
    fn test_session_summary() {
        let steps = vec![
            AgentStep { number: 1, command: "a".into(), explanation: "".into(), status: StepStatus::Completed { exit_code: 0 }, output: None },
            AgentStep { number: 2, command: "b".into(), explanation: "".into(), status: StepStatus::Skipped, output: None },
            AgentStep { number: 3, command: "c".into(), explanation: "".into(), status: StepStatus::Failed { exit_code: 1 }, output: None },
        ];
        let session = AgentSession::new("test".into(), steps);
        let summary = session.summary();
        assert!(summary.contains("1 completed"));
        assert!(summary.contains("1 skipped"));
        assert!(summary.contains("1 failed"));
    }

    #[test]
    fn test_empty_plan() {
        let session = AgentSession::new("test".into(), vec![]);
        assert_eq!(session.state, AgentState::Completed);
        assert!(session.is_finished());
    }
}
