/** 滑杆排序工作台：逐项记录百分制位置，并在全部评分后生成降序结果。 */
import { useEffect, useState, type CSSProperties } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import { MediaThumbnail } from "../media/MediaThumbnail";
import type { ProjectMetadata } from "../projects/types";
import { EditableSortCriteria } from "./EditableSortCriteria";
import { unlockSortResult } from "./result-api";
import { confirmSliderValue, loadSliderWorkspace } from "./slider-api";
import { updateSortTaskCriteria } from "./sort-task-api";
import type { ComparisonItem, SliderWorkspaceState, SortTaskOverview } from "./types";

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

const DEFAULT_VALUE = 50;

export function SliderWorkspace({
  project,
  task,
  onBack,
  onReview,
  onFineTune,
  allowNetworkImages = false,
  mediaFieldNames = [],
  onProjectDataChanged,
}: Props) {
  const [workspace, setWorkspace] = useState<SliderWorkspaceState | null>(null);
  const [value, setValue] = useState(DEFAULT_VALUE);
  const [showValues, setShowValues] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  useEffect(() => {
    let active = true;
    void loadSliderWorkspace({ projectPath: project.projectPath, taskId: task.id })
      .then((state) => active && setWorkspace(state))
      .catch((cause) => active && setError(normalizeAppError(cause)));
    return () => {
      active = false;
    };
  }, [project.projectPath, task.id]);

  async function handleConfirm() {
    if (!workspace?.current || busy || workspace.status === "confirmed") return;
    setBusy(true);
    setError(null);
    try {
      const next = await confirmSliderValue({
        projectPath: project.projectPath,
        taskId: task.id,
        value: roundToHundredth(value),
      });
      setWorkspace(next);
      setValue(DEFAULT_VALUE);
      if (next.completed) await onProjectDataChanged?.();
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

  const progressStyle = { "--slider-progress": `${value}%` } as CSSProperties;

  return (
    <main className="project-shell comparison-shell slider-shell">
      <section className="comparison-workspace slider-workspace">
        <header className="comparison-heading">
          <div>
            <p className="eyebrow">SLIDER RANKING</p>
            <h1>{workspace?.taskName ?? task.name}</h1>
          </div>
          <div className="slider-heading-actions">
            <label className="slider-value-toggle">
              <input
                type="checkbox"
                checked={showValues}
                onChange={(event) => setShowValues(event.currentTarget.checked)}
              />
              <span>显示具体数值</span>
            </label>
            <div className="save-indicator" aria-live="polite">
              <span className={busy ? "saving" : ""} />
              {busy ? "正在保存位置…" : "进度已保存"}
            </div>
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
              <span>滑杆评分已暂停，确认快照仍会保留。</span>
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

        {workspace && (
          <section className="comparison-progress slider-progress" aria-label="评分进度">
            <div className="progress-copy">
              <strong>{workspace.progressPercent}%</strong>
              <span>
                已完成 {workspace.completedCount} / {workspace.totalCount}
              </span>
            </div>
            <div className="progress-track">
              <span style={{ width: `${workspace.progressPercent}%` }} />
            </div>
            <strong className="slider-progress-remaining">
              剩余 {Math.max(0, workspace.totalCount - workspace.completedCount)} 项
            </strong>
          </section>
        )}

        {error && (
          <div className="error-banner compact" role="alert">
            <strong>{error.message}</strong>
            {error.detail && <span>{error.detail}</span>}
          </div>
        )}
        {!workspace && !error && <div className="workspace-loading">正在恢复滑杆排序进度…</div>}

        {workspace && !workspace.completed && workspace.current && (
          <div className="slider-stage">
            <SliderItemCard
              item={workspace.current}
              projectPath={project.projectPath}
              allowNetworkImages={allowNetworkImages}
              mediaFieldNames={mediaFieldNames}
            />
            <section className="slider-control-panel" aria-label="滑杆评分">
              <div
                className={showValues ? "slider-value-output" : "slider-value-output hidden"}
                aria-live="polite"
              >
                <small>当前位置</small>
                <output htmlFor="ranking-slider">
                  {showValues ? value.toFixed(2) : "具体数值已隐藏"}
                </output>
              </div>
              <input
                id="ranking-slider"
                className="ranking-slider"
                type="range"
                min="0"
                max="100"
                step="0.01"
                value={value}
                style={progressStyle}
                disabled={busy || workspace.status === "confirmed"}
                aria-label="当前对象排序位置"
                aria-valuetext={showValues ? value.toFixed(2) : "具体数值已隐藏"}
                onChange={(event) => setValue(roundToHundredth(Number(event.currentTarget.value)))}
              />
              <div className="slider-scale" aria-hidden="true">
                <span>{showValues ? "0.00 · 靠后" : "靠后"}</span>
                <span>{showValues ? "50.00" : "居中"}</span>
                <span>{showValues ? "100.00 · 靠前" : "靠前"}</span>
              </div>
              <p>向右评分越高，最终排名越靠前；完全相同的分值会自动并列。</p>
              <button
                className="primary-button slider-confirm-button"
                disabled={busy || workspace.status === "confirmed"}
                onClick={() => void handleConfirm()}
              >
                {busy ? "正在记录…" : "确定并继续"}
              </button>
            </section>
          </div>
        )}

        {workspace?.completed && (
          <div className="comparison-complete slider-complete">
            <div className="complete-mark">✓</div>
            <h2>滑杆排序已完成</h2>
            <p>已记录全部 {workspace.totalCount} 个对象，并按位置从高到低生成结果。</p>
            <div className="complete-actions">
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

        {workspace && workspace.ratings.length > 0 && (
          <section className="slider-records" aria-label="已完成评分">
            <header>
              <div>
                <p className="eyebrow">RECORDED</p>
                <h2>已完成对象</h2>
              </div>
              <span>{workspace.ratings.length} 项</span>
            </header>
            <ol>
              {workspace.ratings.map((rating, index) => (
                <li key={rating.item.itemId}>
                  <span className="slider-record-index">{index + 1}</span>
                  <strong title={rating.item.primaryLabel}>{rating.item.primaryLabel}</strong>
                  <output className={showValues ? "" : "hidden"}>
                    {showValues ? rating.value.toFixed(2) : "已隐藏"}
                  </output>
                </li>
              ))}
            </ol>
          </section>
        )}
      </section>
    </main>
  );
}

function SliderItemCard({
  item,
  projectPath,
  allowNetworkImages,
  mediaFieldNames,
}: {
  item: ComparisonItem;
  projectPath: string;
  allowNetworkImages: boolean;
  mediaFieldNames: string[];
}) {
  return (
    <article className="comparison-card slider-item-card">
      <div className="comparison-card-label">
        <span>当前对象</span>
        <span>拖动滑块或点击轨道</span>
      </div>
      <h2>{item.primaryLabel}</h2>
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
              <dd>{valueText(field.value)}</dd>
            </div>
          ))}
        </dl>
      </details>
    </article>
  );
}

function roundToHundredth(value: number): number {
  return Math.round(value * 100) / 100;
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
