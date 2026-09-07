// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { DragSortWorkspace } from "./DragSortWorkspace";
import { centerPreviewTransform, resolveDropPosition, resolveLocalRankRange } from "./drag-utils";
import {
  loadDragWorkspace,
  mergeRankGroups,
  moveRankGroup,
  previewRankGroupMove,
  redoDragOperation,
  renameRankGroup,
  undoDragOperation,
} from "./sort-task-api";
import type { DragWorkspaceState, RankedItem } from "./types";

class TestPointerEvent extends MouseEvent {
  readonly isPrimary: boolean;
  readonly pointerId: number;

  constructor(type: string, init: PointerEventInit = {}) {
    super(type, init);
    this.isPrimary = init.isPrimary ?? true;
    this.pointerId = init.pointerId ?? 1;
  }
}

Object.defineProperty(window, "PointerEvent", { configurable: true, value: TestPointerEvent });

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 108,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({ index, start: index * 108, size: 108 })),
    scrollToIndex: vi.fn(),
  }),
}));

vi.mock("./sort-task-api", () => ({
  applyLocalRankOrder: vi.fn(),
  loadDragWorkspace: vi.fn(),
  moveRankGroup: vi.fn(),
  previewRankGroupMove: vi.fn(),
  mergeRankGroups: vi.fn(),
  renameRankGroup: vi.fn(),
  splitRankGroupItem: vi.fn(),
  redoDragOperation: vi.fn(),
  undoDragOperation: vi.fn(),
  updateSortTaskCriteria: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe("DragSortWorkspace", () => {
  it("置顶条目、自动保存、搜索并撤销", async () => {
    const initial = workspace([item("a", "A", 0), item("b", "B", 1), item("c", "C", 2)]);
    const moved = {
      ...workspace([item("b", "B", 0), item("a", "A", 1), item("c", "C", 2)]),
      canUndo: true,
    };
    vi.mocked(loadDragWorkspace).mockResolvedValue(initial);
    vi.mocked(moveRankGroup).mockResolvedValue(moved);
    vi.mocked(previewRankGroupMove).mockResolvedValue({ conflictCount: 0, descriptions: [] });
    vi.mocked(undoDragOperation).mockResolvedValue(initial);

    render(
      <DragSortWorkspace
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
          name: "优先级",
          criteria: "越重要越靠前",
          mode: "drag",
          status: "draft",
          initialOrder: { kind: "import" },
          rankGroupCount: 3,
          createdAt: "2026-08-03T00:00:00Z",
        }}
        onReview={vi.fn()}
      />,
    );

    await screen.findByRole("button", { name: "将 B 置顶" });
    const compactHeader = document.querySelector(".drag-compact-header");
    expect(document.querySelector(".project-topbar")).not.toBeInTheDocument();
    expect(screen.queryByText("DRAG WORKSPACE")).not.toBeInTheDocument();
    expect(compactHeader).toContainElement(screen.getByRole("heading", { name: "优先级" }));
    expect(compactHeader).toContainElement(screen.getByText("越重要越靠前"));
    expect(compactHeader).toContainElement(
      screen.getByRole("button", { name: "预览并确认结果 →" }),
    );
    expect(screen.getByText("原始 2")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "A" })).toHaveAttribute("title", "A");
    fireEvent.click(screen.getByRole("button", { name: "将 B 置顶" }));
    await waitFor(() => expect(moveRankGroup).toHaveBeenCalledOnce());
    expect(moveRankGroup).toHaveBeenCalledWith({
      projectPath: "D:\\test.subject-sort",
      taskId: "task-1",
      groupId: "b",
      toPosition: 0,
      conflictResolution: "temporary",
    });

    await waitFor(() => expect(screen.getByRole("button", { name: /撤销/ })).toBeEnabled());
    fireEvent.change(screen.getByPlaceholderText("搜索条目或字段"), {
      target: { value: "C" },
    });
    expect(screen.getByText("找到 1 组")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("搜索条目或字段"), {
      target: { value: "" },
    });
    fireEvent.click(screen.getByRole("button", { name: /撤销/ }));
    await waitFor(() => expect(undoDragOperation).toHaveBeenCalledOnce());
    expect(redoDragOperation).not.toHaveBeenCalled();
  });

  it("拖拽时保留源位置并为其它组显示三段热区", async () => {
    const initial = workspace([item("a", "A", 0), item("b", "B", 1)]);
    vi.mocked(loadDragWorkspace).mockResolvedValue(initial);
    vi.mocked(mergeRankGroups).mockResolvedValue(initial);

    render(
      <DragSortWorkspace
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
          name: "优先级",
          criteria: "越重要越靠前",
          mode: "drag",
          status: "draft",
          initialOrder: { kind: "import" },
          rankGroupCount: 2,
          createdAt: "2026-08-03T00:00:00Z",
        }}
      />,
    );

    await screen.findByRole("button", { name: "拖动 A" });
    const handle = screen.getByRole("button", { name: "拖动 A" });
    const targetTieZone = screen.getAllByText("与此组并列")[1]?.parentElement;
    expect(targetTieZone).not.toBeNull();
    vi.spyOn(targetTieZone as HTMLElement, "getBoundingClientRect").mockReturnValue({
      bottom: 160,
      height: 60,
      left: 20,
      right: 620,
      top: 100,
      width: 600,
      x: 20,
      y: 100,
      toJSON: () => ({}),
    });
    fireEvent.pointerDown(handle, {
      button: 0,
      clientX: 20,
      clientY: 20,
      isPrimary: true,
      pointerId: 1,
    });
    fireEvent.pointerMove(document, {
      clientX: 100,
      clientY: 120,
      isPrimary: true,
      pointerId: 1,
    });

    expect(await screen.findByTestId("rank-drag-preview")).toHaveTextContent("A");
    expect(screen.getByTestId("rank-drag-preview")).not.toHaveTextContent("当前排名");
    expect(document.querySelectorAll(".drag-source-placeholder")).toHaveLength(1);
    expect(document.querySelectorAll(".rank-drop-zones.visible")).toHaveLength(0);
    expect(screen.getAllByText("放在此组前面")).toHaveLength(2);
    expect(screen.getAllByText("与此组并列")).toHaveLength(2);
    expect(screen.getAllByText("放在此组后面")).toHaveLength(2);
    fireEvent.pointerMove(document, {
      clientX: 101,
      clientY: 121,
      isPrimary: true,
      pointerId: 1,
    });
    await waitFor(() => expect(targetTieZone).toHaveClass("active"));
    expect(document.querySelectorAll(".rank-drop-zones.visible")).toHaveLength(1);
    fireEvent.pointerUp(document, {
      clientX: 100,
      clientY: 120,
      isPrimary: true,
      pointerId: 1,
    });
    await waitFor(() =>
      expect(mergeRankGroups).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        sourceGroupId: "a",
        targetGroupId: "b",
      }),
    );
  });

  it("根据目标热区计算移除源组后的准确插入位置", () => {
    expect(resolveDropPosition(0, 2, "before")).toBe(1);
    expect(resolveDropPosition(0, 2, "after")).toBe(2);
    expect(resolveDropPosition(3, 1, "before")).toBe(1);
    expect(resolveDropPosition(3, 1, "after")).toBe(2);
    expect(resolveDropPosition(1, 2, "before")).toBe(1);
    expect(resolveDropPosition(2, 1, "after")).toBe(2);
  });

  it("将连续排名范围解析为完整排名组区间", () => {
    const groups = workspace([item("a", "A", 0), item("b", "B", 1), item("c", "C", 2)]).groups;
    expect(resolveLocalRankRange(groups, 1, 2)).toEqual({ startPosition: 0, endPosition: 1 });
    expect(resolveLocalRankRange(groups, 2, 2)).toBeNull();
    expect(resolveLocalRankRange(groups, 1, 99)).toBeNull();
  });

  it("将紧凑拖拽预览的中心对齐到指针", () => {
    expect(
      centerPreviewTransform(
        { x: 80, y: 100, scaleX: 1, scaleY: 1 },
        { x: 20, y: 20 },
        { left: 0, top: 0 },
        { width: 280, height: 54 },
      ),
    ).toEqual({ x: -40, y: 93, scaleX: 1, scaleY: 1 });
  });

  it("分行显示带圆点的并列项，并支持双击组名", async () => {
    const first = item("tie", "A", 0);
    const second = { ...item("tie", "B", 0), itemId: "item-b" };
    const initial = workspace([first, second]);
    initial.groups = [
      {
        groupId: "tie",
        name: "并列组1",
        position: 0,
        startingRank: 1,
        items: [first, second],
      },
    ];
    const renamed: DragWorkspaceState = {
      ...initial,
      groups: [{ ...initial.groups[0]!, name: "第一梯队" }],
    };
    vi.mocked(loadDragWorkspace).mockResolvedValue(initial);
    vi.mocked(renameRankGroup).mockResolvedValue(renamed);
    vi.spyOn(window, "prompt").mockReturnValue("第一梯队");

    render(
      <DragSortWorkspace
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
          name: "优先级",
          criteria: "越重要越靠前",
          mode: "drag",
          status: "sorting",
          initialOrder: { kind: "import" },
          rankGroupCount: 1,
          createdAt: "2026-08-03T00:00:00Z",
        }}
      />,
    );

    const groupName = await screen.findByText("并列组1");
    expect(document.querySelector(".tie-members.tied")?.children).toHaveLength(2);
    fireEvent.doubleClick(groupName);
    await waitFor(() =>
      expect(renameRankGroup).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        groupId: "tie",
        name: "第一梯队",
      }),
    );
  });
});

function item(groupId: string, label: string, position: number): RankedItem {
  return {
    groupId,
    itemId: `item-${groupId}`,
    position,
    originalPosition: position,
    primaryLabel: label,
    auxiliaryFields: [{ name: "备注", value: `${label}的备注` }],
    fields: [
      { name: "名称", value: label },
      { name: "备注", value: `${label}的备注` },
    ],
  };
}

function workspace(items: RankedItem[]): DragWorkspaceState {
  return {
    taskId: "task-1",
    taskName: "优先级",
    criteria: "越重要越靠前",
    status: "draft",
    items,
    groups: items.map((item, position) => ({
      groupId: item.groupId,
      position,
      startingRank: position + 1,
      items: [item],
    })),
    hasComparisons: false,
    canUndo: false,
    canRedo: false,
  };
}
