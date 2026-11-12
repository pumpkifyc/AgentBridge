use super::{
    integrity::{self, IntegrityError},
    protocol::{
        ClientMessage, ErrorCode, MAX_CHUNK_BYTES, MAX_TRANSFER_BYTES, PROTOCOL_VERSION,
        ServerMessage, ShareMetadata,
    },
};
use crate::snapshot::{schema::SnapshotV1, validate::validate_snapshot};
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
    #[error("快照校验失败：{0}")]
    Snapshot(String),
    #[error("连接中断")]
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct ShareProposal {
    pub metadata: ShareMetadata,
    pub total_bytes: u32,
    pub chunk_count: u32,
}

#[derive(Debug)]
pub struct TransferClient {
    invitation_id: String,
    secret: String,
    reader: BufReader<tokio::io::ReadHalf<tokio_rustls::client::TlsStream<TcpStream>>>,
    writer: tokio::io::WriteHalf<tokio_rustls::client::TlsStream<TcpStream>>,
    proposal: Option<ShareProposal>,
}

impl TransferClient {
    pub async fn preview(
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

    pub async fn accept(self) -> Result<SnapshotV1, TransferClientError> {
        self.complete().await
    }

    async fn complete(mut self) -> Result<SnapshotV1, TransferClientError> {
        let proposal = self
            .proposal
            .take()
            .ok_or_else(|| TransferClientError::Protocol("缺少预览结果".into()))?;
        send(
            &mut self.writer,
            &ClientMessage::Accept { approved: true },
        )
        .await?;
        let mut bytes = Vec::with_capacity(proposal.total_bytes as usize);
        for index in 0..proposal.chunk_count {
            send(
                &mut self.writer,
                &ClientMessage::ChunkAck { index },
            )
            .await?;
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
        let snapshot = validate_snapshot(&bytes)
            .map_err(|error| TransferClientError::Snapshot(error.to_string()))?;
        send(
            &mut self.writer,
            &ClientMessage::Accept { approved: true },
        )
        .await?;
        match receive(&mut self.reader).await? {
            ServerMessage::Complete => Ok(snapshot),
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
