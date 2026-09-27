//! Types and constants for SSE streaming response accumulation.

/// Cap on the accumulator's text fields combined. The client stream itself
/// has no wire-size ceiling; this bounds only the retained copy, which
/// under concurrency (50 streams × 256 KiB = 12.8 MiB) would otherwise
/// reach 200 MiB at 4 MiB per stream. Chunks past the cap are dropped and
/// `truncated` is set.
pub const MAX_ACCUMULATED_BYTES: usize = 256 * 1024; // 262,144 bytes

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnthropicToolOpen {
    pub id: String,
    pub name: String,
}

/// Anthropic splits one tool call across three SSE events, so the loop
/// dispatches on this marker instead of the raw payload.
#[derive(Debug, Clone)]
pub enum AnthropicToolEvent {
    /// `content_block_start` with `type: "tool_use"`, carrying `id` and
    /// `name`. Opens a new tool_call entry.
    Open(Box<AnthropicToolOpen>),
    /// `content_block_delta` with `type: "input_json_delta"`, whose
    /// `partial_json` fragment is appended to the in-flight arguments.
    Delta { partial_json: String },
    /// `content_block_stop`.
    Close,
}

/// A single accumulated tool call. For OpenAI, `arguments` is a
/// JSON-encoded string per the OpenAI spec. For Anthropic, the
/// concatenation of `partial_json` fragments.
#[derive(Debug, Clone, Default)]
pub struct AccumulatedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: String,
}

#[derive(serde::Deserialize)]
pub(crate) struct ToolCallProbeOuter<'a> {
    #[serde(borrow)]
    pub choices: Option<Vec<ToolCallProbeChoice<'a>>>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ToolCallProbeChoice<'a> {
    #[serde(borrow)]
    pub delta: Option<ToolCallProbeDelta<'a>>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ToolCallProbeDelta<'a> {
    #[serde(borrow)]
    pub tool_calls: Option<Vec<ToolCallProbe<'a>>>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ToolCallProbe<'a> {
    pub index: Option<usize>,
    #[serde(borrow)]
    pub id: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow)]
    pub function: Option<ToolCallFunctionProbe<'a>>,
}

#[derive(serde::Deserialize)]
pub(crate) struct ToolCallFunctionProbe<'a> {
    #[serde(borrow)]
    pub name: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow)]
    pub arguments: Option<std::borrow::Cow<'a, str>>,
}
