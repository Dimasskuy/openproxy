use super::types::ParsedToolCall;

/// A syntax-specific inline tool call parser (MiniMax XML, Hermes JSON).
pub trait InlineToolParser: Send + Sync {
    /// `Some` when the block matches this parser's syntax and yields at
    /// least one valid tool call, `None` when the syntax is not recognized.
    fn parse_block(&self, block: &str) -> Option<Vec<ParsedToolCall>>;
}
