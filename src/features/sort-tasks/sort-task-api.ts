import { invoke } from "@tauri-apps/api/core";
import type {
  DragWorkspaceState,
  InitialOrder,
  MoveConflictPreview,
  MoveConflictResolution,
  SortTaskMode,
  SortTaskOverview,
} from "./types";

export interface CreateSortTaskInput {
  projectPath: string;
  datasetId: string;
  name: string;
  criteria: string;
  mode: SortTaskMode;
  initialOrder: InitialOrder;
  matrixComparisonPercent?: number;
}

export function createSortTask(input: CreateSortTaskInput): Promise<SortTaskOverview> {
  return invoke("create_sort_task", { input });
}

export function listSortTasks(projectPath: string): Promise<SortTaskOverview[]> {
  return invoke("list_sort_tasks", { projectPath });
}

export function deleteSortTask(projectPath: string, taskId: string): Promise<void> {
  return invoke("delete_sort_task", { projectPath, taskId });
}

export function updateSortTaskCriteria(
  input: Pick<CreateSortTaskInput, "projectPath" | "criteria"> & { taskId: string },
): Promise<SortTaskOverview> {
  return invoke("update_sort_task_criteria", { input });
}

interface DragWorkspaceInput {
  projectPath: string;
  taskId: string;
}

export function loadDragWorkspace(input: DragWorkspaceInput): Promise<DragWorkspaceState> {
  return invoke("load_drag_workspace", { input });
}

export function moveRankGroup(
  input: DragWorkspaceInput & {
    groupId: string;
    toPosition: number;
    conflictResolution?: MoveConflictResolution;
  },
): Promise<DragWorkspaceState> {
  return invoke("move_rank_group", { input });
}

export function applyLocalRankOrder(
  input: DragWorkspaceInput & { startPosition: number; orderedGroupIds: string[] },
): Promise<DragWorkspaceState> {
  return invoke("apply_local_rank_order", { input });
}

export function previewRankGroupMove(
  input: DragWorkspaceInput & { groupId: string; toPosition: number },
): Promise<MoveConflictPreview> {
  return invoke("preview_rank_group_move", { input });
}

export function mergeRankGroups(
  input: DragWorkspaceInput & { sourceGroupId: string; targetGroupId: string },
): Promise<DragWorkspaceState> {
  return invoke("merge_rank_groups", { input });
}

export function splitRankGroupItem(
  input: DragWorkspaceInput & { groupId: string; itemId: string },
): Promise<DragWorkspaceState> {
  return invoke("split_rank_group_item", { input });
}

export function renameRankGroup(
  input: DragWorkspaceInput & { groupId: string; name: string },
): Promise<DragWorkspaceState> {
  return invoke("rename_rank_group", { input });
}

export function undoDragOperation(input: DragWorkspaceInput): Promise<DragWorkspaceState> {
  return invoke("undo_drag_operation", { input });
}

export function redoDragOperation(input: DragWorkspaceInput): Promise<DragWorkspaceState> {
  return invoke("redo_drag_operation", { input });
}
