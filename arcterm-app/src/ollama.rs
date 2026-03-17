//! Ollama REST API client for local LLM inference.
//!
//! Provides async streaming chat (`/api/chat`) and one-shot generation
//! (`/api/generate`) against a local Ollama instance.

use serde::{Deserialize, Serialize};

// -- Types --

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Request body for `POST /api/chat`.
#[derive(Debug, Serialize)]
pub struct ChatRequest {
    pub model: String,
    pub messages: Vec<ChatMessage>,
    pub stream: bool,
}

/// A single streamed chunk from `/api/chat`.
#[derive(Debug, Deserialize)]
pub struct ChatChunk {
    pub message: Option<ChatMessage>,
    pub done: bool,
}

/// Request body for `POST /api/generate`.
#[derive(Debug, Serialize)]
pub struct GenerateRequest {
    pub model: String,
    pub prompt: String,
    pub system: Option<String>,
    pub stream: bool,
}

/// A single streamed chunk from `/api/generate`.
#[derive(Debug, Deserialize)]
pub struct GenerateChunk {
    pub response: String,
    pub done: bool,
}

/// Ollama client wrapping a reqwest HTTP client.
pub struct OllamaClient {
    pub endpoint: String,
    pub model: String,
    http: reqwest::Client,
}

impl OllamaClient {
    /// Create a new client pointing at the given Ollama endpoint.
    pub fn new(endpoint: String, model: String) -> Self {
        Self {
            endpoint,
            model,
            http: reqwest::Client::new(),
        }
    }

    /// Build the full URL for a given API path (e.g. "/api/chat").
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.endpoint.trim_end_matches('/'), path)
    }

    /// Send a chat request and return the response for streaming.
    ///
    /// Caller should read the response body line-by-line, deserializing
    /// each line as a `ChatChunk`.
    pub async fn chat(
        &self,
        messages: Vec<ChatMessage>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let req = ChatRequest {
            model: self.model.clone(),
            messages,
            stream: true,
        };
        self.http
            .post(&self.url("/api/chat"))
            .json(&req)
            .send()
            .await
    }

    /// Send a one-shot generate request (no conversation history).
    ///
    /// Used by the command overlay for single-response queries.
    pub async fn generate(
        &self,
        prompt: &str,
        system: Option<&str>,
    ) -> Result<reqwest::Response, reqwest::Error> {
        let req = GenerateRequest {
            model: self.model.clone(),
            prompt: prompt.to_string(),
            system: system.map(|s| s.to_string()),
            stream: false,
        };
        self.http
            .post(&self.url("/api/generate"))
            .json(&req)
            .send()
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_message_serializes() {
        let msg = ChatMessage {
            role: "user".to_string(),
            content: "hello".to_string(),
        };
        let json = serde_json::to_string(&msg).unwrap();
        assert!(json.contains("\"role\":\"user\""));
        assert!(json.contains("\"content\":\"hello\""));
    }

    #[test]
    fn chat_request_serializes_with_stream() {
        let req = ChatRequest {
            model: "qwen2.5-coder:7b".to_string(),
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: "list files".to_string(),
            }],
            stream: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":true"));
        assert!(json.contains("\"model\":\"qwen2.5-coder:7b\""));
    }

    #[test]
    fn chat_chunk_deserializes() {
        let json = r#"{"message":{"role":"assistant","content":"hi"},"done":false}"#;
        let chunk: ChatChunk = serde_json::from_str(json).unwrap();
        assert!(!chunk.done);
        let msg = chunk.message.unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(msg.content, "hi");
    }

    #[test]
    fn chat_chunk_done_deserializes() {
        let json = r#"{"message":{"role":"assistant","content":""},"done":true}"#;
        let chunk: ChatChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
    }

    #[test]
    fn generate_request_serializes() {
        let req = GenerateRequest {
            model: "qwen2.5-coder:7b".to_string(),
            prompt: "how do I list pods".to_string(),
            system: Some("Return only a shell command.".to_string()),
            stream: false,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"stream\":false"));
        assert!(json.contains("\"system\":\"Return only a shell command.\""));
    }

    #[test]
    fn generate_chunk_deserializes() {
        let json = r#"{"response":"kubectl get pods","done":true}"#;
        let chunk: GenerateChunk = serde_json::from_str(json).unwrap();
        assert!(chunk.done);
        assert_eq!(chunk.response, "kubectl get pods");
    }

    #[test]
    fn client_url_building() {
        let client = OllamaClient::new(
            "http://localhost:11434".to_string(),
            "test".to_string(),
        );
        assert_eq!(client.url("/api/chat"), "http://localhost:11434/api/chat");
    }

    #[test]
    fn client_url_strips_trailing_slash() {
        let client = OllamaClient::new(
            "http://localhost:11434/".to_string(),
            "test".to_string(),
        );
        assert_eq!(client.url("/api/chat"), "http://localhost:11434/api/chat");
    }
}
