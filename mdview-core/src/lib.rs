pub mod types;
pub mod parser;

pub use types::*;

#[cfg(feature = "wasm")]
mod wasm_api {
    use wasm_bindgen::prelude::*;
    use crate::parser::parse_markdown;

    #[wasm_bindgen]
    pub fn parse_markdown_to_json(text: &str) -> String {
        let doc = parse_markdown(text);
        serde_json::to_string(&doc).unwrap_or_else(|e| {
            format!(r#"{{"error":"{}"}}"#, e)
        })
    }
}
