/**
 * 拖拽排序工作台。
 * 界面按“排名组”而非单条记录移动，以便并列组在拖动、撤销和持久化时保持原子性。
 */
import * as Dialog from "@radix-ui/react-dialog";
import { message } from "@tauri-apps/plugin-dialog";
import {
  DndContext,
  DragOverlay,
  KeyboardSensor,
  MeasuringStrategy,
  PointerSensor,
  closestCenter,
  pointerWithin,
  useDndContext,
  useDraggable,
  useDroppable,
  useSensor,
  useSensors,
  type CollisionDetection,
  type DragEndEvent,
  type Modifier,
} from "@dnd-kit/core";
import { arrayMove } from "@dnd-kit/sortable";
import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import { MediaThumbnail } from "../media/MediaThumbnail";
import type { ProjectMetadata } from "../projects/types";
import { centerPreviewTransform, resolveDropPosition, resolveLocalRankRange } from "./drag-utils";
import { EditableSortCriteria } from "./EditableSortCriteria";
import {
  applyLocalRankOrder,
  loadDragWorkspace,
  mergeRankGroups,
  moveRankGroup,
  previewRankGroupMove,
  redoDragOperation,
  renameRankGroup,
  splitRankGroupItem,
  undoDragOperation,
  updateSortTaskCriteria,
} from "./sort-task-api";
import { unlockSortResult } from "./result-api";
import type {
  DragWorkspaceState,
  MoveConflictResolution,
  RankedGroup,
  RankedItem,
  SortTaskOverview,
} from "./types";

const rankCollisionDetection: CollisionDetection = (arguments_) => {
  // 指针输入优先命中三段式热区；键盘输入没有指针坐标，退回几何中心算法。
  if (arguments_.pointerCoordinates) {
    return pointerWithin(arguments_);
  }
  return closestCenter(arguments_);
};

type DropPlacement = "before" | "tie" | "after";

interface RankDropData {
  type: "rank-drop-zone";
  groupId: string;
  placement: DropPlacement;
}

const centerCompactPreviewOnPointer: Modifier = ({
  activatorEvent,
  draggingNodeRect,
  transform,
}) => {
  if (
    !activatorEvent ||
    !draggingNodeRect ||
    !("clientX" in activatorEvent) ||
    !("clientY" in activatorEvent)
  ) {
    return transform;
  }
  return centerPreviewTransform(
    transform,
    { x: Number(activatorEvent.clientX), y: Number(activatorEvent.clientY) },
    { left: draggingNodeRect.left, top: draggingNodeRect.top },
    {
      width: draggingNodeRect.width,
      height: draggingNodeRect.height,
    },
  );
};

interface Props {
  project: ProjectMetadata;
  task: SortTaskOverview;
  onReview?: () => void;
  allowNetworkImages?: boolean;
  mediaFieldNames?: string[];
  onProjectDataChanged?: () => Promise<void>;
}

interface LocalRankRange {
  startPosition: number;
  endPosition: number;
  // 局部编辑期间只在内存中重排，保留原列表用于取消时无损回滚。
  originalGroups: RankedGroup[];
}

export function DragSortWorkspace({
  project,
  task,
  onReview,
  allowNetworkImages = false,
  mediaFieldNames = [],
  onProjectDataChanged,
}: Props) {
  const [workspace, setWorkspace] = useState<DragWorkspaceState | null>(null);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [busy, setBusy] = useState(false);
  const [search, setSearch] = useState("");
  const [rank, setRank] = useState("");
  const [localStartRank, setLocalStartRank] = useState("");
  const [localEndRank, setLocalEndRank] = useState("");
  const [localRange, setLocalRange] = useState<LocalRankRange | null>(null);
  const [showAuxiliary, setShowAuxiliary] = useState(true);
  const [activeGroupId, setActiveGroupId] = useState<string | null>(null);
  const [detailItem, setDetailItem] = useState<RankedItem | null>(null);
  const scrollRef = useRef<HTMLDivElement>(null);
  const localStartRef = useRef<HTMLInputElement>(null);
  const localEndRef = useRef<HTMLInputElement>(null);
  const sensors = useSensors(
    useSensor(PointerSensor, { activationConstraint: { distance: 5 } }),
    useSensor(KeyboardSensor),
  );

  useEffect(() => {
    let active = true;
    setError(null);
    void loadDragWorkspace({ projectPath: project.projectPath, taskId: task.id })
      .then((state) => active && setWorkspace(state))
      .catch((cause) => active && setError(normalizeAppError(cause)));
    return () => {
      active = false;
    };
  }, [project.projectPath, task.id]);

  const filteredGroups = useMemo(() => {
    if (!workspace) return [];
    if (localRange) {
      return workspace.groups.slice(localRange.startPosition, localRange.endPosition + 1);
    }
    const query = search.trim().toLocaleLowerCase();
    if (!query) return workspace.groups;
    return workspace.groups.filter((group) =>
      [
        group.name ?? "",
        ...group.items.flatMap((item) => [
          item.primaryLabel,
          ...item.fields.map((field) => valueText(field.value)),
        ]),
      ]
        .join(" ")
        .toLocaleLowerCase()
        .includes(query),
    );
  }, [localRange, search, workspace]);
  const activeGroup = useMemo(
    () => workspace?.groups.find((group) => group.groupId === activeGroupId) ?? null,
    [activeGroupId, workspace],
  );

  const virtualizer = useVirtualizer({
    count: filteredGroups.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: (index) => {
      const memberCount = filteredGroups[index]?.items.length ?? 1;
      return (showAuxiliary ? 104 : 78) + Math.max(0, memberCount - 1) * 48;
    },
    overscan: 8,
  });

  const persistMove = useCallback(
    async (groupId: string, toPosition: number) => {
      if (!workspace || busy || workspace.status === "confirmed") return;
      const fromPosition = workspace.groups.findIndex((group) => group.groupId === groupId);
      if (fromPosition < 0 || fromPosition === toPosition) return;
      if (localRange) {
        // 局部模式暂不逐步写库，结束时会把整个区间作为一次可撤销操作提交。
        if (toPosition < localRange.startPosition || toPosition > localRange.endPosition) return;
        const groups = recalculateGroupRanks(arrayMove(workspace.groups, fromPosition, toPosition));
        setWorkspace({ ...workspace, groups, items: groups.flatMap((group) => group.items) });
        return;
      }
      const previous = workspace;
      setBusy(true);
      setError(null);
      try {
        const conflict = await previewRankGroupMove({
          projectPath: project.projectPath,
          taskId: task.id,
          groupId,
          toPosition,
        });
        let conflictResolution: MoveConflictResolution = "temporary";
        if (conflict.conflictCount > 0) {
          const details = conflict.descriptions.length
            ? `\n\n${conflict.descriptions.join("\n")}`
            : "";
          const choice = await message(
            `本次移动与 ${conflict.conflictCount} 条既有判断冲突。${details}`,
            {
              title: "拖拽与比较关系冲突",
              kind: "warning",
              buttons: { yes: "以拖拽为准", no: "仅临时移动", cancel: "取消移动" },
            },
          );
          if (choice === "Cancel" || choice === "取消移动") return;
          conflictResolution =
            choice === "Yes" || choice === "以拖拽为准" ? "update_relations" : "temporary";
        }
        // 先更新界面降低拖动后的等待感；后端失败时恢复 previous 快照。
        const optimisticGroups = arrayMove(workspace.groups, fromPosition, toPosition).map(
          (group, position) => ({ ...group, position }),
        );
        setWorkspace({ ...workspace, groups: optimisticGroups });
        setWorkspace(
          await moveRankGroup({
            projectPath: project.projectPath,
            taskId: task.id,
            groupId,
            toPosition,
            conflictResolution,
          }),
        );
      } catch (cause) {
        setWorkspace(previous);
        setError(normalizeAppError(cause));
      } finally {
        setBusy(false);
      }
    },
    [busy, localRange, project.projectPath, task.id, workspace],
  );

  const applyHistory = useCallback(
    async (kind: "undo" | "redo") => {
      if (!workspace || busy || localRange || workspace.status === "confirmed") return;
      setBusy(true);
      setError(null);
      try {
        const input = { projectPath: project.projectPath, taskId: task.id };
        setWorkspace(
          kind === "undo" ? await undoDragOperation(input) : await redoDragOperation(input),
        );
      } catch (cause) {
        setError(normalizeAppError(cause));
      } finally {
        setBusy(false);
      }
    },
    [busy, localRange, project.projectPath, task.id, workspace],
  );

  useEffect(() => {
    function handleKeyboard(event: KeyboardEvent) {
      if (!(event.ctrlKey || event.metaKey) || event.key.toLocaleLowerCase() !== "z") return;
      event.preventDefault();
      void applyHistory(event.shiftKey ? "redo" : "undo");
    }
    window.addEventListener("keydown", handleKeyboard);
    return () => window.removeEventListener("keydown", handleKeyboard);
  }, [applyHistory]);

  function handleDragEnd(event: DragEndEvent) {
    setActiveGroupId(null);
    if (!workspace || !event.over) return;
    const sourceGroupId = String(event.active.id);
    const drop = event.over.data.current as RankDropData | undefined;
    if (drop?.type !== "rank-drop-zone" || sourceGroupId === drop.groupId) return;
    if (drop.placement === "tie") {
      if (localRange) return;
      void applyGroupChange(() =>
        mergeRankGroups({
          projectPath: project.projectPath,
          taskId: task.id,
          sourceGroupId,
          targetGroupId: drop.groupId,
        }),
      );
      return;
    }
    const fromPosition = workspace.groups.findIndex((group) => group.groupId === sourceGroupId);
    const targetPosition = workspace.groups.findIndex((group) => group.groupId === drop.groupId);
    const toPosition = resolveDropPosition(fromPosition, targetPosition, drop.placement);
    if (toPosition >= 0) void persistMove(sourceGroupId, toPosition);
  }

  function jumpToRank() {
    if (!workspace) return;
    const target = Number.parseInt(rank, 10) - 1;
    const groupIndex = workspace.groups.findIndex((group) => {
      const end = group.startingRank + group.items.length - 1;
      return target + 1 >= group.startingRank && target + 1 <= end;
    });
    if (!Number.isInteger(target) || target < 0 || groupIndex < 0) {
      setError({ code: "INVALID_RANK", message: `请输入 1—${workspace.items.length} 的排名` });
      return;
    }
    setSearch("");
    setError(null);
    window.setTimeout(() => virtualizer.scrollToIndex(groupIndex, { align: "center" }), 0);
  }

  function beginLocalReorder() {
    if (!workspace || workspace.status === "confirmed") return;
    const startRank = Number.parseInt(localStartRef.current?.value ?? localStartRank, 10);
    const endRank = Number.parseInt(localEndRef.current?.value ?? localEndRank, 10);
    if (
      !Number.isInteger(startRank) ||
      !Number.isInteger(endRank) ||
      startRank < 1 ||
      endRank < startRank
    ) {
      setError({
        code: "INVALID_LOCAL_RANGE",
        message: "请输入有效的起止排名，结束排名不能小于起始排名",
      });
      return;
    }
    const range = resolveLocalRankRange(workspace.groups, startRank, endRank);
    if (!range) {
      setError({ code: "INVALID_LOCAL_RANGE", message: "局部重排需要选择至少两个连续排名组" });
      return;
    }
    setSearch("");
    setError(null);
    setLocalRange({ ...range, originalGroups: workspace.groups });
  }

  function cancelLocalReorder() {
    if (!workspace || !localRange) return;
    const groups = localRange.originalGroups;
    setWorkspace({ ...workspace, groups, items: groups.flatMap((group) => group.items) });
    setLocalRange(null);
  }

  async function finishLocalReorder() {
    if (!workspace || !localRange || busy) return;
    setBusy(true);
    setError(null);
    try {
      // 只传稳定的组 ID 顺序，让后端在事务内重算位置并写入一条历史记录。
      setWorkspace(
        await applyLocalRankOrder({
          projectPath: project.projectPath,
          taskId: task.id,
          startPosition: localRange.startPosition,
          orderedGroupIds: workspace.groups
            .slice(localRange.startPosition, localRange.endPosition + 1)
            .map((group) => group.groupId),
        }),
      );
      setLocalRange(null);
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function applyGroupChange(operation: () => Promise<DragWorkspaceState>) {
    if (busy || workspace?.status === "confirmed") return;
    setBusy(true);
    setError(null);
    try {
      setWorkspace(await operation());
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  function mergeWithPrevious(group: RankedGroup) {
    if (!workspace || group.position === 0) return;
    const previous = workspace.groups[group.position - 1];
    if (!previous) return;
    void applyGroupChange(() =>
      mergeRankGroups({
        projectPath: project.projectPath,
        taskId: task.id,
        sourceGroupId: group.groupId,
        targetGroupId: previous.groupId,
      }),
    );
  }

  function renameGroup(group: RankedGroup) {
    const name = window.prompt("输入新的并列组名称", group.name ?? "并列组");
    if (!name?.trim()) return;
    void applyGroupChange(() =>
      renameRankGroup({
        projectPath: project.projectPath,
        taskId: task.id,
        groupId: group.groupId,
        name,
      }),
    );
  }

  async function updateCriteria(criteria: string) {
    if (!workspace || busy) return;
    setBusy(true);
    setError(null);
    try {
      await updateSortTaskCriteria({ projectPath: project.projectPath, taskId: task.id, criteria });
      setWorkspace({ ...workspace, criteria });
      await onProjectDataChanged?.();
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleUnlock() {
    if (!workspace || busy) return;
    setBusy(true);
    setError(null);
    try {
      await unlockSortResult({ projectPath: project.projectPath, taskId: task.id });
      setWorkspace({ ...workspace, status: "sorting" });
      await onProjectDataChanged?.();
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <main className="project-shell drag-shell">
      <section className="drag-workspace">
        <header className="drag-compact-header">
          <h1>{workspace?.taskName ?? task.name}</h1>
          <EditableSortCriteria
            compact
            busy={!workspace || busy}
            value={workspace?.criteria ?? task.criteria}
            onSave={(criteria) => void updateCriteria(criteria)}
          />
          <div className="workspace-heading-actions">
            {onReview && (
              <button className="primary-button" disabled={busy} onClick={onReview}>
                预览并确认结果 →
              </button>
            )}
            <div className="save-indicator" aria-live="polite">
              <span className={busy ? "saving" : ""} />
              {busy ? "正在保存…" : "已保存到本地"}
            </div>
          </div>
        </header>

        {workspace?.status === "confirmed" && (
          <div className="locked-workspace-banner" role="status">
            <div>
              <strong>🔒 当前结果已确认并锁定</strong>
              <span>排序操作已暂停，最近一次确认快照会保留。</span>
            </div>
            <button
              className="secondary-button"
              disabled={busy}
              onClick={() => void handleUnlock()}
            >
              快速解锁并编辑
            </button>
          </div>
        )}

        <div className="drag-toolbar">
          <label className="search-control">
            <span>⌕</span>
            <input
              value={search}
              disabled={localRange !== null}
              onChange={(event) => setSearch(event.target.value)}
              placeholder="搜索条目或字段"
            />
          </label>
          <div className="rank-jump">
            <input
              aria-label="目标排名"
              value={rank}
              onChange={(event) => setRank(event.target.value)}
              placeholder="排名"
              inputMode="numeric"
            />
            <button onClick={jumpToRank}>跳转</button>
          </div>
          <button className="toolbar-button" onClick={() => setShowAuxiliary((shown) => !shown)}>
            {showAuxiliary ? "折叠辅助字段" : "展开辅助字段"}
          </button>
          <div className="local-rank-controls">
            {localRange ? (
              <>
                <strong>局部重排中</strong>
                <button className="toolbar-button" disabled={busy} onClick={cancelLocalReorder}>
                  取消
                </button>
                <button
                  className="primary-button compact-button"
                  disabled={busy}
                  onClick={() => void finishLocalReorder()}
                >
                  完成并写回
                </button>
              </>
            ) : (
              <>
                <input
                  ref={localStartRef}
                  aria-label="局部重排起始排名"
                  value={localStartRank}
                  onChange={(event) => setLocalStartRank(event.target.value)}
                  placeholder="起始排名"
                  inputMode="numeric"
                />
                <span>—</span>
                <input
                  ref={localEndRef}
                  aria-label="局部重排结束排名"
                  value={localEndRank}
                  onChange={(event) => setLocalEndRank(event.target.value)}
                  placeholder="结束排名"
                  inputMode="numeric"
                />
                <button
                  className="toolbar-button"
                  disabled={workspace?.status === "confirmed"}
                  onClick={beginLocalReorder}
                >
                  局部重排
                </button>
              </>
            )}
          </div>
          <span className="drag-zone-legend">
            {localRange ? "上方插入 · 下方插入" : "上方插入 · 中间并列 · 下方插入"}
          </span>
          <span className="toolbar-spacer" />
          <button
            className="toolbar-button"
            disabled={
              !workspace?.canUndo || busy || localRange !== null || workspace.status === "confirmed"
            }
            onClick={() => void applyHistory("undo")}
          >
            ↶ 撤销
          </button>
          <button
            className="toolbar-button"
            disabled={
              !workspace?.canRedo || busy || localRange !== null || workspace.status === "confirmed"
            }
            onClick={() => void applyHistory("redo")}
          >
            ↷ 重做
          </button>
        </div>

        {error && (
          <div className="error-banner compact" role="alert">
            <strong>{error.message}</strong>
            {error.detail && <span>{error.detail}</span>}
          </div>
        )}
        {!workspace && !error && <div className="workspace-loading">正在加载排序条目…</div>}
        {workspace && (
          <>
            <div className="list-summary">
              <span>
                {localRange
                  ? `局部区间：第 ${filteredGroups[0]?.startingRank ?? "—"}—${filteredGroups.at(-1) ? filteredGroups.at(-1)!.startingRank + filteredGroups.at(-1)!.items.length - 1 : "—"} 名 · ${filteredGroups.length} 组`
                  : search
                    ? `找到 ${filteredGroups.length} 组`
                    : `共 ${workspace.items.length} 条 · ${workspace.groups.length} 个排名组`}
              </span>
              <span>拖动六点手柄，放到目标条目的上方、中间或下方</span>
            </div>
            <DndContext
              sensors={sensors}
              collisionDetection={rankCollisionDetection}
              measuring={{ droppable: { strategy: MeasuringStrategy.Always } }}
              onDragStart={(event) => setActiveGroupId(String(event.active.id))}
              onDragCancel={() => setActiveGroupId(null)}
              onDragEnd={handleDragEnd}
            >
              <div className="virtual-rank-list" ref={scrollRef}>
                <div className="virtual-rank-canvas" style={{ height: virtualizer.getTotalSize() }}>
                  {virtualizer.getVirtualItems().map((virtualRow) => {
                    const group = filteredGroups[virtualRow.index];
                    if (!group) return null;
                    return (
                      <DraggableRankCard
                        key={group.groupId}
                        group={group}
                        showAuxiliary={showAuxiliary}
                        busy={busy || workspace.status === "confirmed"}
                        projectPath={project.projectPath}
                        allowNetworkImages={allowNetworkImages}
                        mediaFieldNames={mediaFieldNames}
                        dragActive={activeGroupId !== null}
                        style={{
                          transform: `translateY(${virtualRow.start}px)`,
                          height: virtualRow.size,
                        }}
                        localMode={localRange !== null}
                        onTop={() =>
                          void persistMove(group.groupId, localRange?.startPosition ?? 0)
                        }
                        onBottom={() =>
                          void persistMove(
                            group.groupId,
                            localRange?.endPosition ?? workspace.groups.length - 1,
                          )
                        }
                        onMergePrevious={() => mergeWithPrevious(group)}
                        onRename={() => renameGroup(group)}
                        onSplit={(item) =>
                          void applyGroupChange(() =>
                            splitRankGroupItem({
                              projectPath: project.projectPath,
                              taskId: task.id,
                              groupId: group.groupId,
                              itemId: item.itemId,
                            }),
                          )
                        }
                        onDetail={setDetailItem}
                      />
                    );
                  })}
                </div>
              </div>
              <DragOverlay dropAnimation={null} modifiers={[centerCompactPreviewOnPointer]}>
                {activeGroup && <RankDragPreview group={activeGroup} />}
              </DragOverlay>
            </DndContext>
          </>
        )}
      </section>
      <ItemDetailDialog
        item={detailItem}
        projectPath={project.projectPath}
        allowNetworkImages={allowNetworkImages}
        mediaFieldNames={mediaFieldNames}
        onClose={() => setDetailItem(null)}
      />
    </main>
  );
}

function RankDragPreview({ group }: { group: RankedGroup }) {
  const representative = group.items[0];
  if (!representative) return null;
  return (
    <div className="rank-drag-preview" data-testid="rank-drag-preview">
      <span className="rank-drag-preview-grip" aria-hidden="true">
        ⠿
      </span>
      <span className="rank-drag-preview-copy">
        <strong>{group.name ?? representative.primaryLabel}</strong>
      </span>
    </div>
  );
}

interface CardProps {
  group: RankedGroup;
  showAuxiliary: boolean;
  busy: boolean;
  projectPath: string;
  allowNetworkImages: boolean;
  mediaFieldNames: string[];
  dragActive: boolean;
  localMode: boolean;
  style: React.CSSProperties;
  onTop: () => void;
  onBottom: () => void;
  onMergePrevious: () => void;
  onRename: () => void;
  onSplit: (item: RankedItem) => void;
  onDetail: (item: RankedItem) => void;
}

function DraggableRankCard({
  group,
  showAuxiliary,
  busy,
  projectPath,
  allowNetworkImages,
  mediaFieldNames,
  dragActive,
  localMode,
  style,
  onTop,
  onBottom,
  onMergePrevious,
  onRename,
  onSplit,
  onDetail,
}: CardProps) {
  const { attributes, listeners, setNodeRef, isDragging } = useDraggable({
    id: group.groupId,
    disabled: busy,
  });
  const representative = group.items[0];
  if (!representative) return null;
  return (
    <div
      ref={setNodeRef}
      className={`virtual-rank-row${isDragging ? " drag-source-placeholder" : ""}`}
      style={style}
    >
      <article className="rank-card">
        <div className="rank-position">
          <strong className="rank-number">
            <small>当前排名</small>
            {group.startingRank}
          </strong>
          {group.items.length > 1 ? (
            <span>并列 × {group.items.length}</span>
          ) : (
            <span>原始 {representative.originalPosition + 1}</span>
          )}
        </div>
        <button
          className="drag-handle"
          aria-label={`拖动 ${group.name ?? representative.primaryLabel}`}
          disabled={busy}
          {...attributes}
          {...listeners}
        >
          ⠿
        </button>
        <div className="rank-content">
          {group.items.length > 1 && (
            <div className="tie-group-heading" onDoubleClick={onRename}>
              <strong>{group.name ?? "并列组"}</strong>
              <button disabled={localMode} onClick={onRename}>
                重命名
              </button>
            </div>
          )}
          <div className={group.items.length > 1 ? "tie-members tied" : "tie-members"}>
            {group.items.map((item) => (
              <div className="tie-member" key={item.itemId}>
                <button
                  className="tie-member-label"
                  title={item.primaryLabel}
                  onClick={() => onDetail(item)}
                >
                  {item.primaryLabel}
                </button>
                {showAuxiliary && item.auxiliaryFields.length > 0 && (
                  <div className="tie-member-auxiliary">
                    {item.auxiliaryFields.map((field) => (
                      <span key={field.name}>
                        {mediaFieldNames.includes(field.name) && (
                          <MediaThumbnail
                            projectPath={projectPath}
                            itemId={item.itemId}
                            fieldName={field.name}
                            value={field.value}
                            allowNetworkImages={allowNetworkImages}
                            compact
                          />
                        )}
                        {field.name}：{valueText(field.value)}
                      </span>
                    ))}
                  </div>
                )}
                {group.items.length > 1 && (
                  <button
                    className="split-tie-button"
                    disabled={localMode}
                    onClick={() => onSplit(item)}
                  >
                    拆分
                  </button>
                )}
              </div>
            ))}
          </div>
        </div>
        <div className="rank-actions">
          <button
            title="置顶"
            aria-label={`将 ${group.name ?? representative.primaryLabel} 置顶`}
            disabled={busy || group.position === 0}
            onClick={onTop}
          >
            ↑ 置顶
          </button>
          <button
            title="置底"
            aria-label={`将 ${group.name ?? representative.primaryLabel} 置底`}
            disabled={busy}
            onClick={onBottom}
          >
            ↓ 置底
          </button>
          <button disabled={busy || localMode || group.position === 0} onClick={onMergePrevious}>
            ＝ 与上组并列
          </button>
        </div>
      </article>
      <RankDropZones
        groupId={group.groupId}
        enabled={dragActive && !isDragging}
        allowTie={!localMode}
      />
    </div>
  );
}

function RankDropZones({
  groupId,
  enabled,
  allowTie,
}: {
  groupId: string;
  enabled: boolean;
  allowTie: boolean;
}) {
  const { over } = useDndContext();
  const overData = over?.data.current as RankDropData | undefined;
  const revealed = enabled && overData?.type === "rank-drop-zone" && overData.groupId === groupId;
  return (
    <div className={`rank-drop-zones${revealed ? " visible" : ""}`} aria-hidden={!revealed}>
      <RankDropZone groupId={groupId} placement="before" enabled={enabled} label="放在此组前面" />
      {allowTie && (
        <RankDropZone groupId={groupId} placement="tie" enabled={enabled} label="与此组并列" />
      )}
      <RankDropZone groupId={groupId} placement="after" enabled={enabled} label="放在此组后面" />
    </div>
  );
}

function RankDropZone({
  groupId,
  placement,
  enabled,
  label,
}: {
  groupId: string;
  placement: DropPlacement;
  enabled: boolean;
  label: string;
}) {
  const data: RankDropData = { type: "rank-drop-zone", groupId, placement };
  const { isOver, setNodeRef } = useDroppable({
    id: `rank-drop:${groupId}:${placement}`,
    data,
    disabled: !enabled,
  });
  return (
    <div
      ref={setNodeRef}
      className={`rank-drop-zone ${placement}${isOver ? " active" : ""}`}
      data-placement={placement}
    >
      <span>{label}</span>
    </div>
  );
}

function recalculateGroupRanks(groups: RankedGroup[]): RankedGroup[] {
  let startingRank = 1;
  return groups.map((group, position) => {
    const next = {
      ...group,
      position,
      startingRank,
      items: group.items.map((item) => ({ ...item, position })),
    };
    startingRank += group.items.length;
    return next;
  });
}

function ItemDetailDialog({
  item,
  projectPath,
  allowNetworkImages,
  mediaFieldNames,
  onClose,
}: {
  item: RankedItem | null;
  projectPath: string;
  allowNetworkImages: boolean;
  mediaFieldNames: string[];
  onClose: () => void;
}) {
  return (
    <Dialog.Root open={Boolean(item)} onOpenChange={(open) => !open && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content item-detail-dialog">
          <Dialog.Title className="dialog-title">条目详情</Dialog.Title>
          <Dialog.Description className="dialog-description">
            当前排名第 {item ? item.position + 1 : "—"} 位 · 原始排名第{" "}
            {item ? item.originalPosition + 1 : "—"} 位
          </Dialog.Description>
          <dl className="detail-fields">
            {item?.fields.map((field) => (
              <div key={field.name}>
                <dt>{field.name}</dt>
                <dd>
                  {item && mediaFieldNames.includes(field.name) && (
                    <MediaThumbnail
                      projectPath={projectPath}
                      itemId={item.itemId}
                      fieldName={field.name}
                      value={field.value}
                      allowNetworkImages={allowNetworkImages}
                    />
                  )}
                  <span>{valueText(field.value)}</span>
                </dd>
              </div>
            ))}
          </dl>
          <div className="dialog-actions">
            <Dialog.Close asChild>
              <button className="primary-button">完成</button>
            </Dialog.Close>
          </div>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function valueText(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "string") return value;
  if (typeof value === "object") return JSON.stringify(value);
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint") {
    return value.toString();
  }
  return "—";
}
