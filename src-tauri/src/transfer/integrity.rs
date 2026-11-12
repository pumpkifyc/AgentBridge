use sha2::{Digest, Sha256};

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IntegrityError {
    #[error("快照数据超过大小限制")]
    TooLarge,
    #[error("数据块序号无效")]
    InvalidChunkIndex,
    #[error("完整性摘要不匹配")]
    DigestMismatch,
}

pub fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn verify(bytes: &[u8], expected: &str) -> Result<(), IntegrityError> {
    if bytes.len() > super::protocol::MAX_TRANSFER_BYTES {
        return Err(IntegrityError::TooLarge);
    }
    if digest(bytes) == expected.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(IntegrityError::DigestMismatch)
    }
}

pub fn encode_chunk(bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD.encode(bytes)
}

pub fn decode_chunk(encoded: &str) -> Result<Vec<u8>, IntegrityError> {
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    STANDARD
        .decode(encoded)
        .map_err(|_| IntegrityError::DigestMismatch)
}
