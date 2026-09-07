// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { confirm } from "@tauri-apps/plugin-dialog";
import { confirmSortResult, loadResultPreview, loadScoreConfig, previewScores } from "./result-api";
import { ResultWorkspace } from "./ResultWorkspace";
import type { ResultPreviewState } from "./types";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
  message: vi.fn(),
  save: vi.fn(),
}));

vi.mock("./result-api", () => ({
  confirmSortResult: vi.fn(),
  exportSortResult: vi.fn(),
  exportTargetExists: vi.fn(),
  loadResultPreview: vi.fn(),
  loadScoreConfig: vi.fn(),
  saveScoreConfig: vi.fn(),
  previewScores: vi.fn(),
  writeScores: vi.fn(),
  unlockSortResult: vi.fn(),
  writeRankField: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("ResultWorkspace", () => {
  it("确认时展示写入进度并在完成后显示锁定状态", async () => {
    const initial = preview("sorting");
    const confirmed = preview("confirmed");
    let finishConfirmation: ((value: ResultPreviewState) => void) | undefined;
    vi.mocked(loadResultPreview).mockResolvedValue(initial);
    vi.mocked(loadScoreConfig).mockResolvedValue(null);
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(confirmSortResult).mockImplementation(
      () =>
        new Promise((resolve) => {
          finishConfirmation = resolve;
        }),
    );

    render(
      <ResultWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-04T00:00:00Z",
          lastOpenedAt: "2026-08-04T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        dataset={{
          id: "dataset-1",
          name: "候选项",
          sourceType: "text",
          itemCount: 2,
          fields: [
            {
              id: "field-1",
              name: "内容",
              fieldType: "text",
              displayOrder: 0,
              isPrimaryIdentifier: true,
              isAuxiliaryIdentifier: false,
            },
          ],
        }}
        task={{
          id: "task-1",
          datasetId: "dataset-1",
          name: "优先级",
          criteria: "越重要越靠前",
          mode: "drag",
          status: "sorting",
          initialOrder: { kind: "import" },
          rankGroupCount: 2,
          createdAt: "2026-08-04T00:00:00Z",
        }}
        onEdit={vi.fn()}
        onProjectDataChanged={vi.fn().mockResolvedValue(undefined)}
      />,
    );

    const button = await screen.findByRole("button", { name: "确认结果并锁定" });
    expect(document.querySelector(".project-topbar")).not.toBeInTheDocument();
    expect(screen.queryByText("结果完整性")).not.toBeInTheDocument();
    fireEvent.click(button);
    await screen.findByText("正在检查完整性并创建确认快照…");
    expect(screen.getByRole("button", { name: "正在确认…" })).toBeDisabled();
    expect(confirmSortResult).toHaveBeenCalledWith({
      projectPath: "D:\\test.subject-sort",
      taskId: "task-1",
    });

    finishConfirmation?.(confirmed);
    await waitFor(() => expect(screen.getByText("已确认 · 已锁定")).toBeInTheDocument());
    expect(screen.getByText("结果已确认并创建快照。")).toBeInTheDocument();
  });

  it("仅在完整性异常时显示问题并禁用确认", async () => {
    const invalid = preview("sorting");
    invalid.integrity = {
      allItemsIncluded: false,
      noUnresolvedComparisons: true,
      noDuplicateItems: true,
      noInvalidItems: true,
      problems: ["有 1 个条目未纳入最终结果。"],
    };
    invalid.canConfirm = true;
    vi.mocked(loadResultPreview).mockResolvedValue(invalid);
    vi.mocked(loadScoreConfig).mockResolvedValue(null);

    renderResultWorkspace();

    expect(await screen.findByText("结果完整性检查未通过")).toBeInTheDocument();
    expect(screen.getByText("有 1 个条目未纳入最终结果。")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "确认结果并锁定" })).toBeDisabled();
  });

  it("按线性评分设置的小数位数显示预览值", async () => {
    vi.mocked(loadResultPreview).mockResolvedValue(preview("confirmed"));
    vi.mocked(loadScoreConfig).mockResolvedValue(null);
    vi.mocked(previewScores).mockResolvedValue({
      taskId: "task-1",
      fieldName: "score",
      fieldExists: false,
      canWrite: true,
      items: [
        {
          groupId: "group-1",
          itemId: "item-1",
          primaryLabel: "A",
          rank: 1,
          score: 8.3,
          oldValue: null,
        },
      ],
    });

    renderResultWorkspace();

    fireEvent.click(await screen.findByRole("button", { name: "生成新旧值预览" }));
    expect(await screen.findByText("8.30", { selector: "strong" })).toBeInTheDocument();
  });
});

function renderResultWorkspace() {
  return render(
    <ResultWorkspace
      project={{
        id: "project-1",
        name: "测试项目",
        createdAt: "2026-08-04T00:00:00Z",
        lastOpenedAt: "2026-08-04T00:00:00Z",
        dataFormatVersion: 1,
        projectPath: "D:\\test.subject-sort",
      }}
      dataset={{
        id: "dataset-1",
        name: "候选项",
        sourceType: "text",
        itemCount: 2,
        fields: [
          {
            id: "field-1",
            name: "内容",
            fieldType: "text",
            displayOrder: 0,
            isPrimaryIdentifier: true,
            isAuxiliaryIdentifier: false,
          },
        ],
      }}
      task={{
        id: "task-1",
        datasetId: "dataset-1",
        name: "优先级",
        criteria: "越重要越靠前",
        mode: "drag",
        status: "sorting",
        initialOrder: { kind: "import" },
        rankGroupCount: 2,
        createdAt: "2026-08-04T00:00:00Z",
      }}
      onEdit={vi.fn()}
      onProjectDataChanged={vi.fn().mockResolvedValue(undefined)}
    />,
  );
}

function preview(status: "sorting" | "confirmed"): ResultPreviewState {
  return {
    taskId: "task-1",
    taskName: "优先级",
    status,
    items: [
      {
        groupId: "group-1",
        itemId: "item-1",
        rank: 1,
        originalRank: 1,
        rankChange: 0,
        primaryLabel: "A",
        fields: [{ name: "内容", value: "A" }],
      },
      {
        groupId: "group-2",
        itemId: "item-2",
        rank: 2,
        originalRank: 2,
        rankChange: 0,
        primaryLabel: "B",
        fields: [{ name: "内容", value: "B" }],
      },
    ],
    integrity: {
      allItemsIncluded: true,
      noUnresolvedComparisons: true,
      noDuplicateItems: true,
      noInvalidItems: true,
      problems: [],
    },
    canConfirm: status !== "confirmed",
    confirmedAt: status === "confirmed" ? "2026-08-04T00:00:00Z" : undefined,
  };
}
