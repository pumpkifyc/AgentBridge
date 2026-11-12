use rcgen::{CertificateParams, DistinguishedName, DnType, Ia5String, KeyPair, SanType};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

pub const PROTOCOL_VERSION: u16 = 1;
pub const MAX_TRANSFER_BYTES: usize = 20 * 1024 * 1024;
pub const MAX_CHUNK_BYTES: usize = 64 * 1024;
pub const MAX_FAILED_AUTHENTICATIONS: usize = 3;

#[derive(Debug, Clone)]
pub struct EphemeralCertificate {
    pub certificate_der: Vec<u8>,
    pub private_key_der: Vec<u8>,
}

impl EphemeralCertificate {
    pub fn generate() -> Self {
        let key_pair = KeyPair::generate().expect("生成临时证书密钥失败");
        let mut params = CertificateParams::new(vec!["agentbridge.local".to_owned()])
            .expect("构造证书参数失败");
        params.distinguished_name = DistinguishedName::new();
        params
            .distinguished_name
            .push(DnType::CommonName, "AgentBridge 临时分享");
        params.subject_alt_names = vec![SanType::DnsName(Ia5String::try_from("agentbridge.local".to_owned()).expect("DNS 名称有效"))];
        let certificate = params
            .self_signed(&key_pair)
            .expect("签发临时证书失败");
        Self {
            certificate_der: certificate.der().to_vec(),
            private_key_der: key_pair.serialize_der(),
        }
    }

    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest(&self.certificate_der);
        format!("sha256:{}", hex::encode(digest))
    }
}

static CERTIFICATES: OnceLock<Mutex<HashMap<String, EphemeralCertificate>>> = OnceLock::new();

pub fn register_certificate(certificate: EphemeralCertificate) {
    CERTIFICATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("证书锁已中毒")
        .insert(certificate.fingerprint(), certificate);
}

pub fn certificate_for(fingerprint: &str) -> Option<EphemeralCertificate> {
    CERTIFICATES
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("证书锁已中毒")
        .get(fingerprint)
        .cloned()
}

pub fn remove_certificate(fingerprint: &str) {
    if let Some(certificates) = CERTIFICATES.get()
        && let Ok(mut certificates) = certificates.lock()
    {
        certificates.remove(fingerprint);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ClientMessage {
    Hello {
        #[serde(rename = "protocolVersion")]
        protocol_version: u16,
        secret: String,
    },
    Preview,
    Decline,
    Accept {
        approved: bool,
    },
    #[serde(rename_all = "camelCase")]
    ChunkAck {
        index: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerMessage {
    #[serde(rename_all = "camelCase")]
    Metadata {
        share_metadata: ShareMetadata,
        total_bytes: u32,
        chunk_count: u32,
    },
    #[serde(rename_all = "camelCase")]
    Chunk {
        index: u32,
        bytes: String,
    },
    #[serde(rename_all = "camelCase")]
    Digest {
        sha256: String,
    },
    Error {
        code: ErrorCode,
        message: String,
    },
    Declined,
    Complete,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ShareMetadata {
    pub share_id: String,
    pub source_title: String,
    pub project_name: Option<String>,
    pub repository_id: Option<String>,
    pub source_working_directory: Option<String>,
    pub message_count: usize,
    pub cutoff_turn_id: Option<String>,
    pub warnings: Vec<String>,
    pub schema_version: u32,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    ProtocolMismatch,
    Unauthorized,
    Expired,
    Revoked,
    RateLimited,
    Declined,
    Cancelled,
    TransferFailed,
}

impl ShareMetadata {
    pub fn from_snapshot(
        share_id: &str,
        snapshot: &crate::snapshot::schema::SnapshotV1,
    ) -> Self {
        Self {
            share_id: share_id.to_owned(),
            source_title: snapshot.metadata.title.clone(),
            project_name: snapshot.metadata.project_name.clone(),
            repository_id: snapshot.metadata.repository_id.clone(),
            source_working_directory: snapshot.metadata.source_working_directory.clone(),
            message_count: snapshot.messages.len(),
            cutoff_turn_id: snapshot.metadata.cutoff_turn_id.clone(),
            warnings: snapshot.warnings.clone(),
            schema_version: snapshot.schema_version,
        }
    }
}
