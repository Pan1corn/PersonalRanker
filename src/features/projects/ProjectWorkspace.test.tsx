// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { confirm } from "@tauri-apps/plugin-dialog";
import { updateSortTaskCriteria } from "../sort-tasks/sort-task-api";
import { getAutosaveStatus, listProjectSnapshots, restoreProjectSnapshot } from "./history-api";
import { ProjectWorkspace } from "./ProjectWorkspace";

vi.mock("@tauri-apps/plugin-dialog", () => ({
  confirm: vi.fn(),
  open: vi.fn(),
}));

vi.mock("./history-api", () => ({
  getAutosaveStatus: vi.fn(),
  listProjectSnapshots: vi.fn(),
  restoreProjectSnapshot: vi.fn(),
}));

vi.mock("../sort-tasks/sort-task-api", () => ({
  createSortTask: vi.fn(),
  deleteSortTask: vi.fn(),
  updateSortTaskCriteria: vi.fn(),
}));

vi.mock("../sort-tasks/DragSortWorkspace", () => ({
  DragSortWorkspace: ({ task }: { task: { name: string } }) => <div>已打开 {task.name}</div>,
}));

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  vi.clearAllMocks();
});

describe("ProjectWorkspace", () => {
  it("shows crash recovery and restores a selected snapshot", async () => {
    const status = {
      sessionOpen: true,
      recoveredUncleanSession: true,
      lastAutosaveAt: "2026-08-09T06:00:00Z",
      lastAutosaveAction: "调整拖拽排序位置",
    };
    const snapshot = {
      id: "snapshot-1",
      snapshotType: "sort_confirmed" as const,
      label: "排序结果确认",
      sourceTaskId: "task-1",
      createdAt: "2026-08-09T05:30:00Z",
      sizeBytes: 2048,
    };
    vi.mocked(getAutosaveStatus).mockResolvedValue(status);
    vi.mocked(listProjectSnapshots).mockResolvedValue([snapshot]);
    vi.mocked(confirm).mockResolvedValue(true);
    vi.mocked(restoreProjectSnapshot).mockResolvedValue({ snapshot, autosave: status });
    const onProjectDataChanged = vi.fn().mockResolvedValue(undefined);
    const onAllowNetworkImagesChange = vi.fn();

    render(
      <ProjectWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-09T00:00:00Z",
          lastOpenedAt: "2026-08-09T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        datasets={[]}
        sortTasks={[]}
        onDatasetImported={vi.fn()}
        onSortTaskCreated={vi.fn()}
        onProjectDataChanged={onProjectDataChanged}
        onCloseProject={vi.fn()}
        initialAutosave={status}
        onAllowNetworkImagesChange={onAllowNetworkImagesChange}
      />,
    );

    expect(screen.getByText("已恢复上次异常关闭前的自动保存状态")).toBeInTheDocument();
    expect(screen.getByText("测试项目")).toBeInTheDocument();
    expect(screen.getByTitle("D:\\test.subject-sort")).toBeInTheDocument();
    expect(screen.queryByText("主观排序器")).not.toBeInTheDocument();
    expect(screen.queryByText("项目数据")).not.toBeInTheDocument();
    expect(screen.queryByText("加载网络图片")).not.toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "设置" }));
    const networkImages = screen.getByRole("checkbox", { name: /加载网络图片/ });
    fireEvent.click(networkImages);
    expect(onAllowNetworkImagesChange).toHaveBeenCalledWith(true);
    fireEvent.click(screen.getByRole("button", { name: "完成" }));
    fireEvent.click(screen.getByRole("button", { name: /历史快照/ }));
    await screen.findByText("排序结果确认");
    fireEvent.click(screen.getByRole("button", { name: "恢复" }));

    await waitFor(() =>
      expect(restoreProjectSnapshot).toHaveBeenCalledWith("D:\\test.subject-sort", "snapshot-1"),
    );
    expect(onProjectDataChanged).toHaveBeenCalledOnce();
  });

  it("lays out each dataset before its horizontally grouped sort tasks", () => {
    vi.mocked(getAutosaveStatus).mockResolvedValue({
      sessionOpen: true,
      recoveredUncleanSession: false,
    });

    const { container } = render(
      <ProjectWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-09T00:00:00Z",
          lastOpenedAt: "2026-08-09T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        datasets={[
          {
            id: "dataset-1",
            name: "选题库",
            sourceType: "csv",
            itemCount: 294,
            fields: [
              {
                id: "field-1",
                name: "选题简述",
                fieldType: "text",
                displayOrder: 0,
                isPrimaryIdentifier: true,
                isAuxiliaryIdentifier: false,
              },
            ],
          },
        ]}
        sortTasks={[
          {
            id: "task-1",
            datasetId: "dataset-1",
            name: "选题库排序",
            criteria: "按优先级排序",
            mode: "drag",
            status: "sorting",
            initialOrder: { kind: "import" },
            rankGroupCount: 292,
            createdAt: "2026-08-09T00:00:00Z",
          },
        ]}
        onDatasetImported={vi.fn()}
        onSortTaskCreated={vi.fn()}
        onProjectDataChanged={vi.fn().mockResolvedValue(undefined)}
        onCloseProject={vi.fn()}
        initialAutosave={null}
      />,
    );

    const row = container.querySelector(".dataset-row");
    expect(row?.firstElementChild).toHaveClass("dataset-summary");
    expect(row?.lastElementChild).toHaveClass("dataset-row-tasks");
    expect(row?.querySelectorAll(".dataset-task-card")).toHaveLength(1);
    expect(screen.getByRole("button", { name: /新建排序任务/ })).toBeInTheDocument();
    const taskCard = row?.querySelector(".dataset-task-card");
    expect(taskCard).toHaveAttribute("tabindex", "0");
    expect(taskCard?.querySelector(".dataset-task-open")?.tagName).toBe("DIV");
    expect(taskCard?.querySelector(".dataset-task-stats")).toHaveTextContent("292 个排序组");
    expect(taskCard?.querySelector(".dataset-task-stats")).not.toHaveTextContent("拖拽排序");
    expect(taskCard?.querySelector(".dataset-task-actions")).not.toHaveTextContent("打开");
    expect(taskCard?.querySelector(".dataset-task-actions .danger")).toHaveTextContent("删除");

    fireEvent.click(taskCard!);
    expect(screen.getByText("已打开 选题库排序")).toBeInTheDocument();
  });

  it("允许修改任务标准，并从已锁定结果直接创建新任务", async () => {
    vi.mocked(getAutosaveStatus).mockResolvedValue({
      sessionOpen: true,
      recoveredUncleanSession: false,
    });
    const confirmedTask = {
      id: "task-1",
      datasetId: "dataset-1",
      name: "已确认排序",
      criteria: "旧标准",
      mode: "drag" as const,
      status: "confirmed" as const,
      initialOrder: { kind: "import" as const },
      rankGroupCount: 2,
      createdAt: "2026-08-09T00:00:00Z",
    };
    vi.mocked(updateSortTaskCriteria).mockResolvedValue({
      ...confirmedTask,
      criteria: "新的标准",
    });
    vi.spyOn(window, "prompt").mockReturnValue("新的标准");

    render(
      <ProjectWorkspace
        project={{
          id: "project-1",
          name: "测试项目",
          createdAt: "2026-08-09T00:00:00Z",
          lastOpenedAt: "2026-08-09T00:00:00Z",
          dataFormatVersion: 1,
          projectPath: "D:\\test.subject-sort",
        }}
        datasets={[
          {
            id: "dataset-1",
            name: "选题库",
            sourceType: "csv",
            itemCount: 3,
            fields: [],
          },
        ]}
        sortTasks={[confirmedTask]}
        onDatasetImported={vi.fn()}
        onSortTaskCreated={vi.fn()}
        onProjectDataChanged={vi.fn().mockResolvedValue(undefined)}
        onCloseProject={vi.fn()}
        initialAutosave={null}
      />,
    );

    const taskCard = document.querySelector(".dataset-task-card");
    const lock = taskCard?.querySelector(".dataset-task-lock");
    expect(lock).toHaveTextContent("🔒");
    expect(lock).not.toHaveTextContent("已锁定");
    expect(taskCard?.querySelector(".dataset-task-actions")).not.toHaveTextContent("打开");

    fireEvent.click(screen.getByRole("button", { name: "修改标准" }));
    await waitFor(() =>
      expect(updateSortTaskCriteria).toHaveBeenCalledWith({
        projectPath: "D:\\test.subject-sort",
        taskId: "task-1",
        criteria: "新的标准",
      }),
    );

    fireEvent.click(screen.getByRole("button", { name: /将当前锁定结果视作新数据集/ }));
    expect(await screen.findByText(/将已锁定结果“已确认排序”视作新数据集/)).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: /沿用已完成任务/ })).toBeChecked();
    expect(screen.getByLabelText("来源任务")).toHaveValue("task-1");
  });
});
