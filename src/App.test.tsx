// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";
import { useProjectStore } from "./features/projects/project-store";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
  open: vi.fn(),
}));

beforeEach(() => {
  useProjectStore.setState({
    currentProject: null,
    datasets: [],
    sortTasks: [],
    busy: false,
    error: null,
  });
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("应用设置", () => {
  it("链接 PersonalRanker 仓库并用独立弹窗展示 Markdown 更新说明", () => {
    render(<App />);

    fireEvent.click(screen.getByRole("button", { name: /应用设置/ }));

    expect(screen.getByRole("link", { name: /打开仓库/ })).toHaveAttribute(
      "href",
      "https://github.com/Pan1corn/PersonalRanker",
    );
    fireEvent.click(screen.getByRole("button", { name: "查看" }));

    expect(screen.getByRole("dialog", { name: "更新说明" })).toBeInTheDocument();
    expect(screen.getByRole("region", { name: "更新说明内容" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: /v1\.0\.0\s*更新说明/i })).toBeInTheDocument();
    expect(screen.getByText(/支持从图片文件夹匹配并导入媒体文件/)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "关闭" }));
    expect(screen.queryByRole("dialog", { name: "更新说明" })).not.toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "应用设置" })).toBeInTheDocument();
  });
});

describe("首页布局", () => {
  it("只在首页锁定窗口级滚动", () => {
    const { unmount } = render(<App />);

    expect(document.body).toHaveClass("landing-page");
    unmount();
    expect(document.body).not.toHaveClass("landing-page");
  });
});
