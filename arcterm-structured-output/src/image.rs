//! Inline image rendering via base64-decoded image data.

use termwiz::escape::Action;

/// Render an image block.
///
/// Decodes base64 data, validates the format, and emits terminal actions.
/// For now, renders a placeholder with image metadata. Full image rendering
/// will reuse the iTerm2 `set_image()` path once wired into the terminal
/// state machine.
pub fn render_image(format: &str, data: &str, actions: &mut Vec<Action>) {
    // Validate format
    if format != "png" && format != "jpeg" && format != "jpg" {
        emit_placeholder(
            &format!("[Image: unsupported format '{}']", format),
            actions,
        );
        return;
    }

    // Decode base64
    let decoded = match base64::engine::general_purpose::STANDARD.decode(data) {
        Ok(bytes) => bytes,
        Err(e) => {
            emit_placeholder(&format!("[Image: decode error: {}]", e), actions);
            return;
        }
    };

    // Validate magic bytes
    let valid = match format {
        "png" => decoded.len() >= 8 && &decoded[0..4] == b"\x89PNG",
        "jpeg" | "jpg" => decoded.len() >= 2 && decoded[0] == 0xFF && decoded[1] == 0xD8,
        _ => false,
    };

    if !valid {
        emit_placeholder("[Image: invalid image data]", actions);
        return;
    }

    // TODO: Wire into iTerm2 image path (ITermFileData + set_image)
    // For now, emit a placeholder showing the image was received
    emit_sgr(36, actions); // cyan
    let msg = format!(
        "[Image: {} format, {} bytes decoded successfully]",
        format,
        decoded.len()
    );
    emit_text(&msg, actions);
    emit_sgr_reset(actions);
    actions.push(Action::Print('\n'));
}

fn emit_placeholder(msg: &str, actions: &mut Vec<Action>) {
    emit_sgr(33, actions); // yellow warning
    emit_text(msg, actions);
    emit_sgr_reset(actions);
    actions.push(Action::Print('\n'));
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

use base64::Engine;

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
    fn test_unsupported_format() {
        let mut actions = Vec::new();
        render_image("bmp", "data", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("unsupported format"));
    }

    #[test]
    fn test_invalid_base64() {
        let mut actions = Vec::new();
        render_image("png", "not valid base64!!!", &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("decode error"));
    }

    #[test]
    fn test_invalid_png_magic() {
        // Valid base64 but not a PNG
        let data = base64::engine::general_purpose::STANDARD.encode(b"not a png file at all");
        let mut actions = Vec::new();
        render_image("png", &data, &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("invalid image data"));
    }

    #[test]
    fn test_valid_png_placeholder() {
        // Minimal PNG header
        let mut png_bytes = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        png_bytes.extend_from_slice(&[0u8; 100]); // padding
        let data = base64::engine::general_purpose::STANDARD.encode(&png_bytes);
        let mut actions = Vec::new();
        render_image("png", &data, &mut actions);
        let text = actions_to_text(&actions);
        assert!(text.contains("decoded successfully"));
    }
}
