use super::super::*;

#[test]
fn anthropic_tool_use_start_emits_id_and_name() {
    // `arguments` is empty at start: the JSON body arrives in the
    // following content_block_delta events.
    let payload = r#"content_block_start
{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_01ABC","name":"get_weather","input":{}}}"#;
    let mut acc: Option<AnthropicToolUseAccumulator> = None;
    let mut counter: u32 = 0;
    let chunk =
        translate_anthropic_sse_event(payload, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap()
            .unwrap();
    assert!(!chunk.done);
    let tool_call = &chunk.payload["choices"][0]["delta"]["tool_calls"][0];
    assert_eq!(tool_call["index"].as_u64().unwrap(), 0);
    assert_eq!(tool_call["id"].as_str().unwrap(), "toolu_01ABC");
    assert_eq!(tool_call["type"].as_str().unwrap(), "function");
    assert_eq!(
        tool_call["function"]["name"].as_str().unwrap(),
        "get_weather"
    );
    assert_eq!(tool_call["function"]["arguments"].as_str().unwrap(), "");
    assert!(acc.is_some());
    assert_eq!(acc.as_ref().unwrap().id, "toolu_01ABC");
    assert_eq!(acc.as_ref().unwrap().name, "get_weather");
    assert_eq!(counter, 1);
    // The pipeline's accumulator reads delta_tool_calls, so the
    // record is mirrored there.
    assert_eq!(chunk.delta_tool_calls.len(), 1);
    assert_eq!(
        chunk.delta_tool_calls[0]["id"].as_str().unwrap(),
        "toolu_01ABC"
    );
    assert_eq!(
        chunk.delta_tool_calls[0]["function"]["name"]
            .as_str()
            .unwrap(),
        "get_weather"
    );
    assert_eq!(
        chunk.delta_tool_calls[0]["function"]["arguments"]
            .as_str()
            .unwrap(),
        ""
    );
    assert!(
        !chunk.has_content,
        "content_block_start (tool_use) must have has_content=false"
    );
}

#[test]
fn anthropic_tool_use_input_json_delta_accumulates() {
    let mut acc = Some(
        AnthropicToolUseAccumulator::new_with_bounds(
            0,
            "toolu_01ABC".to_string(),
            "get_weather".to_string(),
        )
        .unwrap(),
    );
    let mut counter: u32 = 1;

    // Fragment 1: `{"location":`
    let p1 = r#"content_block_delta
{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"location\":"}}"#;
    let chunk1 =
        translate_anthropic_sse_event(p1, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap()
            .unwrap();
    assert!(!chunk1.done);
    assert_eq!(
        chunk1.payload["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap(),
        "{\"location\":"
    );
    assert_eq!(
        chunk1.delta_tool_calls[0]["function"]["arguments"]
            .as_str()
            .unwrap(),
        "{\"location\":"
    );
    assert_eq!(
        chunk1.payload["choices"][0]["delta"]["tool_calls"][0]["index"]
            .as_u64()
            .unwrap(),
        0
    );
    assert!(
        chunk1.has_content,
        "input_json_delta chunk 1 must have has_content=true"
    );

    // Fragment 2: ` "San Francisco"}`
    let p2 = r#"content_block_delta
{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":" \"San Francisco\"}"}}"#;
    let chunk2 =
        translate_anthropic_sse_event(p2, "chunk-2", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap()
            .unwrap();
    assert!(!chunk2.done);
    // Only the new fragment. Sending the running total would
    // duplicate arguments in the downstream accumulator.
    assert_eq!(
        chunk2.payload["choices"][0]["delta"]["tool_calls"][0]["function"]["arguments"]
            .as_str()
            .unwrap(),
        " \"San Francisco\"}"
    );
    assert_eq!(
        chunk2.delta_tool_calls[0]["function"]["arguments"]
            .as_str()
            .unwrap(),
        " \"San Francisco\"}"
    );
    assert!(
        chunk2.has_content,
        "input_json_delta chunk 2 must have has_content=true"
    );

    // The accumulator keeps the full string for validation.
    assert_eq!(
        acc.as_ref().unwrap().arguments,
        "{\"location\": \"San Francisco\"}"
    );
}

#[test]
fn anthropic_tool_use_block_stop_clears_accumulator() {
    let mut acc = Some(
        AnthropicToolUseAccumulator::new_with_bounds(
            0,
            "toolu_01ABC".to_string(),
            "get_weather".to_string(),
        )
        .unwrap(),
    );
    let mut counter: u32 = 1;

    let payload = r#"content_block_stop
{"type":"content_block_stop","index":1}"#;
    let chunk =
        translate_anthropic_sse_event(payload, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap();
    // No chunk here: the downstream pipeline flushes on the next
    // message_delta or at stream end.
    assert!(chunk.is_none());
    assert!(acc.is_none());
}

#[test]
fn anthropic_text_block_passthrough_does_not_open_accumulator() {
    // A text block must not open the tool_use accumulator.
    let start = r#"content_block_start
{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}"#;
    let delta = r#"content_block_delta
{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"hello"}}"#;
    let mut acc: Option<AnthropicToolUseAccumulator> = None;
    let mut counter: u32 = 0;
    let start_chunk =
        translate_anthropic_sse_event(start, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap();
    assert!(start_chunk.is_none());
    assert!(acc.is_none());
    let delta_chunk =
        translate_anthropic_sse_event(delta, "chunk-2", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap()
            .unwrap();
    assert_eq!(
        delta_chunk.payload["choices"][0]["delta"]["content"]
            .as_str()
            .unwrap(),
        "hello"
    );
    assert!(acc.is_none());
}

#[test]
fn anthropic_input_json_delta_without_open_accumulator_is_dropped() {
    // A stream with no prior tool_use content_block_start must neither panic
    // nor emit a phantom chunk.
    let mut acc: Option<AnthropicToolUseAccumulator> = None;
    let mut counter: u32 = 0;

    let payload = "content_block_delta\n{\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{\\\"foo\\\":1}\"}}";
    let chunk =
        translate_anthropic_sse_event(payload, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap();
    assert!(chunk.is_none());
}

#[test]
fn anthropic_message_start_still_works_via_stateful_translator() {
    let mut acc: Option<AnthropicToolUseAccumulator> = None;
    let mut counter: u32 = 0;

    let payload = r#"message_start
{"type":"message","role":"assistant","content":[],"model":"claude-3","stop_reason":null,"usage":{"input_tokens":10,"output_tokens":0}}"#;
    let chunk =
        translate_anthropic_sse_event(payload, "chunk-1", 1000, "claude-3", &mut acc, &mut counter)
            .unwrap()
            .unwrap();
    assert_eq!(
        chunk.payload["choices"][0]["delta"]["role"]
            .as_str()
            .unwrap(),
        "assistant"
    );
}
