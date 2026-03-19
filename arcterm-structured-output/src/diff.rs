//! Unified diff rendering with colored additions and deletions.

use termwiz::escape::Action;

/// Render a unified diff with colored output.
///
/// Additions in green (SGR 32), deletions in red (SGR 31),
/// hunk headers in cyan (SGR 36), file headers in bold.
pub fn render_diff(content: &str, actions: &mut Vec<Action>) {
    for line in content.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            // File headers — bold
            emit_sgr(1, actions);
            emit_line(line, actions);
            emit_sgr_reset(actions);
        } else if line.starts_with("@@") {
            // Hunk headers — cyan
            emit_sgr(36, actions);
            emit_line(line, actions);
            emit_sgr_reset(actions);
        } else if line.starts_with('+') {
            // Additions — green
            emit_sgr(32, actions);
            emit_line(line, actions);
            emit_sgr_reset(actions);
        } else if line.starts_with('-') {
            // Deletions — red
            emit_sgr(31, actions);
            emit_line(line, actions);
            emit_sgr_reset(actions);
        } else if line.starts_with("Binary files") {
            // Binary file marker — yellow
            emit_sgr(33, actions);
            emit_line(line, actions);
            emit_sgr_reset(actions);
        } else {
            // Context lines — default color
            emit_line(line, actions);
        }
    }
}

fn emit_line(text: &str, actions: &mut Vec<Action>) {
    for ch in text.chars() {
        actions.push(Action::Print(ch));
    }
    actions.push(Action::Print('\n'));
}

fn emit_sgr(code: u8, actions: &mut Vec<Action>) {
    let seq = format!("\x1b[{}m", code);
    for ch in seq.chars() {
        actions.push(Action::Print(ch));
    }
}

fn emit_sgr_reset(actions: &mut Vec<Action>) {
    for ch in "\x1b[0m".chars() {
        actions.push(Action::Print(ch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn actions_to_text(actions: &[Action]) -> String {
        actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn test_additions_are_green() {
        let mut actions = Vec::new();
        render_diff("+added line", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("\x1b[32m")); // green SGR
        assert!(text.contains("added line"));
    }

    #[test]
    fn test_deletions_are_red() {
        let mut actions = Vec::new();
        render_diff("-removed line", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("\x1b[31m")); // red SGR
        assert!(text.contains("removed line"));
    }

    #[test]
    fn test_hunk_headers_are_cyan() {
        let mut actions = Vec::new();
        render_diff("@@ -1,3 +1,3 @@", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("\x1b[36m")); // cyan SGR
    }

    #[test]
    fn test_file_headers_are_bold() {
        let mut actions = Vec::new();
        render_diff("--- a/file.txt", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("\x1b[1m")); // bold SGR
    }

    #[test]
    fn test_binary_marker() {
        let mut actions = Vec::new();
        render_diff("Binary files a/img.png and b/img.png differ", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("\x1b[33m")); // yellow SGR
        assert!(text.contains("Binary files"));
    }

    #[test]
    fn test_context_lines_no_color() {
        let mut actions = Vec::new();
        render_diff(" unchanged line", &mut actions);
        let text = actions_to_text(&actions);
        // Context lines should not have SGR codes
        assert!(!text.contains("\x1b[3"));
        assert!(text.contains("unchanged line"));
    }
}
