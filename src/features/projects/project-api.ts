import { invoke } from "@tauri-apps/api/core";
import type { OpenProjectResult, ProjectMetadata } from "./types";

export function createProject(name: string, parentDirectory: string): Promise<ProjectMetadata> {
  return invoke("create_project", { input: { name, parentDirectory } });
}

export function openProject(projectPath: string): Promise<OpenProjectResult> {
  return invoke("open_project", { projectPath });
}

export function closeProject(projectPath: string): Promise<void> {
  return invoke("close_project", { projectPath });
}
