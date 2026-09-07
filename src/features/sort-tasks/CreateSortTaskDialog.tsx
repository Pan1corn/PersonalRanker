import * as Dialog from "@radix-ui/react-dialog";
import { type FormEvent, useState } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import type { DatasetOverview } from "../import/types";
import type { ProjectMetadata } from "../projects/types";
import { createSortTask } from "./sort-task-api";
import { SORT_MODE_DEFINITIONS } from "./sort-mode";
import type { InitialOrder, SortDirection, SortTaskMode, SortTaskOverview } from "./types";

interface Props {
  project: ProjectMetadata;
  dataset: DatasetOverview;
  sourceTasks?: SortTaskOverview[];
  initialSourceTaskId?: string;
  onClose: () => void;
  onCreated: (task: SortTaskOverview) => void;
}

export function CreateSortTaskDialog({
  project,
  dataset,
  sourceTasks = [],
  initialSourceTaskId,
  onClose,
  onCreated,
}: Props) {
  const orderableFields = dataset.fields.filter(
    (field) => field.fieldType === "number" || field.fieldType === "date",
  );
  const [name, setName] = useState(`${dataset.name}排序`);
  const [criteria, setCriteria] = useState("");
  const [mode, setMode] = useState<SortTaskMode>("drag");
  const [matrixComparisonPercent, setMatrixComparisonPercent] = useState(100);
  const completedTasks = sourceTasks.filter(
    (task) => task.datasetId === dataset.id && task.status === "confirmed",
  );
  const initialSourceTask = completedTasks.find((task) => task.id === initialSourceTaskId);
  const [orderKind, setOrderKind] = useState<"import" | "field" | "task_result">(
    initialSourceTask ? "task_result" : "import",
  );
  const [fieldName, setFieldName] = useState(orderableFields[0]?.name ?? "");
  const [direction, setDirection] = useState<SortDirection>("ascending");
  const [sourceTaskId, setSourceTaskId] = useState(
    initialSourceTask?.id ?? completedTasks[0]?.id ?? "",
  );
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (
      !name.trim() ||
      !criteria.trim() ||
      (orderKind === "field" && !fieldName) ||
      (orderKind === "task_result" && !sourceTaskId)
    )
      return;
    const initialOrder: InitialOrder =
      orderKind === "import"
        ? { kind: "import" }
        : orderKind === "field"
          ? { kind: "field", fieldName, direction }
          : { kind: "task_result", taskId: sourceTaskId };
    setBusy(true);
    setError(null);
    try {
      const task = await createSortTask({
        projectPath: project.projectPath,
        datasetId: dataset.id,
        name: name.trim(),
        criteria: criteria.trim(),
        mode,
        initialOrder,
        ...(mode === "matrix" ? { matrixComparisonPercent } : {}),
      });
      onCreated(task);
      onClose();
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog.Root open onOpenChange={(isOpen) => !isOpen && onClose()}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content
          className="dialog-content sort-task-dialog"
          aria-describedby="sort-task-description"
        >
          <Dialog.Title className="dialog-title">创建排序任务</Dialog.Title>
          <Dialog.Description id="sort-task-description" className="dialog-description">
            {initialSourceTask
              ? `将已锁定结果“${initialSourceTask.name}”视作新数据集，沿用其顺序创建新的排序任务。`
              : `基于“${dataset.name}”创建独立排序结果。每个条目会先进入一个单成员排序组。`}
          </Dialog.Description>
          <form onSubmit={(event) => void submit(event)}>
            <label className="field-label" htmlFor="sort-task-name">
              任务名称
            </label>
            <input
              id="sort-task-name"
              className="text-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
            />

            <label className="field-label" htmlFor="sort-criteria">
              排序标准
            </label>
            <textarea
              id="sort-criteria"
              className="criteria-input"
              value={criteria}
              onChange={(event) => setCriteria(event.target.value)}
              placeholder="例如：越重要、越值得优先处理的条目排在前面"
              rows={3}
            />

            <fieldset className="option-fieldset sort-mode-options">
              <legend>排序方式</legend>
              {SORT_MODE_DEFINITIONS.map((definition) => (
                <label
                  className={
                    mode === definition.mode
                      ? "option-card sort-mode-card selected"
                      : "option-card sort-mode-card"
                  }
                  key={definition.mode}
                >
                  <input
                    type="radio"
                    name="mode"
                    checked={mode === definition.mode}
                    onChange={() => setMode(definition.mode)}
                  />
                  <img src={definition.icon} alt="" aria-hidden="true" />
                  <span>
                    <strong>{definition.label}</strong>
                    <small>{definition.description}</small>
                  </span>
                </label>
              ))}
            </fieldset>

            {mode === "matrix" && (
              <section className="matrix-create-options" aria-label="矩阵比较设置">
                <div className="matrix-create-heading">
                  <div>
                    <strong>随机抽取对象比例</strong>
                    <small>每个对象随机抽取其余对象中的指定比例；重复对象对会自动跳过。</small>
                  </div>
                  <output htmlFor="matrix-comparison-percent">{matrixComparisonPercent}%</output>
                </div>
                <input
                  id="matrix-comparison-percent"
                  aria-label="随机抽取对象比例"
                  type="range"
                  min="1"
                  max="100"
                  step="1"
                  value={matrixComparisonPercent}
                  onChange={(event) => setMatrixComparisonPercent(Number(event.target.value))}
                />
                <p>{matrixEstimate(dataset.itemCount, matrixComparisonPercent)}</p>
                {dataset.itemCount > 100 && (
                  <div className="matrix-limit-warning" role="alert">
                    当前数据集有 {dataset.itemCount} 个对象。矩阵排序最多支持 100
                    个对象，请先缩小数据集。
                  </div>
                )}
              </section>
            )}

            {mode === "comparison" && (
              <section className="comparison-create-estimate" aria-label="传递排序预估">
                <strong>预计比较次数</strong>
                <span>
                  {comparisonEstimate(
                    orderKind === "task_result"
                      ? (completedTasks.find((task) => task.id === sourceTaskId)?.rankGroupCount ??
                          dataset.itemCount)
                      : dataset.itemCount,
                  )}
                </span>
              </section>
            )}

            <fieldset className="option-fieldset initial-order-options">
              <legend>初始顺序</legend>
              <label className={orderKind === "import" ? "option-card selected" : "option-card"}>
                <input
                  type="radio"
                  name="initial-order"
                  checked={orderKind === "import"}
                  onChange={() => setOrderKind("import")}
                />
                <span>
                  <strong>沿用导入顺序</strong>
                  <small>按照源数据中的原始序号排列</small>
                </span>
              </label>
              <label
                className={orderKind === "field" ? "option-card selected" : "option-card"}
                aria-disabled={orderableFields.length === 0}
              >
                <input
                  type="radio"
                  name="initial-order"
                  checked={orderKind === "field"}
                  disabled={orderableFields.length === 0}
                  onChange={() => setOrderKind("field")}
                />
                <span>
                  <strong>按字段预排序</strong>
                  <small>
                    {orderableFields.length
                      ? "使用数字或日期字段生成初始顺序"
                      : "当前数据集没有数字或日期字段"}
                  </small>
                </span>
              </label>
              <label
                className={orderKind === "task_result" ? "option-card selected" : "option-card"}
                aria-disabled={completedTasks.length === 0}
              >
                <input
                  type="radio"
                  name="initial-order"
                  checked={orderKind === "task_result"}
                  disabled={completedTasks.length === 0}
                  onChange={() => setOrderKind("task_result")}
                />
                <span>
                  <strong>沿用已完成任务</strong>
                  <small>
                    {completedTasks.length
                      ? "复制已确认任务的顺序与并列组"
                      : "当前数据表还没有已确认的排序任务"}
                  </small>
                </span>
              </label>
            </fieldset>

            {orderKind === "field" && (
              <div className="field-order-controls">
                <label>
                  排序字段
                  <select value={fieldName} onChange={(event) => setFieldName(event.target.value)}>
                    {orderableFields.map((field) => (
                      <option key={field.id} value={field.name}>
                        {field.name}（{field.fieldType === "number" ? "数字" : "日期"}）
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  方向
                  <select
                    value={direction}
                    onChange={(event) => setDirection(event.target.value as SortDirection)}
                  >
                    <option value="ascending">升序</option>
                    <option value="descending">降序</option>
                  </select>
                </label>
              </div>
            )}

            {orderKind === "task_result" && (
              <div className="field-order-controls single-control">
                <label>
                  来源任务
                  <select
                    value={sourceTaskId}
                    onChange={(event) => setSourceTaskId(event.target.value)}
                  >
                    {completedTasks.map((task) => (
                      <option key={task.id} value={task.id}>
                        {task.name}（{task.rankGroupCount} 组）
                      </option>
                    ))}
                  </select>
                </label>
              </div>
            )}

            {error && (
              <div className="error-banner compact" role="alert">
                <strong>{error.message}</strong>
                {error.detail && error.detail !== error.message && <span>{error.detail}</span>}
              </div>
            )}
            <div className="dialog-actions">
              <button className="secondary-button" type="button" onClick={onClose}>
                取消
              </button>
              <button
                className="primary-button"
                type="submit"
                disabled={
                  busy ||
                  !name.trim() ||
                  !criteria.trim() ||
                  (orderKind === "field" && !fieldName) ||
                  (orderKind === "task_result" && !sourceTaskId) ||
                  (mode === "matrix" && dataset.itemCount > 100)
                }
              >
                {busy ? "正在创建…" : "创建任务"}
              </button>
            </div>
          </form>
          <Dialog.Close className="dialog-close" aria-label="关闭">
            ×
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function matrixEstimate(itemCount: number, percent: number): string {
  if (itemCount < 2) return "至少需要两个对象。";
  const maximum = (itemCount * (itemCount - 1)) / 2;
  if (percent === 100) return `将完成全部 ${maximum} 次两两比较。`;
  const opponents = Math.max(1, Math.round(((itemCount - 1) * percent) / 100));
  const baseline = Math.ceil((itemCount * opponents) / 2);
  const upper = Math.min(maximum, itemCount * opponents);
  return `每个对象抽取约 ${opponents} 个对手，预计 ${baseline}–${upper} 次；实际次数取决于随机对象对的重合程度。`;
}

function comparisonEstimate(itemCount: number): string {
  if (itemCount < 2) return "至少需要两个对象。";
  let estimate = 0;
  for (let located = 1; located < itemCount; located += 1) {
    estimate += Math.ceil(Math.log2(located + 1));
  }
  return `约 ${estimate} 次二分比较；出现并列时实际次数可能减少。`;
}
