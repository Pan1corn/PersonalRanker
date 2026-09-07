export interface AppErrorPayload {
  code: string;
  message: string;
  detail?: string;
}

export function normalizeAppError(error: unknown): AppErrorPayload {
  if (typeof error === "object" && error !== null && "message" in error) {
    const candidate = error as Partial<AppErrorPayload>;
    return {
      code: candidate.code ?? "UNKNOWN",
      message: typeof candidate.message === "string" ? candidate.message : "操作失败，请稍后重试。",
      detail: typeof candidate.detail === "string" ? candidate.detail : undefined,
    };
  }

  if (typeof error === "string") {
    try {
      return normalizeAppError(JSON.parse(error) as unknown);
    } catch {
      return { code: "UNKNOWN", message: error };
    }
  }

  return { code: "UNKNOWN", message: "操作失败，请稍后重试。" };
}
