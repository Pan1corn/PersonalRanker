// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { loadMediaPreview } from "./media-api";
import { MediaThumbnail } from "./MediaThumbnail";

vi.mock("./media-api", () => ({ loadMediaPreview: vi.fn() }));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("MediaThumbnail", () => {
  it("renders local media and keeps remote media blocked unless enabled", async () => {
    vi.mocked(loadMediaPreview).mockResolvedValueOnce({
      status: "local_ready",
      dataUrl: "data:image/png;base64,AA==",
    });
    const { rerender } = render(
      <MediaThumbnail
        projectPath="D:\\test.subject-sort"
        itemId="item-1"
        fieldName="图片"
        value="cover.png"
        allowNetworkImages={false}
      />,
    );
    expect(await screen.findByRole("img", { name: "图片缩略图" })).toHaveAttribute(
      "src",
      "data:image/png;base64,AA==",
    );

    vi.mocked(loadMediaPreview).mockResolvedValueOnce({ status: "remote_blocked" });
    rerender(
      <MediaThumbnail
        projectPath="D:\\test.subject-sort"
        itemId="item-2"
        fieldName="图片"
        value="https://example.com/cover.webp"
        allowNetworkImages={false}
      />,
    );
    expect(await screen.findByText("网络图片未启用")).toBeInTheDocument();
  });

  it("shows a failure placeholder when an enabled remote image fails", async () => {
    vi.mocked(loadMediaPreview).mockResolvedValue({
      status: "remote_ready",
      url: "https://example.invalid/missing.png",
    });
    render(
      <MediaThumbnail
        projectPath="D:\\test.subject-sort"
        itemId="item-1"
        fieldName="图片"
        value="https://example.invalid/missing.png"
        allowNetworkImages
      />,
    );
    fireEvent.error(await screen.findByRole("img"));
    expect(await screen.findByText("图片不可用")).toBeInTheDocument();
  });
});
