import { invoke } from "@tauri-apps/api/core";
import type {
  ExportFormat,
  ExportOrder,
  ExportReceipt,
  RankRule,
  ResultPreviewState,
  ScoreConfig,
  ScorePreview,
} from "./types";

interface ResultInput {
  projectPath: string;
  taskId: string;
}

export function loadResultPreview(input: ResultInput): Promise<ResultPreviewState> {
  return invoke("load_result_preview", { input });
}

export function confirmSortResult(input: ResultInput): Promise<ResultPreviewState> {
  return invoke("confirm_sort_result", { input });
}

export function unlockSortResult(input: ResultInput): Promise<ResultPreviewState> {
  return invoke("unlock_sort_result", { input });
}

export function writeRankField(
  input: ResultInput & { fieldName: string; overwrite: boolean; rankRule?: RankRule },
): Promise<ResultPreviewState> {
  return invoke("write_rank_field", { input });
}

export function previewScores(
  input: ResultInput & { config: ScoreConfig; overwrite?: boolean },
): Promise<ScorePreview> {
  return invoke("preview_scores", { input });
}

export function loadScoreConfig(input: ResultInput): Promise<ScoreConfig | null> {
  return invoke("load_score_config", { input });
}

export function saveScoreConfig(
  input: ResultInput & { config: ScoreConfig },
): Promise<ScoreConfig> {
  return invoke("save_score_config", { input });
}

export function writeScores(
  input: ResultInput & { config: ScoreConfig; overwrite: boolean },
): Promise<ScorePreview> {
  return invoke("write_scores", { input });
}

export function exportTargetExists(path: string): Promise<boolean> {
  return invoke("export_target_exists", { path });
}

export interface ExportResultInput extends ResultInput {
  targetPath: string;
  format: ExportFormat;
  order: ExportOrder;
  selectedFields: string[];
  includeRank: boolean;
  rankFieldName: string;
  includeOriginalIndex: boolean;
  overwrite: boolean;
}

export function exportSortResult(input: ExportResultInput): Promise<ExportReceipt> {
  return invoke("export_sort_result", { input });
}
