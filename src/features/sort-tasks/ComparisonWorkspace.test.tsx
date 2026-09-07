// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  answerComparison,
  loadComparisonWorkspace,
  renameComparisonRankGroup,
  skipComparison,
  undoComparison,
} from "./comparison-api";
import { ComparisonWorkspace } from "./ComparisonWorkspace";
import type { ComparisonItem, ComparisonWorkspaceState } from "./types";

vi.mock("./comparison-api", () => ({
  answerComparison: vi.fn(),
  loadComparisonWorkspace: vi.fn(),
  renameComparisonRankGroup: vi.fn(),
  skipComparison: vi.fn(),
  undoComparison: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe("ComparisonWorkspace", () => {
  it("使用快捷键判断、跳过并撤销", async () => {
    const longLabel = "这是一个长度明显超过卡片可用宽度的主标识字段名称";
    const initial = {
      ...workspace(item("b", longLabel), item("a", "方案A")),
      plannedComparisonCount: 3,
    };
    const answered = {
      ...workspace(item("c", "方案C"), item("a", "方案A")),
      canUndo: true,
      comparisonCount: 1,
    };
    vi.mocked(loadComparisonWorkspace).mockResolvedValue(initial);
    vi.mocked(answerComparison).mockResolvedValue(answered);
    vi.mocked(skipComparison).mockResolvedValue(answered);
    vi.mocked(undoComparison).mockResolvedValue(initial);

    render(
      <ComparisonWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        task={{
          id: "task-1",
          datasetId: "dataset-1",
          name: "方案比较",
          criteria: "更重要的靠前",
          mode: "comparison",
          status: "draft",
          initialOrder: { kind: "import" },
          rankGroupCount: 3,
          createdAt: "2026-08-03T00:00:00Z",
        }}
        onBack={vi.fn()}
      />,
    );

    const longHeading = await screen.findByRole("heading", { name: longLabel });
    expect(longHeading).toHaveClass("comparison-card-title");
    expect(longHeading).toHaveAttribute("title", longLabel);
    expect(screen.getByText("定位进度：")).toBeInTheDocument();
    expect(screen.getByText("已定位 1 / 3 条")).toBeInTheDocument();
    expect(screen.getByText("判断进度：")).toBeInTheDocument();
    expect(screen.getByText("已判断 0 / 约 3 次")).toBeInTheDocument();
    expect(
      screen.getByText("0%", { selector: ".progress-copy.estimated strong" }),
    ).toBeInTheDocument();
    expect(document.querySelector(".comparison-progress dl")).not.toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /预览当前顺序/ }));
    expect(screen.getByRole("heading", { name: "当前已确定顺序" })).toBeInTheDocument();
    const activeOrderItem = document.querySelector(".comparison-order-preview li.active");
    expect(activeOrderItem).toHaveAttribute("aria-current", "true");
    expect(activeOrderItem).toHaveTextContent("方案A");
    expect(activeOrderItem).toHaveTextContent("比较中");

    expect(document.querySelector(".project-topbar")).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "a" });
    await waitFor(() => expect(answerComparison).toHaveBeenCalledOnce());
    expect(answerComparison).toHaveBeenCalledWith({
      projectPath: "D:\\test.subject-sort",
      taskId: "task-1",
      decision: "left_before",
    });

    await screen.findByRole("heading", { name: "方案C" });
    fireEvent.keyDown(window, { key: "s" });
    await waitFor(() => expect(skipComparison).toHaveBeenCalledOnce());
    await waitFor(() => expect(screen.getByRole("button", { name: /撤销判断/ })).toBeEnabled());
    fireEvent.keyDown(window, { key: "z", ctrlKey: true });
    await waitFor(() => expect(undoComparison).toHaveBeenCalledOnce());
  });

  it("submits a tie decision from the dedicated action", async () => {
    const initial = workspace(item("b", "方案B"), item("a", "方案A"));
    vi.mocked(loadComparisonWorkspace).mockResolvedValue(initial);
    vi.mocked(answerComparison).mockResolvedValue({
      ...initial,
      comparisonCount: 1,
      canUndo: true,
    });

    render(
      <ComparisonWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        task={{
          id: "task-1",
          datasetId: "dataset-1",
          name: "方案比较",
          criteria: "更重要的靠前",
          mode: "comparison",
          status: "sorting",
          initialOrder: { kind: "import" },
          rankGroupCount: 3,
          createdAt: "2026-08-03T00:00:00Z",
        }}
        onBack={vi.fn()}
      />,
    );

    fireEvent.click(await screen.findByRole("button", { name: /二者并列/ }));

    await waitFor(() =>
      expect(answerComparison).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        decision: "tie",
      }),
    );
  });

  it("renders matrix progress and a pull-out scoreboard ordered by wins", async () => {
    const left = item("a", "方案A");
    const right = item("b", "方案B");
    vi.mocked(loadComparisonWorkspace).mockResolvedValue({
      ...workspace(left, right),
      mode: "matrix",
      comparisonCount: 2,
      plannedComparisonCount: 6,
      matrixComparisonPercent: 100,
      progressPercent: 33,
      standings: [
        { item: left, wins: 2, losses: 0, ties: 0, total: 2 },
        { item: right, wins: 1, losses: 1, ties: 0, total: 2 },
      ],
    });

    render(
      <ComparisonWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        task={{
          id: "task-1",
          datasetId: "dataset-1",
          name: "矩阵比较",
          criteria: "更重要的靠前",
          mode: "matrix",
          status: "sorting",
          initialOrder: { kind: "import" },
          matrixComparisonPercent: 100,
          rankGroupCount: 3,
          createdAt: "2026-08-03T00:00:00Z",
        }}
        onBack={vi.fn()}
      />,
    );

    expect(await screen.findByText("已比较 2 / 6")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /左侧胜出/ })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: /拉出得分表/ }));
    expect(screen.getByRole("heading", { name: "矩阵得分表" })).toBeInTheDocument();
    expect(screen.getByText("方案A", { selector: "td" })).toBeInTheDocument();
    expect(screen.getAllByText("2", { selector: "td" }).length).toBeGreaterThan(0);
  });

  it("双击并列组大标题后重命名，下方仅分行显示组内对象", async () => {
    const tied = {
      ...item("tie", "方案A"),
      itemIds: ["item-a", "item-b"],
      memberLabels: ["方案A", "方案B"],
      groupName: "并列组1",
    };
    const initial = workspace(item("c", "方案C"), tied);
    vi.mocked(loadComparisonWorkspace).mockResolvedValue(initial);
    vi.mocked(renameComparisonRankGroup).mockResolvedValue({
      ...initial,
      right: { ...tied, groupName: "第一梯队" },
    });
    vi.spyOn(window, "prompt").mockReturnValue("第一梯队");

    render(
      <ComparisonWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        task={{
          id: "task-1",
          datasetId: "dataset-1",
          name: "方案比较",
          criteria: "更重要的靠前",
          mode: "comparison",
          status: "sorting",
          initialOrder: { kind: "import" },
          rankGroupCount: 2,
          createdAt: "2026-08-03T00:00:00Z",
        }}
        onBack={vi.fn()}
      />,
    );

    const heading = await screen.findByRole("heading", { name: "并列组1" });
    const members = document.querySelector(".comparison-tie-members");
    expect(heading).toHaveClass("comparison-group-title");
    expect(heading).toHaveAttribute("title", "双击修改并列组名称");
    expect(members).not.toHaveTextContent("并列组1");
    expect(members?.querySelectorAll("li")).toHaveLength(2);
    expect(members?.querySelector("button")).not.toBeInTheDocument();
    fireEvent.doubleClick(heading);
    await waitFor(() =>
      expect(renameComparisonRankGroup).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        groupId: "tie",
        name: "第一梯队",
      }),
    );
    expect(await screen.findByRole("heading", { name: "第一梯队" })).toBeInTheDocument();
    expect(document.querySelector(".comparison-tie-members")).not.toHaveTextContent("第一梯队");
  });
});

function item(groupId: string, label: string): ComparisonItem {
  return {
    groupId,
    itemId: `item-${groupId}`,
    primaryLabel: label,
    auxiliaryFields: [{ name: "说明", value: `${label}的说明` }],
    fields: [
      { name: "名称", value: label },
      { name: "说明", value: `${label}的说明` },
    ],
  };
}

function workspace(left: ComparisonItem, right: ComparisonItem): ComparisonWorkspaceState {
  return {
    taskId: "task-1",
    taskName: "方案比较",
    criteria: "更重要的靠前",
    status: "sorting",
    left,
    right,
    locatedCount: 1,
    totalCount: 3,
    comparisonCount: 0,
    estimatedRemaining: 3,
    pendingCount: 2,
    progressPercent: 33,
    canUndo: false,
    completed: false,
    orderedItems: [right],
  };
}
