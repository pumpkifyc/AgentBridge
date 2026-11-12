use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use getrandom::fill as fill_random;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    net::{IpAddr, Ipv4Addr},
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};

pub const DEFAULT_TTL_SECONDS: u64 = 5 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Invitation {
    pub id: String,
    pub snapshot_id: String,
    pub endpoint: String,
    pub bind_ip: String,
    pub port: u16,
    pub secret: String,
    pub fingerprint: String,
    pub verification_code: String,
    pub expires_at: i64,
    #[serde(default)]
    pub state: InvitationState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum InvitationState {
    #[default]
    Waiting,
    Paired,
    Previewed,
    Transferring,
    Accepted,
    Imported,
    Declined,
    Expired,
    Revoked,
    Cancelled,
    Failed,
}

impl InvitationState {
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Waiting => "等待配对",
            Self::Paired => "已配对",
            Self::Previewed => "预览中",
            Self::Transferring => "传输中",
            Self::Accepted => "传输完成",
            Self::Imported => "已导入",
            Self::Declined => "已拒绝",
            Self::Expired => "已过期",
            Self::Revoked => "已撤销",
            Self::Cancelled => "已取消",
            Self::Failed => "失败",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InvitationPublic {
    pub id: String,
    pub endpoint: String,
    pub secret: String,
    pub verification_code: String,
    pub expires_at: i64,
    pub fingerprint: String,
    pub state: InvitationState,
}

impl Invitation {
    pub fn public_view(&self) -> InvitationPublic {
        InvitationPublic {
            id: self.id.clone(),
            endpoint: self.endpoint.clone(),
            secret: self.secret.clone(),
            verification_code: self.verification_code.clone(),
            expires_at: self.expires_at,
            fingerprint: self.fingerprint.clone(),
            state: self.state,
        }
    }

    pub fn is_expired(&self, now: i64) -> bool {
        now >= self.expires_at
    }

    pub fn is_active(&self, now: i64) -> bool {
        !self.is_expired(now)
            && matches!(
                self.state,
                InvitationState::Waiting
                    | InvitationState::Paired
                    | InvitationState::Previewed
                    | InvitationState::Transferring
            )
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum InvitationError {
    #[error("邀请不存在")]
    NotFound,
    #[error("邀请已过期")]
    Expired,
    #[error("邀请已被撤销或完成")]
    Inactive,
}

pub struct InvitationManager {
    invitations: Mutex<HashMap<String, Invitation>>,
}

impl Default for InvitationManager {
    fn default() -> Self {
        Self::new()
    }
}

impl InvitationManager {
    pub fn new() -> Self {
        Self {
            invitations: Mutex::new(HashMap::new()),
        }
    }

    pub fn create(&self, snapshot_id: impl Into<String>) -> Invitation {
        self.create_with_options(
            snapshot_id,
            IpAddr::V4(Ipv4Addr::LOCALHOST),
            0,
            DEFAULT_TTL_SECONDS,
        )
    }

    pub fn create_with_options(
        &self,
        snapshot_id: impl Into<String>,
        bind_ip: IpAddr,
        port: u16,
        ttl_seconds: u64,
    ) -> Invitation {
        let mut id_bytes = [0u8; 16];
        let mut secret_bytes = [0u8; 32];
        fill_random(&mut id_bytes).expect("操作系统随机数生成器失败");
        fill_random(&mut secret_bytes).expect("操作系统随机数生成器失败");
        let id = hex::encode(id_bytes);
        let secret = URL_SAFE_NO_PAD.encode(secret_bytes);
        let certificate = crate::transfer::protocol::EphemeralCertificate::generate();
        let fingerprint = certificate.fingerprint();
        let verification_code = short_code(&fingerprint);
        let expires_at = unix_seconds() + i64::try_from(ttl_seconds).unwrap_or(i64::MAX);
        let endpoint = format!("{}:{}", format_host(bind_ip), port);
        let invitation = Invitation {
            id,
            snapshot_id: snapshot_id.into(),
            endpoint,
            bind_ip: bind_ip.to_string(),
            port,
            secret,
            fingerprint,
            verification_code,
            expires_at,
            state: InvitationState::Waiting,
        };
        self.invitations
            .lock()
            .expect("邀请锁已中毒")
            .insert(invitation.id.clone(), invitation.clone());
        crate::transfer::protocol::register_certificate(certificate);
        invitation
    }

    pub fn get(&self, invitation_id: &str) -> Result<Invitation, InvitationError> {
        let invitations = self.invitations.lock().expect("邀请锁已中毒");
        let invitation = invitations
            .get(invitation_id)
            .cloned()
            .ok_or(InvitationError::NotFound)?;
        drop(invitations);
        let now = unix_seconds();
        if invitation.is_expired(now) {
            self.set_state(invitation_id, InvitationState::Expired);
            return Err(InvitationError::Expired);
        }
        if !invitation.is_active(now) {
            return Err(InvitationError::Inactive);
        }
        Ok(invitation)
    }

    pub fn status(&self, invitation_id: &str) -> Result<InvitationState, InvitationError> {
        let mut invitations = self.invitations.lock().expect("邀请锁已中毒");
        let invitation = invitations
            .get_mut(invitation_id)
            .ok_or(InvitationError::NotFound)?;
        if invitation.is_expired(unix_seconds())
            && invitation.state != InvitationState::Imported
            && invitation.state != InvitationState::Declined
            && invitation.state != InvitationState::Revoked
            && invitation.state != InvitationState::Cancelled
            && invitation.state != InvitationState::Accepted
        {
            invitation.state = InvitationState::Expired;
        }
        Ok(invitation.state)
    }

    pub fn set_state(&self, invitation_id: &str, state: InvitationState) {
        if let Some(invitation) = self
            .invitations
            .lock()
            .expect("邀请锁已中毒")
            .get_mut(invitation_id)
        {
            invitation.state = state;
        }
    }

    pub fn revoke(&self, invitation_id: &str) -> Result<(), InvitationError> {
        let mut invitations = self.invitations.lock().expect("邀请锁已中毒");
        let invitation = invitations
            .get_mut(invitation_id)
            .ok_or(InvitationError::NotFound)?;
        if !matches!(
            invitation.state,
            InvitationState::Waiting
                | InvitationState::Paired
                | InvitationState::Previewed
                | InvitationState::Transferring
        ) {
            return Err(InvitationError::Inactive);
        }
        invitation.state = InvitationState::Revoked;
        Ok(())
    }

    pub fn cancel(&self, invitation_id: &str) -> Result<(), InvitationError> {
        let mut invitations = self.invitations.lock().expect("邀请锁已中毒");
        let invitation = invitations
            .get_mut(invitation_id)
            .ok_or(InvitationError::NotFound)?;
        if invitation.is_active(unix_seconds()) {
            invitation.state = InvitationState::Cancelled;
            Ok(())
        } else {
            Err(InvitationError::Inactive)
        }
    }
}

pub fn unix_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
        .try_into()
        .unwrap_or(i64::MAX)
}

fn format_host(ip: IpAddr) -> String {
    match ip {
        IpAddr::V4(ip) => ip.to_string(),
        IpAddr::V6(ip) => format!("[{ip}]"),
    }
}

pub fn short_code(fingerprint: &str) -> String {
    let digest = Sha256::digest(fingerprint.as_bytes());
    let number = u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]);
    format!("{:06}", number % 1_000_000)
}
