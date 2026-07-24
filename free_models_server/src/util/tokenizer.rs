use std::sync::OnceLock;

fn tiktoken_bpe() -> &'static tiktoken_rs::CoreBPE {
    static BPE: OnceLock<tiktoken_rs::CoreBPE> = OnceLock::new();
    BPE.get_or_init(|| {
        tiktoken_rs::cl100k_base().expect("Failed to initialize cl100k_base tokenizer")
    })
}

pub fn estimate_prompt_tokens(text: &str) -> usize {
    tiktoken_bpe().encode_with_special_tokens(text).len()
}
