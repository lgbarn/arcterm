//! OSC 7770 JSON payload parsing.

use serde::Deserialize;

/// The type of structured content block.
#[derive(Debug, Clone)]
pub enum BlockType {
    Code { language: String, content: String },
    Json { content: String },
    Diff { content: String },
    Image { format: String, data: String },
}

/// A parsed structured block from an OSC 7770 payload.
#[derive(Debug, Clone)]
pub struct StructuredBlock {
    pub block_type: BlockType,
    pub title: Option<String>,
}

/// Raw JSON payload for deserialization.
#[derive(Deserialize)]
struct RawPayload {
    #[serde(rename = "type")]
    block_type: String,
    title: Option<String>,
    language: Option<String>,
    content: Option<String>,
    format: Option<String>,
    data: Option<String>,
}

/// Parse an OSC 7770 JSON payload into a StructuredBlock.
/// Returns None if the payload is malformed or missing required fields.
pub fn parse(payload_str: &str) -> Option<StructuredBlock> {
    let raw: RawPayload = match serde_json::from_str(payload_str) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("OSC 7770: failed to parse JSON payload: {}", e);
            return None;
        }
    };

    let block_type = match raw.block_type.as_str() {
        "code" => {
            let language = raw.language.unwrap_or_else(|| "plain".to_string());
            let content = raw.content?;
            BlockType::Code { language, content }
        }
        "json" => {
            let content = raw.content?;
            BlockType::Json { content }
        }
        "diff" => {
            let content = raw.content?;
            BlockType::Diff { content }
        }
        "image" => {
            let format = raw.format?;
            let data = raw.data?;
            BlockType::Image { format, data }
        }
        unknown => {
            log::warn!("OSC 7770: unknown block type '{}', ignoring", unknown);
            return None;
        }
    };

    Some(StructuredBlock {
        block_type,
        title: raw.title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_code_block() {
        let payload = r#"{"type":"code","language":"python","content":"print(42)"}"#;
        let block = parse(payload).unwrap();
        assert!(matches!(block.block_type, BlockType::Code { .. }));
        if let BlockType::Code { language, content } = block.block_type {
            assert_eq!(language, "python");
            assert_eq!(content, "print(42)");
        }
    }

    #[test]
    fn test_parse_json_block() {
        let payload = r#"{"type":"json","content":"{\"key\":\"value\"}"}"#;
        let block = parse(payload).unwrap();
        assert!(matches!(block.block_type, BlockType::Json { .. }));
    }

    #[test]
    fn test_parse_diff_block() {
        let payload = r#"{"type":"diff","content":"--- a/f\n+++ b/f"}"#;
        let block = parse(payload).unwrap();
        assert!(matches!(block.block_type, BlockType::Diff { .. }));
    }

    #[test]
    fn test_parse_image_block() {
        let payload = r#"{"type":"image","format":"png","data":"aWVuZA=="}"#;
        let block = parse(payload).unwrap();
        assert!(matches!(block.block_type, BlockType::Image { .. }));
    }

    #[test]
    fn test_parse_with_title() {
        let payload = r#"{"type":"code","language":"rust","title":"example.rs","content":"fn main(){}"}"#;
        let block = parse(payload).unwrap();
        assert_eq!(block.title, Some("example.rs".to_string()));
    }

    #[test]
    fn test_parse_unknown_type() {
        let payload = r#"{"type":"unknown","content":"text"}"#;
        assert!(parse(payload).is_none());
    }

    #[test]
    fn test_parse_invalid_json() {
        assert!(parse("{not valid json}").is_none());
    }

    #[test]
    fn test_parse_missing_content() {
        let payload = r#"{"type":"code","language":"python"}"#;
        assert!(parse(payload).is_none());
    }
}
