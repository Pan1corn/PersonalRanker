/**
 * 1v1 传递排序工作台。
 * 每次用户判断都会由后端推进持久化的二分插入状态，组件只渲染当前候选与基准组。
 */
import { useCallback, useEffect, useState } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import { MediaThumbnail } from "../media/MediaThumbnail";
import type { ProjectMetadata } from "../projects/types";
import {
  answerComparison,
  loadComparisonWorkspace,
  renameComparisonRankGroup,
  skipComparison,
  undoComparison,
} from "./comparison-api";
import { EditableSortCriteria } from "./EditableSortCriteria";
import { unlockSortResult } from "./result-api";
import { updateSortTaskCriteria } from "./sort-task-api";
import type {
  ComparisonDecision,
  ComparisonItem,
  ComparisonWorkspaceState,
  MatrixStanding,
  SortTaskOverview,
} from "./types";

interface Props {
  project: ProjectMetadata;
  task: SortTaskOverview;
  onBack: () => void;
  onReview?: () => void;
  onFineTune?: () => void;
  allowNetworkImages?: boolean;
  mediaFieldNames?: string[];
  onProjectDataChanged?: () => Promise<void>;
}

type ComparisonAction = ComparisonDecision | "skip" | "undo";

export function ComparisonWorkspace({
  project,
  task,
  onBack,
  onReview,
  onFineTune,
  allowNetworkImages = false,
  mediaFieldNames = [],
  onProjectDataChanged,
}: Props) {
  const [workspace, setWorkspace] = useState<ComparisonWorkspaceState | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [scoreboardOpen, setScoreboardOpen] = useState(false);
  const [orderPreviewOpen, setOrderPreviewOpen] = useState(false);
  const isMatrix = (workspace?.mode ?? task.mode) === "matrix";

  useEffect(() => {
    let active = true;
    void loadComparisonWorkspace({ projectPath: project.projectPath, taskId: task.id })
      .then((state) => active && setWorkspace(state))
      .catch((cause) => active && setError(normalizeAppError(cause)));
    return () => {
      active = false;
    };
  }, [project.projectPath, task.id]);

  const perform = useCallback(
    async (action: ComparisonAction) => {
      // busy 锁同时防止按钮连击和键盘重复触发导致同一会话并发推进。
      if (!workspace || busy || workspace.status === "confirmed") return;
      if (workspace.completed && action !== "undo") return;
      setBusy(true);
      setError(null);
      const input = { projectPath: project.projectPath, taskId: task.id };
      try {
        if (action === "skip") setWorkspace(await skipComparison(input));
        else if (action === "undo") setWorkspace(await undoComparison(input));
        else setWorkspace(await answerComparison({ ...input, decision: action }));
      } catch (cause) {
        setError(normalizeAppError(cause));
      } finally {
        setBusy(false);
      }
    },
    [busy, project.projectPath, task.id, workspace],
  );

  useEffect(() => {
    function handleKey(event: KeyboardEvent) {
      if (event.repeat) return;
      // 表单控件获得焦点时保留其原生按键行为，避免录入内容时误提交判断。
      const target = event.target;
      if (target instanceof Element && target.matches("input, textarea, select")) return;
      const key = event.key.toLocaleLowerCase();
      let action: ComparisonAction | null = null;
      if ((event.ctrlKey || event.metaKey) && key === "z") action = "undo";
      else if (key === "a" || key === "arrowleft") action = "left_before";
      else if (key === "d" || key === "arrowright") action = "right_before";
      else if (key === "w" || key === "arrowup") action = "tie";
      else if (key === "s" || key === "arrowdown") action = "skip";
      if (!action) return;
      event.preventDefault();
      void perform(action);
    }
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [perform]);

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

  async function renameGroup(item: ComparisonItem) {
    if (!workspace || busy || workspace.status === "confirmed" || (item.itemIds?.length ?? 1) < 2)
      return;
    const name = window.prompt("输入新的并列组名称", item.groupName ?? "并列组");
    if (!name?.trim()) return;
    setBusy(true);
    setError(null);
    try {
      setWorkspace(
        await renameComparisonRankGroup({
          projectPath: project.projectPath,
          taskId: task.id,
          groupId: item.groupId,
          name: name.trim(),
        }),
      );
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
    <main className="project-shell comparison-shell">
      <section className="comparison-workspace">
        <header className="comparison-heading">
          <div>
            <p className="eyebrow">{isMatrix ? "1V1 MATRIX" : "1V1 COMPARISON"}</p>
            <h1>{workspace?.taskName ?? task.name}</h1>
          </div>
          <div className="save-indicator" aria-live="polite">
            <span className={busy ? "saving" : ""} />
            {busy ? "正在保存判断…" : "进度已保存"}
          </div>
        </header>

        <EditableSortCriteria
          busy={!workspace || busy}
          value={workspace?.criteria ?? task.criteria}
          onSave={(criteria) => void updateCriteria(criteria)}
        />

        {workspace?.status === "confirmed" && (
          <div className="locked-workspace-banner" role="status">
            <div>
              <strong>🔒 当前结果已确认并锁定</strong>
              <span>比较判断和撤销操作已暂停，确认快照仍会保留。</span>
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

        {workspace && <ComparisonProgress workspace={workspace} />}
        {isMatrix && workspace && (
          <button
            className="matrix-scoreboard-handle"
            aria-expanded={scoreboardOpen}
            onClick={() => setScoreboardOpen((open) => !open)}
          >
            {scoreboardOpen ? "收起得分表 →" : "← 拉出得分表"}
          </button>
        )}
        {!isMatrix && workspace && (
          <button
            className="comparison-order-handle"
            aria-expanded={orderPreviewOpen}
            aria-controls="comparison-order-preview"
            onClick={() => setOrderPreviewOpen((open) => !open)}
          >
            {orderPreviewOpen ? "收起当前顺序 →" : "← 预览当前顺序"}
          </button>
        )}
        {error && (
          <div className="error-banner compact" role="alert">
            <strong>{error.message}</strong>
            {error.detail && <span>{error.detail}</span>}
          </div>
        )}
        {!workspace && !error && <div className="workspace-loading">正在恢复比较进度…</div>}

        {workspace && !workspace.completed && workspace.left && workspace.right && (
          <>
            <div className="comparison-stage">
              <ComparisonCard
                side="left"
                item={workspace.left}
                shortcut="A / ←"
                projectPath={project.projectPath}
                allowNetworkImages={allowNetworkImages}
                mediaFieldNames={mediaFieldNames}
                label={isMatrix ? "左侧对象" : "候选条目"}
                onRename={() => void renameGroup(workspace.left!)}
              />
              <div className="versus-mark">VS</div>
              <ComparisonCard
                side="right"
                item={workspace.right}
                shortcut="D / →"
                projectPath={project.projectPath}
                allowNetworkImages={allowNetworkImages}
                mediaFieldNames={mediaFieldNames}
                label={isMatrix ? "右侧对象" : "已定位条目"}
                onRename={() => void renameGroup(workspace.right!)}
              />
            </div>
            <div className="comparison-actions">
              <button
                className="decision-button left"
                disabled={busy || workspace.status === "confirmed"}
                onClick={() => void perform("left_before")}
              >
                <kbd>A</kbd>
                <span>
                  <strong>{isMatrix ? "左侧胜出" : "左侧更靠前"}</strong>
                  <small>或按 ←</small>
                </span>
              </button>
              <button
                className="comparison-utility tie-decision"
                disabled={busy || workspace.status === "confirmed"}
                onClick={() => void perform("tie")}
              >
                <strong>{isMatrix ? "本场平局" : "二者并列"}</strong>
                <small>W / ↑</small>
              </button>
              <button
                className="comparison-utility"
                disabled={busy || workspace.status === "confirmed"}
                onClick={() => void perform("skip")}
              >
                <strong>跳过本次</strong>
                <small>S / ↓</small>
              </button>
              <button
                className="comparison-utility"
                disabled={busy || !workspace.canUndo || workspace.status === "confirmed"}
                onClick={() => void perform("undo")}
              >
                <strong>撤销判断</strong>
                <small>Ctrl+Z</small>
              </button>
              <button
                className="decision-button right"
                disabled={busy || workspace.status === "confirmed"}
                onClick={() => void perform("right_before")}
              >
                <span>
                  <strong>{isMatrix ? "右侧胜出" : "右侧更靠前"}</strong>
                  <small>或按 →</small>
                </span>
                <kbd>D</kbd>
              </button>
            </div>
          </>
        )}

        {workspace?.completed && (
          <div className="comparison-complete">
            <div className="complete-mark">✓</div>
            <h2>{isMatrix ? "1v1 矩阵排序已完成" : "1v1 传递排序已完成"}</h2>
            <p>
              {isMatrix
                ? `已完成 ${workspace.comparisonCount} 场比较，并按胜场生成最终排名。`
                : `通过 ${workspace.comparisonCount} 次直接判断，已定位全部 ${workspace.totalCount} 个条目。`}
            </p>
            <ol>
              {workspace.orderedItems.map((item) => (
                <li
                  key={item.groupId}
                  title={(item.itemIds?.length ?? 1) > 1 ? "双击修改并列组名称" : undefined}
                  onDoubleClick={() => void renameGroup(item)}
                >
                  <div className="comparison-complete-item">
                    <span>{item.groupName ?? item.primaryLabel}</span>
                    {(item.itemIds?.length ?? 1) > 1 && (
                      <>
                        <button
                          type="button"
                          disabled={busy || workspace.status === "confirmed"}
                          onClick={() => void renameGroup(item)}
                        >
                          重命名
                        </button>
                        <div className="comparison-complete-members">
                          {item.memberLabels?.map((label, index) => (
                            <span key={`${item.groupId}-${index}`}>{label}</span>
                          ))}
                        </div>
                      </>
                    )}
                  </div>
                </li>
              ))}
            </ol>
            <div className="complete-actions">
              <button
                className="secondary-button"
                disabled={!workspace.canUndo || busy || workspace.status === "confirmed"}
                onClick={() => void perform("undo")}
              >
                撤销最后一次判断
              </button>
              <button className="primary-button" onClick={onReview ?? onBack}>
                {onReview ? "预览并确认结果" : "返回项目数据"}
              </button>
              {onFineTune && (
                <button className="secondary-button" onClick={onFineTune}>
                  进入拖拽微调
                </button>
              )}
            </div>
          </div>
        )}
      </section>
      {isMatrix && workspace && (
        <MatrixScoreboard
          open={scoreboardOpen}
          standings={workspace.standings ?? []}
          onClose={() => setScoreboardOpen(false)}
        />
      )}
      {!isMatrix && workspace && (
        <ComparisonOrderPreview
          open={orderPreviewOpen}
          items={workspace.orderedItems}
          activeGroupId={workspace.right?.groupId}
          onClose={() => setOrderPreviewOpen(false)}
        />
      )}
    </main>
  );
}

function ComparisonProgress({ workspace }: { workspace: ComparisonWorkspaceState }) {
  const isMatrix = workspace.mode === "matrix";
  const estimatedTotal =
    workspace.plannedComparisonCount ?? workspace.comparisonCount + workspace.estimatedRemaining;
  const estimatedCompleted = Math.max(0, estimatedTotal - workspace.estimatedRemaining);
  const estimatedProgressPercent =
    estimatedTotal === 0
      ? 100
      : Math.min(100, Math.floor((estimatedCompleted * 100) / estimatedTotal));

  if (!isMatrix) {
    return (
      <section className="comparison-progress comparison-progress-dual" aria-label="比较进度">
        <div className="comparison-progress-row">
          <div className="progress-copy">
            <span className="progress-name">定位进度：</span>
            <strong>{workspace.progressPercent}%</strong>
            <span>{`已定位 ${workspace.locatedCount} / ${workspace.totalCount} 条`}</span>
          </div>
          <div className="progress-track">
            <span style={{ width: `${workspace.progressPercent}%` }} />
          </div>
        </div>
        <div className="comparison-progress-row">
          <div className="progress-copy estimated">
            <span className="progress-name">判断进度：</span>
            <strong>{estimatedProgressPercent}%</strong>
            <span>{`已判断 ${workspace.comparisonCount} / 约 ${estimatedTotal} 次`}</span>
          </div>
          <div className="progress-track estimated">
            <span style={{ width: `${estimatedProgressPercent}%` }} />
          </div>
        </div>
      </section>
    );
  }

  return (
    <section className="comparison-progress" aria-label="比较进度">
      <div className="progress-copy">
        <strong>{workspace.progressPercent}%</strong>
        <span>{`已比较 ${workspace.comparisonCount} / ${workspace.plannedComparisonCount ?? 0}`}</span>
      </div>
      <div className="progress-track">
        <span style={{ width: `${workspace.progressPercent}%` }} />
      </div>
      <dl>
        <div>
          <dt>已完成判断</dt>
          <dd>{workspace.comparisonCount}</dd>
        </div>
        <div>
          <dt>剩余场次</dt>
          <dd>{workspace.estimatedRemaining}</dd>
        </div>
        <div>
          <dt>抽取比例</dt>
          <dd>{`${workspace.matrixComparisonPercent ?? 100}%`}</dd>
        </div>
      </dl>
    </section>
  );
}

function ComparisonCard({
  side,
  item,
  shortcut,
  projectPath,
  allowNetworkImages,
  mediaFieldNames,
  label,
  onRename,
}: {
  side: "left" | "right";
  item: ComparisonItem;
  shortcut: string;
  projectPath: string;
  allowNetworkImages: boolean;
  mediaFieldNames: string[];
  label: string;
  onRename?: () => void;
}) {
  const isTieGroup = (item.memberLabels?.length ?? 0) > 1;

  return (
    <article className={`comparison-card ${side}`}>
      <div className="comparison-card-label">
        <span>{label}</span>
        <kbd>{shortcut}</kbd>
      </div>
      <h2
        className={
          isTieGroup ? "comparison-card-title comparison-group-title" : "comparison-card-title"
        }
        title={isTieGroup ? "双击修改并列组名称" : item.primaryLabel}
        tabIndex={isTieGroup ? 0 : undefined}
        onDoubleClick={isTieGroup ? onRename : undefined}
        onKeyDown={(event) => {
          if (!isTieGroup || (event.key !== "Enter" && event.key !== "F2")) return;
          event.preventDefault();
          onRename?.();
        }}
      >
        {isTieGroup ? (item.groupName ?? item.primaryLabel) : item.primaryLabel}
      </h2>
      {isTieGroup && (
        <div className="comparison-tie-members">
          <ul>
            {item.memberLabels?.map((label, index) => (
              <li key={`${item.groupId}-${index}`}>{label}</li>
            ))}
          </ul>
        </div>
      )}
      {item.auxiliaryFields.length > 0 ? (
        <dl>
          {item.auxiliaryFields.map((field) => (
            <div key={field.name}>
              <dt>{field.name}</dt>
              <dd>
                {mediaFieldNames.includes(field.name) && (
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
      ) : (
        <p className="no-auxiliary">没有配置辅助字段</p>
      )}
      <details>
        <summary>查看全部字段</summary>
        <dl className="comparison-all-fields">
          {item.fields.map((field) => (
            <div key={field.name}>
              <dt>{field.name}</dt>
              <dd>
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
                <span>{valueText(field.value)}</span>
              </dd>
            </div>
          ))}
        </dl>
      </details>
    </article>
  );
}

function MatrixScoreboard({
  open,
  standings,
  onClose,
}: {
  open: boolean;
  standings: MatrixStanding[];
  onClose: () => void;
}) {
  return (
    <aside className={open ? "matrix-scoreboard open" : "matrix-scoreboard"} aria-hidden={!open}>
      <header>
        <div>
          <small>LIVE SCOREBOARD</small>
          <h2>矩阵得分表</h2>
        </div>
        <button aria-label="收起得分表" onClick={onClose}>
          ×
        </button>
      </header>
      <div className="matrix-score-table-wrap">
        <table>
          <thead>
            <tr>
              <th>对象</th>
              <th>胜</th>
              <th>负</th>
              <th>平</th>
              <th>总场</th>
            </tr>
          </thead>
          <tbody>
            {standings.map((standing) => (
              <tr key={standing.item.groupId}>
                <td title={standing.item.primaryLabel}>{standing.item.primaryLabel}</td>
                <td>
                  <strong>{standing.wins}</strong>
                </td>
                <td>{standing.losses}</td>
                <td>{standing.ties}</td>
                <td>{standing.total}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>
    </aside>
  );
}

function ComparisonOrderPreview({
  open,
  items,
  activeGroupId,
  onClose,
}: {
  open: boolean;
  items: ComparisonItem[];
  activeGroupId?: string;
  onClose: () => void;
}) {
  return (
    <aside
      id="comparison-order-preview"
      className={open ? "comparison-order-preview open" : "comparison-order-preview"}
      aria-hidden={!open}
    >
      <header>
        <div>
          <small>CURRENT ORDER</small>
          <h2>当前已确定顺序</h2>
        </div>
        <button aria-label="收起当前顺序" onClick={onClose}>
          ×
        </button>
      </header>
      <p>已定位 {items.length} 组；橙色条目是当前正在参与比较的基准。</p>
      <ol>
        {items.map((item, index) => {
          const active = item.groupId === activeGroupId;
          const label = item.groupName ?? item.primaryLabel;
          return (
            <li key={item.groupId} className={active ? "active" : ""} aria-current={active}>
              <span>{index + 1}</span>
              <div>
                <strong title={label}>{label}</strong>
                {(item.memberLabels?.length ?? 0) > 1 && (
                  <small>{item.memberLabels?.join(" · ")}</small>
                )}
              </div>
              {active && <em>比较中</em>}
            </li>
          );
        })}
      </ol>
    </aside>
  );
}

function valueText(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "string") return value;
  if (typeof value === "object") return JSON.stringify(value);
  if (typeof value === "number" || typeof value === "boolean" || typeof value === "bigint")
    return value.toString();
  return "—";
}
