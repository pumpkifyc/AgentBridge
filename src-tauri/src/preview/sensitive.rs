use regex::Regex;
use std::sync::LazyLock;

use super::schema::PreviewBlock;

static CREDENTIAL_PATTERNS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"(?i)\b(?:sk-(?:proj|live|test)-[A-Za-z0-9_-]{8,}|gh[pousr]_[A-Za-z0-9]{8,}|github_pat_[A-Za-z0-9_]{8,})\b",
        r"(?i)\b(?:api[_-]?key|token|password|passwd|secret)\s*[:=]\s*[^\s,;]+",
        r"(?i)\bBearer\s+[A-Za-z0-9._~+/-]+=*",
    ]
    .into_iter()
    .map(|pattern| Regex::new(pattern).expect("凭据正则应有效"))
    .collect()
});

pub fn detect_sensitive(content: &str) -> bool {
    CREDENTIAL_PATTERNS
        .iter()
        .any(|pattern| pattern.is_match(content))
}

pub fn mark_block_sensitive(block: &mut PreviewBlock) {
    if detect_sensitive(&block.content) {
        block.sensitive = true;
        block.default_expanded = false;
    }
}
