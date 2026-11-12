export interface TauriCommandError {
  message?: unknown;
  code?: unknown;
}

export function describeError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }
  if (typeof error === "string") {
    return error;
  }
  if (error && typeof error === "object") {
    const commandError = error as TauriCommandError;
    if (typeof commandError.message === "string" && commandError.message.trim()) {
      return commandError.message;
    }
  }
  return "未知错误，请重试";
}
