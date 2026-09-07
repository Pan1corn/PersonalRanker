import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

interface Capability {
  permissions: string[];
}

interface TauriConfig {
  app: {
    security: {
      csp: Record<string, string>;
    };
  };
}

const capability = JSON.parse(
  readFileSync(new URL("../src-tauri/capabilities/default.json", import.meta.url), "utf8"),
) as Capability;
const tauriConfig = JSON.parse(
  readFileSync(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
) as TauriConfig;

describe("桌面窗口权限", () => {
  it("允许结果页使用打开、保存和确认类对话框", () => {
    expect(capability.permissions).toContain("dialog:default");
  });

  it("仅启用应用运行和文件对话框所需的能力", () => {
    expect(capability.permissions).toEqual(["core:default", "dialog:default"]);
  });
});

describe("WebView 内容安全策略", () => {
  it("禁止远程脚本、对象和嵌入式页面", () => {
    const csp = tauriConfig.app.security.csp;
    expect(csp["script-src"]).toBe("'self'");
    expect(csp["object-src"]).toBe("'none'");
    expect(csp["frame-src"]).toBe("'none'");
    expect(csp["connect-src"]).toBe("ipc: http://ipc.localhost");
  });
});
