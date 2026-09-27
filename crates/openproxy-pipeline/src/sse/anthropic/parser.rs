//! Byte and line parser for Anthropic SSE stream lines.

use super::super::MAX_SSE_EVENT_TYPE_BYTES;
use openproxy_types::error::Result;

/// Parse one Anthropic SSE stream line, tracking the current event type
/// across calls.
///
/// Returns `Ok(Some("event_type\ndata_payload"))` on a `data:` line and
/// `Ok(None)` for every other line, including `event:` lines.
pub fn parse_anthropic_sse_stream_line(
    line: &str,
    current_event: &mut Option<String>,
) -> Result<Option<String>> {
    let line = line.trim_end_matches('\r');

    if line.is_empty() {
        *current_event = None;
        return Ok(None);
    }

    if let Some(event_type) = line.strip_prefix("event: ") {
        let event_type = event_type.trim();
        if event_type.len() > MAX_SSE_EVENT_TYPE_BYTES {
            tracing::warn!(
                actual_len = event_type.len(),
                max = MAX_SSE_EVENT_TYPE_BYTES,
                "SSE event type exceeds maximum length — truncating"
            );
            // Truncating keeps the stream alive.
            *current_event = None;
            return Ok(None);
        }
        *current_event = Some(event_type.to_string());
        return Ok(None);
    }

    if let Some(data) = line.strip_prefix("data: ") {
        let event_type = current_event.as_deref().unwrap_or("unknown");
        return Ok(Some(format!("{event_type}\n{data}")));
    }

    // id:, retry: and comment lines carry no event.
    Ok(None)
}
