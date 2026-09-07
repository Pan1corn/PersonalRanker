import type { RankedGroup } from "./types";

/**
 * 把“相对目标卡片的前/后”换算成移除源卡片后的数组下标。
 * 源卡片位于目标之前时，移除动作会让目标下标左移一位。
 */
export function resolveDropPosition(
  fromPosition: number,
  targetPosition: number,
  placement: "before" | "after",
): number {
  if (fromPosition < 0 || targetPosition < 0 || fromPosition === targetPosition) return -1;
  if (placement === "before") {
    return targetPosition - (fromPosition < targetPosition ? 1 : 0);
  }
  return targetPosition + (fromPosition > targetPosition ? 1 : 0);
}

export function resolveLocalRankRange(
  groups: RankedGroup[],
  startRank: number,
  endRank: number,
): { startPosition: number; endPosition: number } | null {
  // 一个并列组会占据多个名次，输入名次必须先映射回完整的组边界。
  const startPosition = groups.findIndex(
    (group) =>
      startRank >= group.startingRank && startRank < group.startingRank + group.items.length,
  );
  const endPosition = groups.findIndex(
    (group) => endRank >= group.startingRank && endRank < group.startingRank + group.items.length,
  );
  if (startPosition < 0 || endPosition - startPosition < 1) return null;
  return { startPosition, endPosition };
}

export function centerPreviewTransform<T extends { x: number; y: number }>(
  transform: T,
  pointerStart: { x: number; y: number },
  activeOrigin: { left: number; top: number },
  previewSize: { width: number; height: number },
): T {
  // dnd-kit 默认以原节点左上角为基准；补偿偏移后，紧凑预览会以指针为中心。
  return {
    ...transform,
    x: transform.x + pointerStart.x - activeOrigin.left - previewSize.width / 2,
    y: transform.y + pointerStart.y - activeOrigin.top - previewSize.height / 2,
  };
}
