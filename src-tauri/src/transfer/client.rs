use super::{
    integrity::{self, IntegrityError},
    protocol::{
        ClientMessage, ErrorCode, MAX_CHUNK_BYTES, MAX_TRANSFER_BYTES, PROTOCOL_VERSION,
        ServerMessage, ShareMetadata,
    },
};
use crate::native::{self, NativeBundleError};
use rustls::{
    ClientConfig,
    pki_types::{CertificateDer, ServerName},
};
use sha2::{Digest, Sha256};
use std::{io, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::TcpStream,
};
use tokio_rustls::TlsConnector;

const PREVIEW_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug, thiserror::Error)]
pub enum TransferClientError {
    #[error("连接信息无效：{0}")]
    InvalidEndpoint(String),
    #[error("连接失败：{0}")]
    Connection(#[from] io::Error),
    #[error("TLS 配置错误：{0}")]
    Tls(String),
    #[error("传输协议错误：{0}")]
    Protocol(String),
    #[error("服务端拒绝：{0:?}：{1}")]
    Rejected(ErrorCode, String),
    #[error("接收方已拒绝分享")]
    Declined,
    #[error("完整性校验失败：{0}")]
    Integrity(#[from] IntegrityError),
    #[error("原生会话包校验失败：{0}")]
    Native(#[from] NativeBundleError),
    #[error("连接中断")]
    Interrupted,
    #[error("连接分享方超时")]
    TimedOut,
}

#[derive(Debug, Clone)]
pub struct ShareProposal {
    pub metadata: ShareMetadata,
    pub total_bytes: u32,
    pub chunk_count: u32,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferProgress {
    pub kind: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
}

pub type ProgressCallback = Arc<dyn Fn(TransferProgress) + Send + Sync>;

#[derive(Debug)]
pub struct TransferClient {
    invitation_id: String,
    secret: String,
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
    proposal: Option<ShareProposal>,
}

#[derive(Debug)]
pub struct PendingTransfer {
    pub client: TransferClient,
    pub pairing: crate::transfer::protocol::PairingInfo,
    pub proposal: ShareProposal,
}

impl TransferClient {
    pub async fn preview(
        endpoint: &str,
        secret: &str,
        expected_fingerprint: &str,
    ) -> Result<(Self, ShareProposal), TransferClientError> {
        tokio::time::timeout(
            PREVIEW_TIMEOUT,
            Self::preview_inner(endpoint, secret, expected_fingerprint),
        )
        .await
        .map_err(|_| TransferClientError::TimedOut)?
    }

    async fn preview_inner(
        endpoint: &str,
        secret: &str,
        expected_fingerprint: &str,
    ) -> Result<(Self, ShareProposal), TransferClientError> {
        let address = resolve_endpoint(endpoint)?;
        let stream = TcpStream::connect(address).await?;
        let config = client_config(expected_fingerprint)?;
        let connector = TlsConnector::from(Arc::new(config));
        let server_name = ServerName::try_from("agentbridge.local")
            .map_err(|error| TransferClientError::Tls(error.to_string()))?
            .to_owned();
        let tls = connector.connect(server_name, stream).await?;
        let (reader, mut writer) = tokio::io::split(tls);
        let mut reader = BufReader::new(reader);
        send(
            &mut writer,
            &ClientMessage::Hello {
                protocol_version: PROTOCOL_VERSION,
                secret: secret.to_owned(),
            },
        )
        .await?;
        send(&mut writer, &ClientMessage::Preview).await?;
        let response = receive(&mut reader).await?;
        let (metadata, total_bytes, chunk_count) = match response {
            ServerMessage::Metadata {
                share_metadata,
                total_bytes,
                chunk_count,
            } => (share_metadata, total_bytes, chunk_count),
            ServerMessage::Error { code, message } => {
                return Err(TransferClientError::Rejected(code, message));
            }
            other => {
                return Err(TransferClientError::Protocol(format!(
                    "预览阶段收到意外响应：{other:?}"
                )));
            }
        };
        if total_bytes as usize > MAX_TRANSFER_BYTES {
            return Err(TransferClientError::Protocol("快照超过大小限制".into()));
        }
        let proposal = ShareProposal {
            metadata: metadata.clone(),
            total_bytes,
            chunk_count,
        };
        Ok((
            Self {
                invitation_id: metadata.share_id.clone(),
                secret: secret.to_owned(),
                reader,
                writer,
                proposal: Some(proposal.clone()),
            },
            proposal,
        ))
    }

    pub fn invitation_id(&self) -> &str {
        &self.invitation_id
    }

    pub fn secret(&self) -> &str {
        &self.secret
    }

    pub async fn decline(mut self) -> Result<(), TransferClientError> {
        send(&mut self.writer, &ClientMessage::Accept { approved: false }).await?;
        match receive(&mut self.reader).await? {
            ServerMessage::Declined => Ok(()),
            ServerMessage::Error { code, message } => {
                Err(TransferClientError::Rejected(code, message))
            }
            other => Err(TransferClientError::Protocol(format!(
                "拒绝阶段收到意外响应：{other:?}"
            ))),
        }
    }

    pub async fn accept(
        self,
        progress: Option<ProgressCallback>,
    ) -> Result<(Vec<u8>, Vec<serde_json::Value>), TransferClientError> {
        self.complete(progress).await
    }

    async fn complete(
        mut self,
        progress: Option<ProgressCallback>,
    ) -> Result<(Vec<u8>, Vec<serde_json::Value>), TransferClientError> {
        let proposal = self
            .proposal
            .take()
            .ok_or_else(|| TransferClientError::Protocol("缺少预览结果".into()))?;
        send(&mut self.writer, &ClientMessage::Accept { approved: true }).await?;
        let mut bytes = Vec::with_capacity(proposal.total_bytes as usize);
        let mut progress_events = Vec::new();
        let mut emit = |transferred_bytes: u64| {
            let event = TransferProgress {
                kind: "chunk".into(),
                transferred_bytes,
                total_bytes: u64::from(proposal.total_bytes),
            };
            if let Some(callback) = &progress {
                callback(event.clone());
            }
            progress_events.push(serde_json::to_value(event).unwrap_or_default());
        };
        for index in 0..proposal.chunk_count {
            send(&mut self.writer, &ClientMessage::ChunkAck { index }).await?;
            match receive(&mut self.reader).await? {
                ServerMessage::Chunk {
                    index: chunk_index,
                    bytes: encoded,
                } if chunk_index == index => {
                    let chunk = integrity::decode_chunk(&encoded)?;
                    if chunk.len() > MAX_CHUNK_BYTES
                        || bytes.len() + chunk.len() > MAX_TRANSFER_BYTES
                    {
                        return Err(TransferClientError::Protocol("数据块超过大小限制".into()));
                    }
                    bytes.extend_from_slice(&chunk);
                    emit(bytes.len() as u64);
                }
                ServerMessage::Error { code, message } => {
                    return Err(TransferClientError::Rejected(code, message));
                }
                other => {
                    return Err(TransferClientError::Protocol(format!(
                        "传输阶段收到意外响应：{other:?}"
                    )));
                }
            }
        }
        let expected = match receive(&mut self.reader).await? {
            ServerMessage::Digest { sha256 } => sha256,
            ServerMessage::Error { code, message } => {
                return Err(TransferClientError::Rejected(code, message));
            }
            other => {
                return Err(TransferClientError::Protocol(format!(
                    "摘要阶段收到意外响应：{other:?}"
                )));
            }
        };
        integrity::verify(&bytes, &expected)?;
        native::parse_bundle(&bytes)?;
        send(&mut self.writer, &ClientMessage::Accept { approved: true }).await?;
        match receive(&mut self.reader).await? {
            ServerMessage::Complete => Ok((bytes, progress_events)),
            ServerMessage::Error { code, message } => {
                Err(TransferClientError::Rejected(code, message))
            }
            _ => Err(TransferClientError::Protocol("缺少传输完成确认".into())),
        }
    }
}

fn resolve_endpoint(endpoint: &str) -> Result<std::net::SocketAddr, TransferClientError> {
    use std::net::ToSocketAddrs;
    endpoint
        .to_socket_addrs()
        .map_err(|error| TransferClientError::InvalidEndpoint(error.to_string()))?
        .next()
        .ok_or_else(|| TransferClientError::InvalidEndpoint("地址未解析到接口".into()))
}

fn client_config(expected_fingerprint: &str) -> Result<ClientConfig, TransferClientError> {
    let expected = expected_fingerprint.to_owned();
    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(FingerprintVerifier { expected }))
        .with_no_client_auth();
    Ok(config)
}

#[derive(Debug)]
struct FingerprintVerifier {
    expected: String,
}

impl rustls::client::danger::ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        let digest = Sha256::digest(end_entity.as_ref());
        let actual = format!("sha256:{}", hex::encode(digest));
        if actual == self.expected {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General("临时证书指纹不匹配".into()))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

async fn send<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    message: &ClientMessage,
) -> Result<(), TransferClientError> {
    let value = serde_json::to_vec(message)
        .map_err(|error| TransferClientError::Protocol(error.to_string()))?;
    tokio::time::timeout(Duration::from_secs(10), writer.write_all(&value))
        .await
        .map_err(|_| TransferClientError::Interrupted)?
        .map_err(TransferClientError::Connection)?;
    tokio::time::timeout(Duration::from_secs(10), writer.write_all(b"\n"))
        .await
        .map_err(|_| TransferClientError::Interrupted)?
        .map_err(TransferClientError::Connection)?;
    tokio::time::timeout(Duration::from_secs(10), writer.flush())
        .await
        .map_err(|_| TransferClientError::Interrupted)?
        .map_err(TransferClientError::Connection)?;
    Ok(())
}

async fn receive<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
) -> Result<ServerMessage, TransferClientError> {
    let mut line = String::new();
    let count = tokio::time::timeout(Duration::from_secs(10), reader.read_line(&mut line))
        .await
        .map_err(|_| TransferClientError::Interrupted)?
        .map_err(TransferClientError::Connection)?;
    if count == 0 {
        return Err(TransferClientError::Interrupted);
    }
    serde_json::from_str(&line).map_err(|error| TransferClientError::Protocol(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn preview_stops_when_the_peer_accepts_but_never_responds() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("测试监听器应启动");
        let endpoint = listener.local_addr().expect("应取得测试地址").to_string();
        let server = tokio::spawn(async move {
            let (_stream, _) = listener.accept().await.expect("应接受测试连接");
            tokio::time::sleep(Duration::from_secs(30)).await;
        });
        let fingerprint = format!("sha256:{}", "0".repeat(64));

        let result = tokio::time::timeout(
            Duration::from_secs(6),
            TransferClient::preview(&endpoint, "secret", &fingerprint),
        )
        .await;
        server.abort();

        assert!(result.is_ok(), "预览连接必须由客户端自身限时结束");
        assert!(matches!(
            result.expect("外层超时已确认不会触发"),
            Err(TransferClientError::TimedOut)
        ));
    }
}
