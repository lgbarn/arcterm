//! OSC 7770 structured content accumulator.
//!
//! Contains a copy of `StructuredContentAccumulator` originally defined in
//! `arcterm-vt/src/handler.rs`, relocated here so that `arcterm-app` owns its
//! protocol surface.  Unlike the original, this version imports `ContentType`
//! from `arcterm_render` rather than `arcterm_vt`, eliminating the need for
//! a conversion bridge at the call site.

use std::collections::HashMap;

use arcterm_render::ContentType;

// ---------------------------------------------------------------------------
// StructuredContentAccumulator
// ---------------------------------------------------------------------------

/// Accumulates characters written inside an OSC 7770 `start` / `end` pair.
///
/// While an accumulator is active, every `put_char` call both writes to the
/// terminal grid (so the content is rendered normally) and appends to
/// `buffer` so the full text is available for structured processing.
#[derive(Debug, Clone)]
pub struct StructuredContentAccumulator {
    /// The semantic type of this content block.
    pub content_type: ContentType,
    /// Key/value attributes parsed from the OSC 7770 params (e.g. `lang=rust`).
    pub attrs: HashMap<String, String>,
    /// Raw text accumulated since the `start` OSC was received.
    pub buffer: String,
}

impl StructuredContentAccumulator {
    /// Create a new, empty accumulator for the given content type and attrs.
    pub fn new(content_type: ContentType, attrs: HashMap<String, String>) -> Self {
        Self {
            content_type,
            attrs,
            buffer: String::new(),
        }
    }
}
