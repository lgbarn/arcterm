//! Syntax-highlighted code block rendering via syntect.

use syntect::highlighting::{Style, ThemeSet};
use syntect::parsing::SyntaxSet;
use termwiz::escape::Action;

lazy_static::lazy_static! {
    static ref SYNTAX_SET: SyntaxSet = SyntaxSet::load_defaults_newlines();
    static ref THEME_SET: ThemeSet = ThemeSet::load_defaults();
}

/// Render a code block with syntax highlighting.
///
/// Produces `Action::Print` characters with inline SGR color escapes.
/// If the language is not recognized, renders as plain text.
pub fn render_code(language: &str, content: &str, actions: &mut Vec<Action>) {
    let syntax = SYNTAX_SET
        .find_syntax_by_token(language)
        .or_else(|| SYNTAX_SET.find_syntax_by_extension(language))
        .unwrap_or_else(|| SYNTAX_SET.find_syntax_plain_text());

    let theme = &THEME_SET.themes["base16-ocean.dark"];
    let mut highlighter = syntect::highlighting::Highlighter::new(theme);
    let mut highlight_state = syntect::highlighting::HighlightState::new(
        &highlighter,
        syntect::parsing::ScopeStack::new(),
    );
    let mut parse_state = syntect::parsing::ParseState::new(syntax);

    for line in syntect::util::LinesWithEndings::from(content) {
        let ops = parse_state
            .parse_line(line, &SYNTAX_SET)
            .unwrap_or_default();
        let regions = syntect::highlighting::HighlightIterator::new(
            &mut highlight_state,
            &ops,
            line,
            &highlighter,
        );

        for (style, text) in regions {
            emit_styled_text(style, text, actions);
        }
    }

    // Reset colors at the end
    emit_sgr_reset(actions);
}

/// Emit SGR escape + text characters for a styled region.
fn emit_styled_text(style: Style, text: &str, actions: &mut Vec<Action>) {
    let fg = style.foreground;
    // Emit SGR 38;2;r;g;b for 24-bit foreground color
    let sgr = format!("\x1b[38;2;{};{};{}m", fg.r, fg.g, fg.b);
    for ch in sgr.chars() {
        actions.push(Action::Print(ch));
    }
    for ch in text.chars() {
        actions.push(Action::Print(ch));
    }
}

/// Emit SGR 0 (reset all attributes).
fn emit_sgr_reset(actions: &mut Vec<Action>) {
    for ch in "\x1b[0m".chars() {
        actions.push(Action::Print(ch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_python_produces_actions() {
        let mut actions = Vec::new();
        render_code("python", "def hello():\n    pass\n", &mut actions);
        // Should produce a non-empty list of Print actions
        assert!(!actions.is_empty());
        // Should contain SGR escape sequences (color codes)
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(text.contains("\x1b[38;2;"));
    }

    #[test]
    fn test_render_unknown_language_plain() {
        let mut actions = Vec::new();
        render_code("nonexistent_language_xyz", "some text\n", &mut actions);
        // Should still produce actions (plain text rendering)
        assert!(!actions.is_empty());
    }

    #[test]
    fn test_render_empty_content() {
        let mut actions = Vec::new();
        render_code("python", "", &mut actions);
        // Should produce minimal actions (just the reset)
        let text: String = actions
            .iter()
            .filter_map(|a| match a {
                Action::Print(c) => Some(*c),
                _ => None,
            })
            .collect();
        assert!(text.contains("\x1b[0m"));
    }
}
