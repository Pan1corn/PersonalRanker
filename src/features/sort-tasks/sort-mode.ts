import comparisonIcon from "../../assets/sort-modes/comparison.png";
import dragIcon from "../../assets/sort-modes/drag.png";
import matrixIcon from "../../assets/sort-modes/matrix.png";
import sliderIcon from "../../assets/sort-modes/slider.png";
import type { SortTaskMode } from "./types";

export interface SortModeDefinition {
  mode: SortTaskMode;
  label: string;
  shortLabel: string;
  description: string;
  icon: string;
}

export const SORT_MODE_DEFINITIONS: readonly SortModeDefinition[] = [
  {
    mode: "drag",
    label: "拖拽排序",
    shortLabel: "拖拽",
    description: "直接拖动条目，快速调整整体顺序",
    icon: dragIcon,
  },
  {
    mode: "matrix",
    label: "1v1矩阵排序",
    shortLabel: "1v1矩阵",
    description: "逐一完成对象之间的两两比较",
    icon: matrixIcon,
  },
  {
    mode: "comparison",
    label: "1v1传递排序",
    shortLabel: "1v1传递",
    description: "利用传递关系逐步确定对象顺序",
    icon: comparisonIcon,
  },
  {
    mode: "slider",
    label: "滑杆排序",
    shortLabel: "滑杆",
    description: "用滑杆为每个对象设置精确位置",
    icon: sliderIcon,
  },
] as const;

export function getSortModeDefinition(mode: SortTaskMode): SortModeDefinition {
  return SORT_MODE_DEFINITIONS.find((definition) => definition.mode === mode)!;
}
