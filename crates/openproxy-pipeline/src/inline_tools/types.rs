use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

impl ParsedToolCall {
    pub fn to_openai_value(&self) -> Value {
        serde_json::json!({
            "id": self.id,
            "type": "function",
            "function": {
                "name": self.name,
                "arguments": self.arguments,
            }
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExtractedInlineTools {
    pub clean_content: String,
    pub tool_calls: Vec<ParsedToolCall>,
}

impl ExtractedInlineTools {
    #[inline]
    pub fn has_tools(&self) -> bool {
        !self.tool_calls.is_empty()
    }
}

pub fn generate_tool_call_id() -> String {
    let raw = uuid::Uuid::new_v4().simple().to_string();
    let suffix = if raw.len() >= 16 { &raw[..16] } else { &raw };
    format!("call_{suffix}")
}
