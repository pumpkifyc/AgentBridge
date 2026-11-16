import { UpdaterError, type UpdateErrorCode } from "./updateTypes";

const MESSAGES: Record<UpdateErrorCode, string> = {
  network: "无法连接更新服务器，请检查网络后重试。",
  manifest: "更新清单无效或暂无适用于当前平台的安装包。",
  download: "更新下载失败，请检查网络和磁盘空间后重试。",
  signature: "更新包签名校验失败，当前版本未更改。",
  install: "安装更新失败，当前版本未更改。",
  restart: "更新已安装，但应用未能自动重启，请手动重新打开 AgentBridge。",
  unknown: "更新失败，请稍后重试。",
};

export function formatUpdateError(error: unknown): string {
  return error instanceof UpdaterError
    ? MESSAGES[error.code]
    : MESSAGES.unknown;
}
