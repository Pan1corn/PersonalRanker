export type SortTaskMode = "drag" | "matrix" | "comparison" | "slider";
export type SortTaskStatus = "draft" | "sorting" | "confirmed";
export type SortDirection = "ascending" | "descending";

export type InitialOrder =
  | { kind: "import" }
  | { kind: "field"; fieldName: string; direction: SortDirection }
  | { kind: "task_result"; taskId: string };

export interface SortTaskOverview {
  id: string;
  datasetId: string;
  name: string;
  criteria: string;
  mode: SortTaskMode;
  status: SortTaskStatus;
  initialOrder: InitialOrder;
  matrixComparisonPercent?: number;
  rankGroupCount: number;
  createdAt: string;
}

export interface SortDisplayField {
  name: string;
  value: unknown;
}

export interface RankedItem {
  groupId: string;
  itemId: string;
  position: number;
  originalPosition: number;
  primaryLabel: string;
  auxiliaryFields: SortDisplayField[];
  fields: SortDisplayField[];
}

export interface DragWorkspaceState {
  taskId: string;
  taskName: string;
  criteria: string;
  status: SortTaskStatus;
  items: RankedItem[];
  groups: RankedGroup[];
  hasComparisons: boolean;
  canUndo: boolean;
  canRedo: boolean;
}

export interface RankedGroup {
  groupId: string;
  name?: string;
  position: number;
  startingRank: number;
  items: RankedItem[];
}

export type MoveConflictResolution = "update_relations" | "temporary";

export interface MoveConflictPreview {
  conflictCount: number;
  descriptions: string[];
}

export type ComparisonDecision = "left_before" | "right_before" | "tie";

export interface ComparisonItem {
  groupId: string;
  itemId: string;
  itemIds?: string[];
  memberLabels?: string[];
  groupName?: string;
  primaryLabel: string;
  auxiliaryFields: SortDisplayField[];
  fields: SortDisplayField[];
}

export interface ComparisonWorkspaceState {
  taskId: string;
  taskName: string;
  criteria: string;
  status: SortTaskStatus;
  mode?: SortTaskMode;
  left?: ComparisonItem;
  right?: ComparisonItem;
  locatedCount: number;
  totalCount: number;
  comparisonCount: number;
  estimatedRemaining: number;
  pendingCount: number;
  progressPercent: number;
  canUndo: boolean;
  completed: boolean;
  orderedItems: ComparisonItem[];
  plannedComparisonCount?: number;
  matrixComparisonPercent?: number;
  standings?: MatrixStanding[];
}

export interface MatrixStanding {
  item: ComparisonItem;
  wins: number;
  losses: number;
  ties: number;
  total: number;
}

export interface SliderRating {
  item: ComparisonItem;
  value: number;
}

export interface SliderWorkspaceState {
  taskId: string;
  taskName: string;
  criteria: string;
  status: SortTaskStatus;
  current?: ComparisonItem;
  completedCount: number;
  totalCount: number;
  progressPercent: number;
  completed: boolean;
  ratings: SliderRating[];
}

export interface ResultIntegrity {
  allItemsIncluded: boolean;
  noUnresolvedComparisons: boolean;
  noDuplicateItems: boolean;
  noInvalidItems: boolean;
  problems: string[];
}

export interface ResultItem {
  groupId: string;
  groupName?: string;
  tieSize?: number;
  itemId: string;
  rank: number;
  originalRank: number;
  rankChange: number;
  primaryLabel: string;
  fields: SortDisplayField[];
}

export interface ResultPreviewState {
  taskId: string;
  taskName: string;
  status: SortTaskStatus;
  items: ResultItem[];
  integrity: ResultIntegrity;
  canConfirm: boolean;
  confirmedAt?: string;
  rankWrittenAt?: string;
  rankWrittenField?: string;
}

export type ExportFormat = "csv" | "json" | "markdown_table" | "markdown_list" | "text";
export type ExportOrder = "final" | "original";

export interface ExportReceipt {
  path: string;
  rowCount: number;
  format: ExportFormat;
}

export type RankRule = "competition" | "dense" | "ordinal";
export type TieScoreRule = "average" | "highest" | "lowest";

export type ScoreMethod =
  | {
      kind: "linear";
      highestScore: number;
      lowestScore: number;
      decimalPlaces: number;
      highRankHighScore: boolean;
    }
  | { kind: "buckets"; levels: number[] };

export interface ScoreConfig {
  fieldName: string;
  method: ScoreMethod;
  tieRule: TieScoreRule;
}

export interface ScorePreviewItem {
  groupId: string;
  groupName?: string;
  itemId: string;
  primaryLabel: string;
  rank: number;
  score: number;
  oldValue: unknown;
}

export interface ScorePreview {
  taskId: string;
  fieldName: string;
  fieldExists: boolean;
  canWrite: boolean;
  items: ScorePreviewItem[];
}
