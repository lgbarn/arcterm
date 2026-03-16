//! Kitty Graphics Protocol types and chunk assembler.
//!
//! Contains copies of the Kitty-related types originally defined in
//! `arcterm-vt/src/kitty.rs`.  They are relocated here so that `arcterm-app`
//! owns its protocol surface and `arcterm-vt` can eventually be removed from
//! the dependency graph.

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
