import { describe, expect, it } from "vitest";
import { normalizeAppError } from "./errors";

describe("normalizeAppError", () => {
  it("保留后端返回的中文错误", () => {
    expect(normalizeAppError({ code: "NOT_WRITABLE", message: "所选目录不可写。" })).toEqual({
      code: "NOT_WRITABLE",
      message: "所选目录不可写。",
      detail: undefined,
    });
  });

  it("为未知错误提供可理解的提示", () => {
    expect(normalizeAppError(null).message).toBe("操作失败，请稍后重试。");
  });
});
