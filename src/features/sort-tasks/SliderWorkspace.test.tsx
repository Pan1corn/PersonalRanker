// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { confirmSliderValue, loadSliderWorkspace } from "./slider-api";
import { SliderWorkspace } from "./SliderWorkspace";
import type { ComparisonItem, SliderWorkspaceState } from "./types";

vi.mock("./slider-api", () => ({
  confirmSliderValue: vi.fn(),
  loadSliderWorkspace: vi.fn(),
}));

vi.mock("./result-api", () => ({
  unlockSortResult: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("SliderWorkspace", () => {
  it("将滑杆值限制到两位小数，确认后显示下一对象和已完成记录", async () => {
    const first = item("a", "方案A");
    const second = item("b", "方案B");
    vi.mocked(loadSliderWorkspace).mockResolvedValue(workspace(first));
    vi.mocked(confirmSliderValue).mockResolvedValue({
      ...workspace(second),
      completedCount: 1,
      progressPercent: 50,
      ratings: [{ item: first, value: 87.46 }],
    });

    renderWorkspace();

    await screen.findByRole("heading", { name: "方案A" });
    const slider = screen.getByRole("slider", { name: "当前对象排序位置" });
    fireEvent.change(slider, { target: { value: "87.456" } });
    expect(screen.queryByText("87.46")).not.toBeInTheDocument();
    expect(screen.getByText("具体数值已隐藏")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "显示具体数值" }));
    expect(screen.getByText("87.46")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "确定并继续" }));

    await waitFor(() =>
      expect(confirmSliderValue).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        value: 87.46,
      }),
    );
    expect(await screen.findByRole("heading", { name: "方案B" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "已完成对象" })).toBeInTheDocument();
    expect(screen.getByText("方案A", { selector: ".slider-records strong" })).toBeInTheDocument();
    expect(screen.getByText("87.46", { selector: ".slider-records output" })).toBeInTheDocument();
    expect(slider).toHaveValue("50");
  });

  it("完成全部评分后提供结果预览与拖拽微调入口", async () => {
    const onReview = vi.fn();
    const onFineTune = vi.fn();
    vi.mocked(loadSliderWorkspace).mockResolvedValue({
      ...workspace(undefined),
      completedCount: 2,
      progressPercent: 100,
      completed: true,
      ratings: [
        { item: item("a", "方案A"), value: 90 },
        { item: item("b", "方案B"), value: 90 },
      ],
    });

    renderWorkspace({ onReview, onFineTune });

    expect(await screen.findByRole("heading", { name: "滑杆排序已完成" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "预览并确认结果" }));
    fireEvent.click(screen.getByRole("button", { name: "进入拖拽微调" }));
    expect(onReview).toHaveBeenCalledOnce();
    expect(onFineTune).toHaveBeenCalledOnce();
    expect(screen.queryByText("90.00")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("checkbox", { name: "显示具体数值" }));
    expect(screen.getAllByText("90.00", { selector: ".slider-records output" })).toHaveLength(2);
  });
});

function renderWorkspace({
  onReview,
  onFineTune,
}: {
  onReview?: () => void;
  onFineTune?: () => void;
} = {}) {
  return render(
    <SliderWorkspace
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
        name: "滑杆判断",
        criteria: "越重要越靠前",
        mode: "slider",
        status: "sorting",
        initialOrder: { kind: "import" },
        rankGroupCount: 2,
        createdAt: "2026-08-03T00:00:00Z",
      }}
      onBack={vi.fn()}
      onReview={onReview}
      onFineTune={onFineTune}
    />,
  );
}

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

function workspace(current?: ComparisonItem): SliderWorkspaceState {
  return {
    taskId: "task-1",
    taskName: "滑杆判断",
    criteria: "越重要越靠前",
    status: "sorting",
    current,
    completedCount: 0,
    totalCount: 2,
    progressPercent: 0,
    completed: false,
    ratings: [],
  };
}
