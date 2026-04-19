pub mod parser;
pub mod types;

pub use types::*;

#[cfg(feature = "wasm")]
mod wasm_api {
    use crate::parser::parse_markdown;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub fn parse_markdown_to_json(text: &str) -> String {
        let doc = parse_markdown(text);
        match serde_json::to_value(&doc) {
            Ok(v) => serde_json::to_string(&serde_json::json!({ "ok": v })).unwrap_or_else(|_| {
                r#"{"error":{"kind":"SerializeError","message":"failed to wrap ok"}}"#.to_string()
            }),
            Err(e) => serde_json::to_string(&serde_json::json!({
                "error": { "kind": "SerializeError", "message": e.to_string() }
            }))
            .unwrap_or_else(|_| {
                r#"{"error":{"kind":"SerializeError","message":"unknown"}}"#.to_string()
            }),
        }
    }

    #[wasm_bindgen]
    pub fn schema_version() -> u32 {
        crate::types::SCHEMA_VERSION
    }
}
