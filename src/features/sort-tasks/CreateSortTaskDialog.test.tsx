// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CreateSortTaskDialog } from "./CreateSortTaskDialog";
import { createSortTask } from "./sort-task-api";
import type { SortTaskOverview } from "./types";

vi.mock("./sort-task-api", () => ({
  createSortTask: vi.fn(),
}));

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe("CreateSortTaskDialog", () => {
  it("展示四种大按钮并创建按数字字段降序预排的 1v1 传递排序任务", async () => {
    const saved: SortTaskOverview = {
      id: "task-1",
      datasetId: "dataset-1",
      name: "优先级排序",
      criteria: "越重要的条目越靠前",
      mode: "comparison",
      status: "draft",
      initialOrder: {
        kind: "field",
        fieldName: "评分",
        direction: "descending",
      },
      rankGroupCount: 3,
      createdAt: "2026-08-03T00:00:00Z",
    };
    vi.mocked(createSortTask).mockResolvedValue(saved);
    const onClose = vi.fn();
    const onCreated = vi.fn();

    render(
      <CreateSortTaskDialog
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        dataset={{
          id: "dataset-1",
          name: "候选事项",
          sourceType: "csv",
          itemCount: 3,
          fields: [
            {
              id: "field-1",
              name: "名称",
              fieldType: "text",
              displayOrder: 0,
              isPrimaryIdentifier: true,
              isAuxiliaryIdentifier: false,
            },
            {
              id: "field-2",
              name: "评分",
              fieldType: "number",
              displayOrder: 1,
              isPrimaryIdentifier: false,
              isAuxiliaryIdentifier: true,
            },
          ],
        }}
        onClose={onClose}
        onCreated={onCreated}
      />,
    );

    expect(screen.getByRole("radio", { name: /拖拽排序/ })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /1v1矩阵排序/ })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /1v1传递排序/ })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /滑杆排序/ })).toBeInTheDocument();
    expect(document.querySelectorAll(".sort-mode-card img")).toHaveLength(4);

    fireEvent.click(screen.getByRole("radio", { name: /1v1矩阵排序/ }));
    expect(screen.getByLabelText("随机抽取对象比例")).toHaveValue("100");
    expect(screen.getByText("将完成全部 3 次两两比较。")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("随机抽取对象比例"), {
      target: { value: "50" },
    });
    expect(screen.getByText(/实际次数取决于随机对象对/)).toBeInTheDocument();

    fireEvent.change(screen.getByLabelText("任务名称"), {
      target: { value: "优先级排序" },
    });
    fireEvent.change(screen.getByLabelText("排序标准"), {
      target: { value: "越重要的条目越靠前" },
    });
    fireEvent.click(screen.getByRole("radio", { name: /1v1传递排序/ }));
    expect(screen.getByText(/约 3 次二分比较/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("radio", { name: /按字段预排序/ }));
    fireEvent.change(screen.getByLabelText("排序字段"), { target: { value: "评分" } });
    fireEvent.change(screen.getByLabelText("方向"), { target: { value: "descending" } });
    fireEvent.click(screen.getByRole("button", { name: "创建任务" }));

    await waitFor(() => expect(createSortTask).toHaveBeenCalledOnce());
    expect(createSortTask).toHaveBeenCalledWith({
      projectPath: "D:\\test.subject-sort",
      datasetId: "dataset-1",
      name: "优先级排序",
      criteria: "越重要的条目越靠前",
      mode: "comparison",
      initialOrder: {
        kind: "field",
        fieldName: "评分",
        direction: "descending",
      },
    });
    expect(onCreated).toHaveBeenCalledWith(saved);
    expect(onClose).toHaveBeenCalledOnce();
  });

  it("从已锁定结果进入时默认沿用该结果", () => {
    render(
      <CreateSortTaskDialog
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-03T00:00:00Z",
          lastOpenedAt: "2026-08-03T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        dataset={{
          id: "dataset-1",
          name: "候选事项",
          sourceType: "csv",
          itemCount: 3,
          fields: [],
        }}
        sourceTasks={[
          {
            id: "source-1",
            datasetId: "dataset-1",
            name: "已锁定优先级",
            criteria: "旧标准",
            mode: "drag",
            status: "confirmed",
            initialOrder: { kind: "import" },
            rankGroupCount: 2,
            createdAt: "2026-08-03T00:00:00Z",
          },
        ]}
        initialSourceTaskId="source-1"
        onClose={vi.fn()}
        onCreated={vi.fn()}
      />,
    );

    expect(screen.getByText(/将已锁定结果“已锁定优先级”视作新数据集/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /沿用已完成任务/ })).toBeChecked();
    expect(screen.getByLabelText("来源任务")).toHaveValue("source-1");
  });
});
