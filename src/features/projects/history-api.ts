import { invoke } from "@tauri-apps/api/core";
import type { AutosaveStatus, ProjectSnapshot, RestoreSnapshotResult } from "./types";

interface ProjectHistoryInput {
  projectPath: string;
}

export function getAutosaveStatus(projectPath: string): Promise<AutosaveStatus> {
  return invoke("get_autosave_status", { input: { projectPath } satisfies ProjectHistoryInput });
}

export function recordAutosave(projectPath: string, action: string): Promise<AutosaveStatus> {
  return invoke("record_autosave", { input: { projectPath, action } });
}

export function listProjectSnapshots(projectPath: string): Promise<ProjectSnapshot[]> {
  return invoke("list_project_snapshots", { input: { projectPath } });
}

export function restoreProjectSnapshot(
  projectPath: string,
  snapshotId: string,
): Promise<RestoreSnapshotResult> {
  return invoke("restore_project_snapshot", { input: { projectPath, snapshotId } });
}
