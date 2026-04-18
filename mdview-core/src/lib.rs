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
        serde_json::to_string(&doc).unwrap_or_else(|e| format!(r#"{{"error":"{}"}}"#, e))
    }
}
