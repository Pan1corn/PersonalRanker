import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

const stylesheet = readFileSync(new URL("./styles.css", import.meta.url), "utf8");

function zIndexFor(selector: string): number {
  const escapedSelector = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const rule = stylesheet.match(new RegExp(`${escapedSelector}\\s*\\{([^}]*)\\}`));
  const zIndex = rule?.[1]?.match(/z-index:\s*(\d+)/)?.[1];
  if (!zIndex) throw new Error(`找不到 ${selector} 的 z-index`);
  return Number(zIndex);
}

describe("模态窗口层级", () => {
  it("遮罩和内容始终位于首页操作按钮上方", () => {
    expect(zIndexFor(".dialog-overlay")).toBeGreaterThan(zIndexFor(".action-panel"));
    expect(zIndexFor(".dialog-content")).toBeGreaterThan(zIndexFor(".dialog-overlay"));
  });
});

describe("首页视口布局", () => {
  it("首页和窗口正文都不会生成滚动条", () => {
    expect(stylesheet).toMatch(/body\.landing-page\s*\{[^}]*overflow:\s*hidden/s);
    expect(stylesheet).toMatch(/\.landing\s*\{[^}]*height:\s*100dvh[^}]*overflow:\s*hidden/s);
  });
});
