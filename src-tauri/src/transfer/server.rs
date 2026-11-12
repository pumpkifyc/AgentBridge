use super::{
    integrity,
    protocol::{
        ClientMessage, ErrorCode, MAX_CHUNK_BYTES, MAX_FAILED_AUTHENTICATIONS, PROTOCOL_VERSION,
        ServerMessage, ShareMetadata, certificate_for,
    },
};
use crate::{
    pairing::{Invitation, InvitationManager, InvitationState},
    snapshot::schema::SnapshotV1,
};
use rustls::{
    ServerConfig,
    pki_types::{CertificateDer, PrivateKeyDer},
};
use std::{
    net::{IpAddr, SocketAddr},
    sync::{Arc, Mutex, atomic::{AtomicBool, Ordering}},
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::oneshot,
};
use tokio_rustls::TlsAcceptor;

pub struct TransferServerHandle {
    pub local_addr: SocketAddr,
    shutdown: std::sync::Mutex<Option<oneshot::Sender<()>>>,
    stopped: Arc<AtomicBool>,
}

impl TransferServerHandle {
    pub fn stop(&self) {
        self.stopped.store(true, Ordering::SeqCst);
        if let Some(shutdown) = self.shutdown.lock().expect("关闭锁已中毒").take() {
            let _ = shutdown.send(());
        }
    }

    pub fn is_stopped(&self) -> bool {
        self.stopped.load(Ordering::SeqCst)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum TransferServerError {
    #[error("绑定局域网接口失败：{0}")]
    Bind(String),
    #[error("临时证书不存在")]
    MissingCertificate,
    #[error("TLS 配置错误：{0}")]
    Tls(String),
    #[error("快照无法序列化：{0}")]
    Encode(String),
    #[error("分享状态错误：{0}")]
    Invitation(String),
}

pub struct TransferServer;

impl TransferServer {
    pub async fn serve(
        invitation: Invitation,
        snapshot: SnapshotV1,
        manager: Arc<InvitationManager>,
    ) -> Result<TransferServerHandle, TransferServerError> {
        let bind_ip: IpAddr = invitation
            .bind_ip
            .parse::<IpAddr>()
            .map_err(|error| TransferServerError::Bind(error.to_string()))?;
        let address = SocketAddr::new(bind_ip, invitation.port);
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| TransferServerError::Bind(error.to_string()))?;
        let local_addr = listener
            .local_addr()
            .map_err(|error| TransferServerError::Bind(error.to_string()))?;

        let certificate = certificate_for(&invitation.fingerprint)
            .ok_or(TransferServerError::MissingCertificate)?;
        let config = ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(certificate.certificate_der)],
                PrivateKeyDer::try_from(certificate.private_key_der)
                    .map_err(|error| TransferServerError::Tls(error.to_string()))?,
            )
            .map_err(|error| TransferServerError::Tls(error.to_string()))?;
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let bytes = Arc::new(
            serde_json::to_vec(&snapshot)
                .map_err(|error| TransferServerError::Encode(error.to_string()))?,
        );
        let metadata = ShareMetadata::from_snapshot(&invitation.id, &snapshot);
        let failed_auths = Arc::new(Mutex::new(0usize));
        let consumed = Arc::new(AtomicBool::new(false));
        let stopped = Arc::new(AtomicBool::new(false));
        let active_transfers = Arc::new(Mutex::new(Vec::<tokio::task::AbortHandle>::new()));
        let (shutdown, mut shutdown_rx) = oneshot::channel();

        let stopped_in_task = Arc::clone(&stopped);
        tokio::spawn(async move {
            let stopped = stopped_in_task;
            loop {
                tokio::select! {
                    _ = &mut shutdown_rx => {
                        stopped.store(true, Ordering::SeqCst);
                        break;
                    }
                    accepted = listener.accept() => {
                        let (stream, _) = match accepted {
                            Ok(value) => value,
                            Err(_) => break,
                        };
                        let acceptor = acceptor.clone();
                        let invitation = invitation.clone();
                        let manager = manager.clone();
                        let bytes = Arc::clone(&bytes);
                        let metadata = metadata.clone();
                        let failed_auths = failed_auths.clone();
                        let consumed = consumed.clone();
                        let stopped = Arc::clone(&stopped);
                        let task = tokio::spawn(async move {
                            if stopped.load(Ordering::SeqCst) {
                                return;
                            }
                            let tls = match acceptor.accept(stream).await {
                                Ok(stream) => stream,
                                Err(_) => return,
                            };
                            let _ = handle_connection(
                                tls,
                                invitation,
                                manager,
                                bytes,
                                metadata,
                                failed_auths,
                                consumed,
                            ).await;
                        });
                        active_transfers
                            .lock()
                            .expect("传输任务锁已中毒")
                            .push(task.abort_handle());
                    }
                }
            }
            for handle in active_transfers
                .lock()
                .expect("传输任务锁已中毒")
                .iter()
            {
                handle.abort();
            }
        });

        Ok(TransferServerHandle {
            local_addr,
            shutdown: Mutex::new(Some(shutdown)),
            stopped,
        })
    }
}

async fn handle_connection(
    stream: tokio_rustls::server::TlsStream<TcpStream>,
    invitation: Invitation,
    manager: Arc<InvitationManager>,
    bytes: Arc<Vec<u8>>,
    metadata: ShareMetadata,
    failed_auths: Arc<Mutex<usize>>,
    consumed: Arc<AtomicBool>,
) -> Result<(), TransferServerError> {
    let (reader, mut writer) = tokio::io::split(stream);
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    if reader.read_line(&mut line).await.is_err() {
        return Ok(());
    }
    let hello: ClientMessage = match serde_json::from_str(&line) {
        Ok(ClientMessage::Hello {
            protocol_version,
            secret,
        }) => ClientMessage::Hello {
            protocol_version,
            secret,
        },
        _ => {
            let _ = send(
                &mut writer,
                &ServerMessage::Error {
                    code: ErrorCode::ProtocolMismatch,
                    message: "无效的握手消息".into(),
                },
            )
            .await;
            return Ok(());
        }
    };

    let ClientMessage::Hello {
        protocol_version,
        secret,
    } = hello
    else {
        unreachable!()
    };
    if protocol_version != PROTOCOL_VERSION {
        let _ = send(
            &mut writer,
            &ServerMessage::Error {
                code: ErrorCode::ProtocolMismatch,
                message: "传输协议版本不兼容".into(),
            },
        )
        .await;
        return Ok(());
    }
    let active = manager.get(&invitation.id);
    let invalid_secret = secret != invitation.secret;
    if invalid_secret || active.is_err() {
        let count = {
            let mut count = failed_auths.lock().expect("认证计数锁已中毒");
            *count = count.saturating_add(1);
            *count
        };
        let code = if count >= MAX_FAILED_AUTHENTICATIONS {
            ErrorCode::RateLimited
        } else if active.is_err() {
            ErrorCode::Revoked
        } else {
            ErrorCode::Unauthorized
        };
        let _ = send(
            &mut writer,
            &ServerMessage::Error {
                code,
                message: "配对凭据无效或邀请已失效".into(),
            },
        )
        .await;
        return Ok(());
    }
    if consumed.swap(true, Ordering::SeqCst) {
        let _ = send(
            &mut writer,
            &ServerMessage::Error {
                code: ErrorCode::Unauthorized,
                message: "邀请凭据已使用".into(),
            },
        )
        .await;
        return Ok(());
    }
    manager.set_state(&invitation.id, InvitationState::Paired);

    line.clear();
    if reader.read_line(&mut line).await.is_err() {
        return Ok(());
    }
    match serde_json::from_str::<ClientMessage>(&line) {
        Ok(ClientMessage::Preview) => {
            manager.set_state(&invitation.id, InvitationState::Previewed);
            let chunk_count = bytes.len().div_ceil(MAX_CHUNK_BYTES) as u32;
            let _ = send(
                &mut writer,
                &ServerMessage::Metadata {
                    share_metadata: metadata,
                    total_bytes: bytes.len() as u32,
                    chunk_count,
                },
            )
            .await;
        }
        Ok(ClientMessage::Decline) => {
            manager.set_state(&invitation.id, InvitationState::Declined);
            let _ = send(&mut writer, &ServerMessage::Declined).await;
            return Ok(());
        }
        _ => {
            let _ = send(
                &mut writer,
                &ServerMessage::Error {
                    code: ErrorCode::ProtocolMismatch,
                    message: "必须先预览分享".into(),
                },
            )
            .await;
            return Ok(());
        }
    }

    line.clear();
    if reader.read_line(&mut line).await.is_err() {
        return Ok(());
    }
    let approved = matches!(
        serde_json::from_str::<ClientMessage>(&line),
        Ok(ClientMessage::Accept { approved: true })
    );
    if !approved {
        manager.set_state(&invitation.id, InvitationState::Declined);
        let _ = send(&mut writer, &ServerMessage::Declined).await;
        return Ok(());
    }
    manager.set_state(&invitation.id, InvitationState::Transferring);

    for (index, chunk) in bytes.chunks(MAX_CHUNK_BYTES).enumerate() {
        line.clear();
        if reader.read_line(&mut line).await.is_err() {
            return Ok(());
        }
        let ack = serde_json::from_str::<ClientMessage>(&line);
        if !matches!(ack, Ok(ClientMessage::ChunkAck { index: ack_index }) if ack_index as usize == index) {
            manager.set_state(&invitation.id, InvitationState::Failed);
            let _ = send(
                &mut writer,
                &ServerMessage::Error {
                    code: ErrorCode::TransferFailed,
                    message: "数据块确认无效".into(),
                },
            )
            .await;
            return Ok(());
        }
        let message = ServerMessage::Chunk {
            index: index as u32,
            bytes: integrity::encode_chunk(chunk),
        };
        if send(&mut writer, &message).await.is_err() {
            manager.set_state(&invitation.id, InvitationState::Failed);
            return Ok(());
        }
    }
    let _ = send(
        &mut writer,
        &ServerMessage::Digest {
            sha256: integrity::digest(&bytes),
        },
    )
    .await;
    line.clear();
    if reader.read_line(&mut line).await.is_err() {
        return Ok(());
    }
    if matches!(
        serde_json::from_str::<ClientMessage>(&line),
        Ok(ClientMessage::Accept { approved: true })
    ) {
        let _ = send(&mut writer, &ServerMessage::Complete).await;
        manager.set_state(&invitation.id, InvitationState::Accepted);
    } else {
        manager.set_state(&invitation.id, InvitationState::Failed);
    }
    Ok(())
}

async fn send<W: AsyncWriteExt + Unpin>(
    writer: &mut W,
    message: &ServerMessage,
) -> std::io::Result<()> {
    let value = serde_json::to_vec(message).unwrap_or_default();
    writer.write_all(&value).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await
}

#[allow(dead_code)]
async fn read_message<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    timeout: Duration,
) -> Option<ClientMessage> {
    let mut line = String::new();
    match tokio::time::timeout(timeout, reader.read_line(&mut line)).await {
        Ok(Ok(_)) => serde_json::from_str(&line).ok(),
        _ => None,
    }
}
