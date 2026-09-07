import { invoke } from "@tauri-apps/api/core";
import type { ComparisonDecision, ComparisonWorkspaceState } from "./types";

interface ComparisonWorkspaceInput {
  projectPath: string;
  taskId: string;
}

export function loadComparisonWorkspace(
  input: ComparisonWorkspaceInput,
): Promise<ComparisonWorkspaceState> {
  return invoke("load_comparison_workspace", { input });
}

export function answerComparison(
  input: ComparisonWorkspaceInput & { decision: ComparisonDecision },
): Promise<ComparisonWorkspaceState> {
  return invoke("answer_comparison", { input });
}

export function skipComparison(input: ComparisonWorkspaceInput): Promise<ComparisonWorkspaceState> {
  return invoke("skip_comparison", { input });
}

export function undoComparison(input: ComparisonWorkspaceInput): Promise<ComparisonWorkspaceState> {
  return invoke("undo_comparison", { input });
}

export function renameComparisonRankGroup(
  input: ComparisonWorkspaceInput & { groupId: string; name: string },
): Promise<ComparisonWorkspaceState> {
  return invoke("rename_comparison_rank_group", { input });
}
