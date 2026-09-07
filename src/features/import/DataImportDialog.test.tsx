// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { commitDatasetImport, previewPastedText } from "./import-api";
import { DataImportDialog } from "./DataImportDialog";
import type { DatasetOverview, ImportResult } from "./types";

vi.mock("@tauri-apps/plugin-dialog", () => ({ open: vi.fn() }));
vi.mock("./import-api", () => ({
  commitDatasetImport: vi.fn(),
  inspectJsonArrayNodes: vi.fn(),
  previewImportFile: vi.fn(),
  previewPastedText: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("DataImportDialog", () => {
  it("完成粘贴文本的预览、字段配置和确认导入", async () => {
    const preview: ImportResult = {
      sourceType: "text",
      fields: [
        {
          name: "名称",
          fieldType: "text",
          displayOrder: 0,
          emptyCount: 0,
          uniqueCount: 2,
          samples: ["任务A", "任务B"],
        },
        {
          name: "备注",
          fieldType: "text",
          displayOrder: 1,
          emptyCount: 0,
          uniqueCount: 2,
          samples: ["甲", "乙"],
        },
      ],
      items: [
        { id: "1", originalIndex: 0, fields: { 名称: "任务A", 备注: "甲" } },
        { id: "2", originalIndex: 1, fields: { 名称: "任务B", 备注: "乙" } },
      ],
      removedEmptyCount: 1,
      duplicateCount: 0,
      deduplicated: false,
    };
    const saved: DatasetOverview = {
      id: "dataset-1",
      name: "粘贴文本",
      sourceType: "text",
      itemCount: 2,
      fields: [],
    };
    vi.mocked(previewPastedText).mockResolvedValue(preview);
    vi.mocked(commitDatasetImport).mockResolvedValue(saved);
    const onClose = vi.fn();
    const onImported = vi.fn();

    render(
      <DataImportDialog
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        onClose={onClose}
        onImported={onImported}
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "粘贴文本" }));
    fireEvent.change(screen.getByPlaceholderText("每个非空行会成为一个条目"), {
      target: { value: "任务A\n任务B" },
    });
    fireEvent.click(screen.getByRole("button", { name: "解析文本" }));

    await screen.findByText("有效条目");
    expect(screen.getByLabelText("名称作为主标识")).toBeChecked();
    fireEvent.click(screen.getByLabelText("备注作为辅助标识"));
    fireEvent.click(screen.getByRole("button", { name: "确认导入" }));

    await waitFor(() => expect(commitDatasetImport).toHaveBeenCalledOnce());
    expect(commitDatasetImport).toHaveBeenCalledWith(
      expect.objectContaining({
        projectPath: "D:\\test.subject-sort",
        datasetName: "粘贴文本",
        source: { kind: "pasted_text", content: "任务A\n任务B" },
        primaryIdentifier: "名称",
        auxiliaryIdentifiers: ["备注"],
      }),
    );
    expect(onImported).toHaveBeenCalledWith(saved);
    expect(onClose).toHaveBeenCalledOnce();
  });
});
