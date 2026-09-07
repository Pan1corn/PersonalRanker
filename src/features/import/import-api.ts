import { invoke } from "@tauri-apps/api/core";
import type { DatasetOverview, ImportResult, JsonArrayNode } from "./types";

export interface ImportFileInput {
  path: string;
  deduplicate?: boolean;
  /** RFC 6901 JSON Pointer；根数组使用空字符串。 */
  jsonPointer?: string;
}

export function previewImportFile(input: ImportFileInput): Promise<ImportResult> {
  return invoke("preview_import_file", { input });
}

export function previewPastedText(content: string, deduplicate = false): Promise<ImportResult> {
  return invoke("preview_pasted_text", { input: { content, deduplicate } });
}

export function inspectJsonArrayNodes(path: string): Promise<JsonArrayNode[]> {
  return invoke("inspect_json_array_nodes", { path });
}

export type CommitImportSource =
  { kind: "file"; path: string } | { kind: "pasted_text"; content: string };

export interface CommitImportInput {
  projectPath: string;
  datasetName: string;
  source: CommitImportSource;
  deduplicate: boolean;
  jsonPointer?: string;
  primaryIdentifier: string;
  auxiliaryIdentifiers: string[];
}

export function commitDatasetImport(input: CommitImportInput): Promise<DatasetOverview> {
  return invoke("commit_dataset_import", { input });
}

export function listDatasets(projectPath: string): Promise<DatasetOverview[]> {
  return invoke("list_datasets", { projectPath });
}

export function deleteDataset(projectPath: string, datasetId: string): Promise<void> {
  return invoke("delete_dataset", { projectPath, datasetId });
}
