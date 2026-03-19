//! OSC 7770 structured output rendering for ArcTerm.
//!
//! Parses JSON payloads from OSC 7770 escape sequences and converts them
//! into terminal-native actions (SGR-colored text or image cells) that the
//! existing terminal state machine can process natively.

pub mod code;
pub mod diff;
pub mod image;
pub mod json_tree;
pub mod payload;

use termwiz::escape::Action;

/// Maximum payload size in bytes (default 1MB).
/// SECURITY: Larger values risk memory amplification — each payload byte
/// can produce ~6 Action entries in the worst case.
pub const DEFAULT_MAX_PAYLOAD_SIZE: usize = 1024 * 1024;

/// Render an OSC 7770 payload into terminal actions.
///
/// Returns `None` if the payload is malformed or exceeds size limits.
/// Returns `Some(Vec<Action>)` with SGR-colored text actions for
/// code/json/diff types, or image cell actions for image types.
pub fn render(payload_str: &str, max_payload_size: usize) -> Option<Vec<Action>> {
    if payload_str.len() > max_payload_size {
        log::warn!(
            "OSC 7770: payload size {} exceeds limit {}",
            payload_str.len(),
            max_payload_size
        );
        return None;
    }

    let block = payload::parse(payload_str)?;

    // Pre-allocate with a capped estimate to prevent memory amplification
    let estimated_capacity = (payload_str.len() * 2 + 64).min(256 * 1024);
    let mut actions = Vec::with_capacity(estimated_capacity);

    // Render optional title
    if let Some(title) = &block.title {
        render_title(title, &mut actions);
    }

    // Dispatch to type-specific renderer
    match &block.block_type {
        payload::BlockType::Code { language, content } => {
            code::render_code(language, content, &mut actions);
        }
        payload::BlockType::Json { content } => {
            json_tree::render_json(content, &mut actions);
        }
        payload::BlockType::Diff { content } => {
            diff::render_diff(content, &mut actions);
        }
        payload::BlockType::Image { format, data } => {
            image::render_image(format, data, &mut actions);
        }
    }

    // End with a newline
    actions.push(Action::Print('\n'));

    Some(actions)
}

/// Render a bold title line above structured content.
fn render_title(title: &str, actions: &mut Vec<Action>) {
    // Emit SGR bold + title + SGR reset as raw Print characters
    for ch in "\x1b[1m".chars() {
        actions.push(Action::Print(ch));
    }
    for ch in title.chars() {
        actions.push(Action::Print(ch));
    }
    for ch in "\x1b[0m\n".chars() {
        actions.push(Action::Print(ch));
    }
}
