/**
 * 当前打开项目的轻量前端缓存。
 * SQLite 是事实来源；该 store 只让顶层页面与各工作区共享最新概览，不承载持久化逻辑。
 */
import { create } from "zustand";
import type { AppErrorPayload } from "../../lib/errors";
import type { DatasetOverview } from "../import/types";
import type { SortTaskOverview } from "../sort-tasks/types";
import type { ProjectMetadata } from "./types";

interface ProjectState {
  currentProject: ProjectMetadata | null;
  datasets: DatasetOverview[];
  sortTasks: SortTaskOverview[];
  busy: boolean;
  error: AppErrorPayload | null;
  setProject: (project: ProjectMetadata | null) => void;
  setDatasets: (datasets: DatasetOverview[]) => void;
  addDataset: (dataset: DatasetOverview) => void;
  setSortTasks: (tasks: SortTaskOverview[]) => void;
  addSortTask: (task: SortTaskOverview) => void;
  updateSortTask: (task: SortTaskOverview) => void;
  setBusy: (busy: boolean) => void;
  setError: (error: AppErrorPayload | null) => void;
}

export const useProjectStore = create<ProjectState>((set) => ({
  currentProject: null,
  datasets: [],
  sortTasks: [],
  busy: false,
  error: null,
  // 切换项目时先清空上一个项目的派生数据，防止异步加载期间短暂串台。
  setProject: (currentProject) => set({ currentProject, datasets: [], sortTasks: [] }),
  setDatasets: (datasets) => set({ datasets }),
  addDataset: (dataset) => set((state) => ({ datasets: [...state.datasets, dataset] })),
  setSortTasks: (sortTasks) => set({ sortTasks }),
  addSortTask: (task) => set((state) => ({ sortTasks: [...state.sortTasks, task] })),
  updateSortTask: (task) =>
    set((state) => ({
      sortTasks: state.sortTasks.map((current) => (current.id === task.id ? task : current)),
    })),
  setBusy: (busy) => set({ busy }),
  setError: (error) => set({ error }),
}));
