//! AI integration layer for ArcTerm.
//!
//! Provides LLM backend abstraction (Ollama, Claude), pane context extraction,
//! system prompts, and destructive command detection.

pub mod backend;
pub mod config;
pub mod context;
pub mod destructive;
pub mod prompts;
