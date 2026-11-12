import { invoke } from "@tauri-apps/api/core";

export type ShareState =
  | "等待配对"
  | "已配对"
  | "预览中"
  | "传输中"
  | "传输完成"
  | "已导入"
  | "已拒绝"
  | "已过期"
  | "已撤销"
  | "已取消"
  | "失败";

export interface ShareSession {
  id: string;
  endpoint: string;
  secret: string;
  fingerprint: string;
  verificationCode: string;
  expiresAt: number;
  state: ShareState;
}

export const pairingApi = {
  async startShare(threadId: string, previewId: string, bindIp: string): Promise<ShareSession> {
    return invoke<ShareSession>("start_share", {
      request: { threadId, previewId, bindIp },
    });
  },

  async status(invitationId: string): Promise<ShareState> {
    return invoke<ShareState>("share_status", { invitationId });
  },

  async revoke(invitationId: string): Promise<void> {
    return invoke<void>("revoke_share", { invitationId });
  },

  async cancel(invitationId: string): Promise<void> {
    return invoke<void>("cancel_share", { invitationId });
  },
};

export interface PairingInfo {
  endpoint: string;
  secret: string;
  fingerprint: string;
  verificationCode: string;
  expiresAt: number;
}

const PAIRING_KEYS = [
  "endpoint",
  "secret",
  "fingerprint",
  "verificationCode",
  "expiresAt",
] as const;

export function buildPairingInfo(session: ShareSession): string {
  const info: PairingInfo = {
    endpoint: session.endpoint,
    secret: session.secret,
    fingerprint: session.fingerprint,
    verificationCode: session.verificationCode,
    expiresAt: session.expiresAt,
  };
  return JSON.stringify(info);
}

export async function parsePairingInfo(text: string): Promise<PairingInfo> {
  let value: unknown;
  try {
    value = JSON.parse(text);
  } catch {
    throw new Error("配对信息无效：请复制完整的 JSON 文本");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new Error("配对信息无效：请复制完整的 JSON 文本");
  }
  const object = value as Record<string, unknown>;
  const keys = Object.keys(object).sort();
  if (keys.length !== PAIRING_KEYS.length
    || keys.some((key, index) => key !== [...PAIRING_KEYS].sort()[index])
    || PAIRING_KEYS.some((key) => key === "expiresAt"
      ? typeof object[key] !== "number"
      : typeof object[key] !== "string" || object[key] === "")
    || !/^sha256:[0-9a-f]{64}$/i.test(String(object.fingerprint))) {
    throw new Error("配对信息无效：请从分享方重新复制");
  }
  const info = object as unknown as PairingInfo;
  if (!await matchesVerificationCode(info.fingerprint, info.verificationCode)) {
    throw new Error("配对信息无效：请从分享方重新复制");
  }
  return info;
}

async function matchesVerificationCode(
  fingerprint: string,
  verificationCode: string,
): Promise<boolean> {
  if (!/^\d{6}$/.test(verificationCode)) {
    return false;
  }
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(fingerprint),
  );
  const bytes = new Uint8Array(digest);
  const number = ((bytes[0] << 24) | (bytes[1] << 16) | (bytes[2] << 8) | bytes[3]) >>> 0;
  return String(number % 1_000_000).padStart(6, "0") === verificationCode;
}
