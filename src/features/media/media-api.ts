import { invoke } from "@tauri-apps/api/core";

export type MediaPreview =
  | { status: "local_ready"; dataUrl: string }
  | { status: "remote_ready"; url: string }
  | { status: "remote_blocked" }
  | { status: "missing" }
  | { status: "too_large" };

export interface MediaFolderImportResult {
  imageFileCount: number;
  matchedItemCount: number;
  matchedAssetCount: number;
  unmatchedItemCount: number;
}

export function importMediaFolder(input: {
  projectPath: string;
  datasetId: string;
  folderPath: string;
}): Promise<MediaFolderImportResult> {
  return invoke("import_media_folder", { input });
}

export function loadMediaPreview(input: {
  projectPath: string;
  itemId: string;
  fieldName: string;
  allowNetwork: boolean;
}): Promise<MediaPreview> {
  return invoke("load_media_preview", { input });
}
