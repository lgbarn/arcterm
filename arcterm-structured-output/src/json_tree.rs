//! JSON tree rendering with color-coded keys and values.

use termwiz::escape::Action;

/// Render a JSON string as a colored tree.
///
/// Keys are cyan, strings green, numbers yellow, booleans magenta, null red.
/// Falls back to plain text if the JSON is invalid.
pub fn render_json(content: &str, actions: &mut Vec<Action>) {
    let value: serde_json::Value = match serde_json::from_str(content) {
        Ok(v) => v,
        Err(_) => {
            // Invalid JSON — render as plain text
            for ch in content.chars() {
                actions.push(Action::Print(ch));
            }
            actions.push(Action::Print('\n'));
            return;
        }
    };

    render_value(&value, 0, true, actions);
    emit_sgr_reset(actions);
}

fn render_value(
    value: &serde_json::Value,
    indent: usize,
    is_last: bool,
    actions: &mut Vec<Action>,
) {
    match value {
        serde_json::Value::Object(map) => {
            emit_text("{\n", actions);
            let len = map.len();
            for (i, (key, val)) in map.iter().enumerate() {
                emit_indent(indent + 1, actions);
                // Key in cyan
                emit_sgr(36, actions);
                emit_text(&format!("\"{}\"", key), actions);
                emit_sgr_reset(actions);
                emit_text(": ", actions);
                if should_collapse(val, indent + 1) {
                    render_collapsed(val, actions);
                } else {
                    render_value(val, indent + 1, i == len - 1, actions);
                }
                if i < len - 1 {
                    emit_text(",", actions);
                }
                emit_text("\n", actions);
            }
            emit_indent(indent, actions);
            emit_text("}", actions);
        }
        serde_json::Value::Array(arr) => {
            emit_text("[\n", actions);
            let len = arr.len();
            for (i, val) in arr.iter().enumerate() {
                emit_indent(indent + 1, actions);
                if should_collapse(val, indent + 1) {
                    render_collapsed(val, actions);
                } else {
                    render_value(val, indent + 1, i == len - 1, actions);
                }
                if i < len - 1 {
                    emit_text(",", actions);
                }
                emit_text("\n", actions);
            }
            emit_indent(indent, actions);
            emit_text("]", actions);
        }
        serde_json::Value::String(s) => {
            emit_sgr(32, actions); // green
            emit_text(&format!("\"{}\"", s), actions);
            emit_sgr_reset(actions);
        }
        serde_json::Value::Number(n) => {
            emit_sgr(33, actions); // yellow
            emit_text(&n.to_string(), actions);
            emit_sgr_reset(actions);
        }
        serde_json::Value::Bool(b) => {
            emit_sgr(35, actions); // magenta
            emit_text(&b.to_string(), actions);
            emit_sgr_reset(actions);
        }
        serde_json::Value::Null => {
            emit_sgr(31, actions); // red
            emit_text("null", actions);
            emit_sgr_reset(actions);
        }
    }
}

/// Collapse nested structures beyond depth 1.
fn should_collapse(value: &serde_json::Value, depth: usize) -> bool {
    if depth <= 1 {
        return false;
    }
    matches!(
        value,
        serde_json::Value::Object(_) | serde_json::Value::Array(_)
    )
}

/// Render a collapsed node marker.
fn render_collapsed(value: &serde_json::Value, actions: &mut Vec<Action>) {
    emit_sgr(90, actions); // dim gray
    match value {
        serde_json::Value::Object(m) => {
            emit_text(&format!("▶ {{...}} ({} keys)", m.len()), actions);
        }
        serde_json::Value::Array(a) => {
            emit_text(&format!("▶ [...] ({} items)", a.len()), actions);
        }
        _ => {}
    }
    emit_sgr_reset(actions);
}

fn emit_indent(level: usize, actions: &mut Vec<Action>) {
    for _ in 0..level * 2 {
        actions.push(Action::Print(' '));
    }
}

fn emit_text(text: &str, actions: &mut Vec<Action>) {
    for ch in text.chars() {
        actions.push(Action::Print(ch));
    }
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

    #[test]
    fn test_render_simple_json() {
        let mut actions = Vec::new();
        render_json(r#"{"name":"ArcTerm","version":1}"#, &mut actions);
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(text.contains("\"name\""));
        assert!(text.contains("\"ArcTerm\""));
        assert!(text.contains("1"));
    }

    #[test]
    fn test_render_invalid_json_fallback() {
        let mut actions = Vec::new();
        render_json("not json at all", &mut actions);
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(text.contains("not json at all"));
    }

    #[test]
    fn test_deep_nesting_collapses() {
        let mut actions = Vec::new();
        render_json(r#"{"a":{"b":{"c":"deep"}}}"#, &mut actions);
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect();
        // Depth 2+ should be collapsed
        assert!(text.contains("▶"));
    }
}
