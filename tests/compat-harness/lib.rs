// Compile the production translators, without Tauri, account access or networking.
#[path = "../../src-tauri/src/relay_translate.rs"]
pub mod relay_translate;
pub mod antigravity;
#[path = "../../src-tauri/src/pool_math.rs"]
pub mod pool_math;
#[path = "../../src-tauri/src/native_metadata.rs"]
pub mod native_metadata;
#[path = "../../src-tauri/src/source_identity.rs"]
pub mod source_identity;

#[cfg(test)]
mod regressions {
    use super::*;
    use serde_json::{json, Value};

    fn chat(input: Value) -> Value {
        let (body, _) = relay_translate::translate_request(&serde_json::to_vec(&json!({"input":input})).unwrap(), "test-model").unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[test]
    fn structured_tool_output_is_not_lost() {
        let body = chat(json!([{"type":"function_call_output", "call_id":"one", "output":{"count":2,"ok":true}}]));
        let content = body["messages"][0]["content"].as_str().unwrap();
        assert_eq!(serde_json::from_str::<Value>(content).unwrap(), json!({"count":2,"ok":true}));
    }

    #[test]
    fn tool_images_follow_all_sibling_results_not_between_them() {
        let body = chat(json!([
            {"type":"function_call","call_id":"one","name":"screen","arguments":"{}"},
            {"type":"function_call","call_id":"two","name":"read","arguments":"{}"},
            {"type":"function_call_output","call_id":"one","output":[{"type":"input_text","text":"screen"},{"type":"input_image","image_url":"data:image/png;base64,YQ=="}]},
            {"type":"function_call_output","call_id":"two","output":"done"},
            {"role":"user","content":"continue"}
        ]));
        let m = body["messages"].as_array().unwrap();
        assert_eq!(m.len(), 5);
        assert_eq!(m[1]["role"], "tool");
        assert_eq!(m[2]["role"], "tool");
        assert_eq!(m[3]["role"], "user");
        assert_eq!(m[3]["content"][1]["image_url"]["url"], "data:image/png;base64,YQ==");
        assert!(m[3]["content"][0]["text"].as_str().unwrap().contains("one"));
        assert!(!body.to_string().contains("_tool_images"));
    }

    fn google(value: Value, model: &str) -> Result<Value, String> {
        let (body, _) = antigravity::translate::responses_to_antigravity(&serde_json::to_vec(&value).unwrap(),model,"synthetic-project")?;
        Ok(serde_json::from_slice(&body).unwrap())
    }

    #[test]
    fn google_tool_image_is_inline_data_not_file_uri() {
        let body = google(json!({"input":[
            {"type":"function_call","call_id":"one","name":"screen","arguments":"{}"},
            {"type":"function_call_output","call_id":"one","output":[{"type":"image","mimeType":"image/png","data":"YQ=="}]}
        ]}), "gemini-3-flash").unwrap();
        let contents = body["request"]["contents"].as_array().unwrap();
        assert!(contents.iter().flat_map(|c| c["parts"].as_array().unwrap()).any(|p| p["inlineData"] == json!({"mimeType":"image/png","data":"YQ=="})));
        assert!(!body.to_string().contains("data:image"));
    }

    #[test]
    fn wrapped_mcp_result_preserves_image_and_structured_content() {
        let body = chat(json!([{"type":"function_call_output","call_id":"one","output":{
            "content":[{"type":"text","text":"result:"},{"type":"image_url","image_url":{"url":"https://example.invalid/image.png","detail":"low"}}],
            "structuredContent":{"answer":42},"isError":false
        }}]));
        assert!(body["messages"][0]["content"].as_str().unwrap().contains("\"answer\":42"));
        assert_eq!(body["messages"][1]["content"][1]["image_url"]["detail"],"low");
    }

    #[test]
    fn gpt_oss_default_effort_is_supported() {
        let model = antigravity::models::model_for_id("gpt-oss-120b-medium").unwrap();
        assert!(model.thinking_levels.contains(&model.default_thinking_level));
        let model = antigravity::models::model_for_id("gpt-oss-120b").unwrap();
        assert!(model.thinking_levels.contains(&model.default_thinking_level));
        google(json!({"input":"hello","reasoning":{"effort":model.default_thinking_level}}), &model.id).unwrap();
    }

    #[test]
    fn output_cap_without_reasoning_and_for_gpt_oss_is_preserved() {
        for (model, reasoning) in [("claude-sonnet-4",Value::Null),("gpt-oss-120b",json!({"effort":"medium"}))] {
            let body = google(json!({"input":"hello","max_output_tokens":2048,"reasoning":reasoning}), model).unwrap();
            assert_eq!(body["request"]["generationConfig"]["maxOutputTokens"],2048);
            assert!(body["request"]["generationConfig"].get("thinkingConfig").is_none());
            assert!(google(json!({"input":"hello","max_output_tokens":0}),model).is_err());
        }
    }
}
