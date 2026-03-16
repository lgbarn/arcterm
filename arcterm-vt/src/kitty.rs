//! Kitty Graphics Protocol payload parser and chunk assembler.
//!
//! The Kitty Graphics Protocol transmits image data via APC escape sequences
//! of the form `ESC _ G<key=val,...>;<base64-payload> ESC \`.
//!
//! This module parses the APC body into a structured `KittyCommand` and
//! assembles multi-chunk transfers via `KittyChunkAssembler`.

use std::collections::HashMap;

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

/// The action requested by a Kitty graphics command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KittyAction {
    /// `a=T` — transmit image data and immediately display it.
    TransmitAndDisplay,
    /// `a=t` — transmit image data only (store without displaying).
    Transmit,
    /// `a=p` — display a previously transmitted image.
    Display,
    /// `a=d` — delete an image or placement.
    Delete,
    /// Any unrecognised action value.
    Unknown,
}

/// The pixel format of the image payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KittyFormat {
    /// `f=100` — PNG-encoded image.
    Png,
    /// `f=24` — raw 24-bit RGB pixels.
    Rgb24,
    /// `f=32` — raw 32-bit RGBA pixels.
    Rgba32,
    /// Any unrecognised format value.
    Unknown,
}

// ---------------------------------------------------------------------------
// KittyCommand
// ---------------------------------------------------------------------------

/// Structured representation of a single Kitty graphics APC command.
///
/// The raw APC body has the form `G<key=val,...>;<base64-payload>`.
/// The `G` prefix is stripped by the caller before passing to
/// [`parse_kitty_command`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KittyCommand {
    /// Requested action.
    pub action: KittyAction,
    /// Pixel format of the image data.
    pub format: KittyFormat,
    /// Image ID (`i=` key).  Defaults to 0.
    pub image_id: u32,
    /// Whether more chunks follow (`m=1`).
    pub more_chunks: bool,
    /// Quiet level (`q=` key).  0 = report errors/OK, 1 = only errors, 2 = silent.
    pub quiet: u8,
    /// Number of columns the image should occupy (`c=` key).
    pub cols: Option<u32>,
    /// Number of rows the image should occupy (`r=` key).
    pub rows: Option<u32>,
    /// Raw base64-encoded payload bytes (ASCII).  May be empty.
    pub payload_base64: Vec<u8>,
}

// ---------------------------------------------------------------------------
// parse_kitty_command
// ---------------------------------------------------------------------------

/// Parse a Kitty graphics APC body into a [`KittyCommand`].
///
/// `raw` is the bytes after the `ESC _` introducer and before the `ESC \`
/// terminator.  The Kitty protocol mandates a `G` prefix on the control data,
/// but this function accepts payloads with or without it for robustness.
///
/// Returns `None` only if the input is so malformed that no meaningful
/// command can be extracted.  In practice it always returns `Some`.
pub fn parse_kitty_command(raw: &[u8]) -> Option<KittyCommand> {
    // Strip optional leading 'G'.
    let raw = raw.strip_prefix(b"G").unwrap_or(raw);

    // Split control-data from base64 payload on the first ';'.
    let (ctrl_bytes, payload_bytes) = match raw.iter().position(|&b| b == b';') {
        Some(pos) => (&raw[..pos], &raw[pos + 1..]),
        None => (raw, &b""[..]),
    };

    // Parse comma-separated key=value pairs from ctrl_bytes.
    let ctrl_str = std::str::from_utf8(ctrl_bytes).unwrap_or("");
    let mut kv: HashMap<&str, &str> = HashMap::new();
    for pair in ctrl_str.split(',') {
        if let Some((k, v)) = pair.split_once('=') {
            kv.insert(k.trim(), v.trim());
        }
    }

    // Map action.
    let action = match kv.get("a").copied().unwrap_or("") {
        "T" => KittyAction::TransmitAndDisplay,
        "t" => KittyAction::Transmit,
        "p" => KittyAction::Display,
        "d" => KittyAction::Delete,
        _ => KittyAction::Unknown,
    };

    // Map format.
    let format = match kv.get("f").copied().unwrap_or("") {
        "100" => KittyFormat::Png,
        "24" => KittyFormat::Rgb24,
        "32" => KittyFormat::Rgba32,
        _ => KittyFormat::Unknown,
    };

    // Parse scalar fields with safe defaults.
    let image_id: u32 = kv.get("i").and_then(|v| v.parse().ok()).unwrap_or(0);
    let more_chunks: bool = kv.get("m").map(|v| *v == "1").unwrap_or(false);
    let quiet: u8 = kv.get("q").and_then(|v| v.parse().ok()).unwrap_or(0);
    let cols: Option<u32> = kv.get("c").and_then(|v| v.parse().ok());
    let rows: Option<u32> = kv.get("r").and_then(|v| v.parse().ok());

    Some(KittyCommand {
        action,
        format,
        image_id,
        more_chunks,
        quiet,
        cols,
        rows,
        payload_base64: payload_bytes.to_vec(),
    })
}

// ---------------------------------------------------------------------------
// KittyChunkAssembler
// ---------------------------------------------------------------------------

/// Assembles multi-chunk Kitty image transfers into a single decoded payload.
///
/// When `m=1` (more chunks), the base64 data is buffered keyed by image ID.
/// When `m=0` (last chunk), all buffered data for that ID is concatenated,
/// decoded from base64, and returned together with the command metadata.
pub struct KittyChunkAssembler {
    /// Accumulated raw base64 bytes per image ID, waiting for the final chunk.
    pending: HashMap<u32, Vec<u8>>,
}

impl KittyChunkAssembler {
    /// Create an empty assembler.
    pub fn new() -> Self {
        Self {
            pending: HashMap::new(),
        }
    }

    /// Feed one chunk to the assembler.
    ///
    /// - If `cmd.more_chunks` is true: buffer the base64 payload and return `None`.
    /// - If `cmd.more_chunks` is false: concatenate all buffered chunks for this
    ///   image ID, decode from base64, clear the buffer, and return
    ///   `Some((command_metadata, decoded_bytes))`.
    ///
    /// `command_metadata` is a clone of `cmd` with `payload_base64` cleared
    /// (the raw bytes are discarded after decoding).
    pub fn receive_chunk(&mut self, cmd: &KittyCommand) -> Option<(KittyCommand, Vec<u8>)> {
        use base64::Engine as _;
        use base64::engine::general_purpose::STANDARD as B64;

        let id = cmd.image_id;

        /// Maximum number of raw base64 bytes buffered per image before the
        /// transfer is abandoned.  64 MiB of base64 decodes to ~48 MiB of
        /// image data, which is already generous for a terminal image protocol.
        const MAX_CHUNK_BUFFER_BYTES: usize = 64 * 1024 * 1024;

        if cmd.more_chunks {
            // Append this chunk's base64 bytes to the pending buffer.
            let buf = self.pending.entry(id).or_default();
            let new_len = buf.len().saturating_add(cmd.payload_base64.len());
            if new_len > MAX_CHUNK_BUFFER_BYTES {
                // Discard this image's accumulated data and log a warning to
                // prevent OOM via crafted escape sequences.
                self.pending.remove(&id);
                log::warn!(
                    "kitty: image id={id} exceeded {MAX_CHUNK_BUFFER_BYTES}-byte cap; discarding"
                );
                return None;
            }
            buf.extend_from_slice(&cmd.payload_base64);
            return None;
        }

        // Final chunk (or single-chunk transfer): concatenate all pending bytes
        // with this chunk's payload, then decode from base64.
        let mut accumulated = self.pending.remove(&id).unwrap_or_default();
        accumulated.extend_from_slice(&cmd.payload_base64);

        let decoded = B64.decode(&accumulated).unwrap_or_default();

        // Return command metadata with the raw base64 cleared.
        let mut meta = cmd.clone();
        meta.payload_base64 = Vec::new();

        Some((meta, decoded))
    }
}

impl Default for KittyChunkAssembler {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Tests (written first per TDD requirement)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine as _;
    use base64::engine::general_purpose::STANDARD as B64;

    // -----------------------------------------------------------------------
    // parse_kitty_command tests
    // -----------------------------------------------------------------------

    #[test]
    fn parse_transmit_and_display_png_quiet1() {
        // `a=T,f=100,q=1;<base64-data>`
        let payload = b"a=T,f=100,q=1;iVBORz";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::TransmitAndDisplay);
        assert_eq!(cmd.format, KittyFormat::Png);
        assert_eq!(cmd.quiet, 1);
        assert_eq!(cmd.image_id, 0);
        assert!(!cmd.more_chunks);
        assert_eq!(cmd.payload_base64, b"iVBORz");
    }

    #[test]
    fn parse_with_g_prefix() {
        // The APC body from the wire includes a leading 'G'.
        let payload = b"Ga=T,f=100,q=1;iVBORz";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::TransmitAndDisplay);
        assert_eq!(cmd.format, KittyFormat::Png);
        assert_eq!(cmd.payload_base64, b"iVBORz");
    }

    #[test]
    fn parse_chunked_first_chunk() {
        // `a=T,f=100,i=42,m=1;chunk1` → image_id=42, more_chunks=true
        let payload = b"a=T,f=100,i=42,m=1;chunk1";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.image_id, 42);
        assert!(cmd.more_chunks);
        assert_eq!(cmd.payload_base64, b"chunk1");
    }

    #[test]
    fn parse_no_semicolon_no_payload() {
        // Control-only with no payload.
        let payload = b"a=t,f=32";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::Transmit);
        assert_eq!(cmd.format, KittyFormat::Rgba32);
        assert!(cmd.payload_base64.is_empty());
    }

    #[test]
    fn parse_unknown_action() {
        let payload = b"a=z,f=100;data";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::Unknown);
    }

    #[test]
    fn parse_delete_action() {
        let payload = b"a=d,d=A;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::Delete);
    }

    #[test]
    fn parse_display_action() {
        let payload = b"a=p,i=7;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.action, KittyAction::Display);
        assert_eq!(cmd.image_id, 7);
    }

    #[test]
    fn parse_cols_rows() {
        let payload = b"a=T,f=100,c=40,r=20;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.cols, Some(40));
        assert_eq!(cmd.rows, Some(20));
    }

    #[test]
    fn parse_rgb24_format() {
        let payload = b"a=t,f=24;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.format, KittyFormat::Rgb24);
    }

    #[test]
    fn parse_rgba32_format() {
        let payload = b"a=t,f=32;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.format, KittyFormat::Rgba32);
    }

    #[test]
    fn parse_unknown_format() {
        let payload = b"a=t,f=99;";
        let cmd = parse_kitty_command(payload).unwrap();
        assert_eq!(cmd.format, KittyFormat::Unknown);
    }

    // -----------------------------------------------------------------------
    // KittyChunkAssembler tests
    // -----------------------------------------------------------------------

    fn b64(data: &[u8]) -> Vec<u8> {
        B64.encode(data).into_bytes()
    }

    fn make_cmd(id: u32, more: bool, payload: Vec<u8>) -> KittyCommand {
        KittyCommand {
            action: KittyAction::TransmitAndDisplay,
            format: KittyFormat::Png,
            image_id: id,
            more_chunks: more,
            quiet: 0,
            cols: None,
            rows: None,
            payload_base64: payload,
        }
    }

    #[test]
    fn single_chunk_m0_returns_immediately() {
        let mut asm = KittyChunkAssembler::new();
        let data = b"hello world";
        let cmd = make_cmd(1, false, b64(data));
        let result = asm.receive_chunk(&cmd);
        assert!(result.is_some());
        let (meta, decoded) = result.unwrap();
        assert_eq!(decoded, data);
        assert_eq!(meta.image_id, 1);
        assert!(meta.payload_base64.is_empty(), "metadata payload_base64 should be cleared");
    }

    #[test]
    fn single_chunk_m1_returns_none() {
        let mut asm = KittyChunkAssembler::new();
        let cmd = make_cmd(2, true, b64(b"partial"));
        let result = asm.receive_chunk(&cmd);
        assert!(result.is_none());
    }

    #[test]
    fn two_chunks_m1_then_m0_assembles_correctly() {
        let mut asm = KittyChunkAssembler::new();
        let part1 = b"hello ";
        let part2 = b"world";
        let full = b"hello world";

        // Encode each part separately, then concatenate the base64 strings
        // to simulate what the terminal protocol sends.
        let b64_part1 = b64(part1);
        let b64_part2 = b64(part2);

        // The protocol concatenates the base64 chunks before decoding.
        let mut combined_b64 = b64_part1.clone();
        combined_b64.extend_from_slice(&b64_part2);

        let cmd1 = make_cmd(3, true, b64_part1);
        let cmd2 = make_cmd(3, false, b64_part2);

        assert!(asm.receive_chunk(&cmd1).is_none());
        let result = asm.receive_chunk(&cmd2);
        assert!(result.is_some());
        let (_, decoded) = result.unwrap();
        // The decoded bytes should equal decoding the concatenated base64 string.
        let expected = B64.decode(&combined_b64).unwrap();
        assert_eq!(decoded, expected);
        // And when data is aligned base64 (multiples of 3 bytes), this equals
        // the concatenated raw bytes. Let's verify with data that IS a multiple of 3.
        let _ = full; // used for documentation
    }

    #[test]
    fn three_chunks_assembles_correctly() {
        let mut asm = KittyChunkAssembler::new();
        // Use data whose base64 concatenation decodes cleanly: each part 3 bytes.
        let part1 = b"abc"; // b64: "YWJj"
        let part2 = b"def"; // b64: "ZGVm"
        let part3 = b"ghi"; // b64: "Z2hp"

        let b64_1 = b64(part1);
        let b64_2 = b64(part2);
        let b64_3 = b64(part3);

        let cmd1 = make_cmd(4, true, b64_1.clone());
        let cmd2 = make_cmd(4, true, b64_2.clone());
        let cmd3 = make_cmd(4, false, b64_3.clone());

        assert!(asm.receive_chunk(&cmd1).is_none());
        assert!(asm.receive_chunk(&cmd2).is_none());
        let result = asm.receive_chunk(&cmd3);
        assert!(result.is_some());
        let (_, decoded) = result.unwrap();

        // Concatenate all base64 strings and decode.
        let mut all_b64 = b64_1;
        all_b64.extend_from_slice(&b64_2);
        all_b64.extend_from_slice(&b64_3);
        let expected = B64.decode(&all_b64).unwrap();
        assert_eq!(decoded, expected);
        // With 3-byte chunks: "abcdefghi"
        assert_eq!(decoded, b"abcdefghi");
    }

    #[test]
    fn different_image_ids_do_not_interfere() {
        let mut asm = KittyChunkAssembler::new();

        // Image 10: two chunks.
        let cmd10a = make_cmd(10, true, b64(b"foo"));
        // Image 11: single chunk.
        let cmd11 = make_cmd(11, false, b64(b"bar"));
        // Image 10: final chunk.
        let cmd10b = make_cmd(10, false, b64(b"baz"));

        assert!(asm.receive_chunk(&cmd10a).is_none());
        let r11 = asm.receive_chunk(&cmd11);
        assert!(r11.is_some());
        let (_, decoded11) = r11.unwrap();
        // bar decodes correctly
        assert_eq!(decoded11, b"bar");

        let r10 = asm.receive_chunk(&cmd10b);
        assert!(r10.is_some());
        let (_, decoded10) = r10.unwrap();
        // "foobaz" in concatenated base64 decodes to "foobaz" (each 3 bytes).
        assert_eq!(decoded10, b"foobaz");
    }

    #[test]
    fn pending_cleared_after_final_chunk() {
        let mut asm = KittyChunkAssembler::new();
        let cmd1 = make_cmd(5, true, b64(b"aaa"));
        let cmd2 = make_cmd(5, false, b64(b"bbb"));
        asm.receive_chunk(&cmd1);
        asm.receive_chunk(&cmd2);
        // After completion, pending for image 5 must be gone.
        assert!(!asm.pending.contains_key(&5));
    }

    // ── Security I2: 64 MB cap on accumulated chunk buffer ────────────────

    /// Sending a chunk that alone exceeds the 64 MB cap is discarded and
    /// returns None (not OOM).
    #[test]
    fn chunk_exceeding_cap_is_discarded() {
        let mut asm = KittyChunkAssembler::new();
        // Construct a payload that is just over 64 MiB of base64 bytes.
        // 64 MiB + 1 byte of raw base64 text (the cap is on the byte count
        // of the base64 buffer, not the decoded output).
        let oversized_payload = vec![b'A'; 64 * 1024 * 1024 + 1];
        let cmd = KittyCommand {
            action: KittyAction::TransmitAndDisplay,
            format: KittyFormat::Png,
            image_id: 99,
            more_chunks: true,
            quiet: 0,
            cols: None,
            rows: None,
            payload_base64: oversized_payload,
        };
        let result = asm.receive_chunk(&cmd);
        assert!(result.is_none(), "oversized chunk must return None");
        // The pending buffer for this image must have been purged.
        assert!(
            !asm.pending.contains_key(&99),
            "oversized image must be removed from pending"
        );
    }

    /// Accumulating chunks that together exceed the cap discards the image.
    #[test]
    fn accumulated_chunks_exceeding_cap_are_discarded() {
        let mut asm = KittyChunkAssembler::new();
        // Each chunk is 32 MiB + 1 byte; two together exceed 64 MiB.
        let large_payload = vec![b'B'; 32 * 1024 * 1024 + 1];

        let cmd1 = KittyCommand {
            action: KittyAction::TransmitAndDisplay,
            format: KittyFormat::Png,
            image_id: 77,
            more_chunks: true,
            quiet: 0,
            cols: None,
            rows: None,
            payload_base64: large_payload.clone(),
        };
        let cmd2 = KittyCommand {
            image_id: 77,
            more_chunks: true,
            payload_base64: large_payload,
            ..cmd1.clone()
        };

        // First chunk is below the cap — buffered.
        let r1 = asm.receive_chunk(&cmd1);
        assert!(r1.is_none());

        // Second chunk pushes the total over the cap — image is discarded.
        let r2 = asm.receive_chunk(&cmd2);
        assert!(r2.is_none());
        assert!(
            !asm.pending.contains_key(&77),
            "image must be purged after cap exceeded"
        );
    }
}
