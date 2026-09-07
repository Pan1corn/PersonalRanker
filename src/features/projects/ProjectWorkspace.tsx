/**
 * 项目级路由与编排组件：组织数据表、排序任务、工作台、媒体导入和历史快照。
 * 它只保存当前界面位置；排序内容本身始终由后端数据库提供。
 */
import * as Dialog from "@radix-ui/react-dialog";
import { confirm, open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import logo from "../../assets/logo.png";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import { DataImportDialog } from "../import/DataImportDialog";
import { deleteDataset } from "../import/import-api";
import type { DatasetOverview } from "../import/types";
import { importMediaFolder } from "../media/media-api";
import { CreateSortTaskDialog } from "../sort-tasks/CreateSortTaskDialog";
import { ComparisonWorkspace } from "../sort-tasks/ComparisonWorkspace";
import { DragSortWorkspace } from "../sort-tasks/DragSortWorkspace";
import { ResultWorkspace } from "../sort-tasks/ResultWorkspace";
import { SliderWorkspace } from "../sort-tasks/SliderWorkspace";
import { getSortModeDefinition } from "../sort-tasks/sort-mode";
import { deleteSortTask, updateSortTaskCriteria } from "../sort-tasks/sort-task-api";
import { unlockSortResult } from "../sort-tasks/result-api";
import type { SortTaskOverview } from "../sort-tasks/types";
import { getAutosaveStatus, listProjectSnapshots, restoreProjectSnapshot } from "./history-api";
import type { AutosaveStatus, ProjectMetadata, ProjectSnapshot } from "./types";

interface Props {
  project: ProjectMetadata;
  datasets: DatasetOverview[];
  sortTasks: SortTaskOverview[];
  onDatasetImported: (dataset: DatasetOverview) => void;
  onSortTaskCreated: (task: SortTaskOverview) => void;
  onProjectDataChanged: () => Promise<void>;
  onCloseProject: () => void;
  initialAutosave: AutosaveStatus | null;
  allowNetworkImages?: boolean;
  onAllowNetworkImagesChange?: (enabled: boolean) => void;
}

export function ProjectWorkspace({
  project,
  datasets,
  sortTasks,
  onDatasetImported,
  onSortTaskCreated,
  onProjectDataChanged,
  onCloseProject,
  initialAutosave,
  allowNetworkImages = false,
  onAllowNetworkImagesChange,
}: Props) {
  const [importOpen, setImportOpen] = useState(false);
  const [taskDataset, setTaskDataset] = useState<DatasetOverview | null>(null);
  const [taskSourceResultId, setTaskSourceResultId] = useState<string | null>(null);
  // 四种工作区互斥；使用独立状态可保留各组件明确的 props 类型和进入路径。
  const [activeDragTask, setActiveDragTask] = useState<SortTaskOverview | null>(null);
  const [activeComparisonTask, setActiveComparisonTask] = useState<SortTaskOverview | null>(null);
  const [activeSliderTask, setActiveSliderTask] = useState<SortTaskOverview | null>(null);
  const [activeResultTask, setActiveResultTask] = useState<SortTaskOverview | null>(null);
  const [autosave, setAutosave] = useState<AutosaveStatus | null>(initialAutosave);
  const [snapshots, setSnapshots] = useState<ProjectSnapshot[]>([]);
  const [historyOpen, setHistoryOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyError, setHistoryError] = useState<AppErrorPayload | null>(null);
  const [mediaImportDatasetId, setMediaImportDatasetId] = useState<string | null>(null);
  const [mediaNotice, setMediaNotice] = useState("");

  function beginTaskCreation(dataset: DatasetOverview, sourceTask?: SortTaskOverview) {
    setTaskSourceResultId(sourceTask?.id ?? null);
    setTaskDataset(dataset);
  }

  function closeTaskCreation() {
    setTaskDataset(null);
    setTaskSourceResultId(null);
  }
  useEffect(() => {
    if (historyBusy) return;
    let active = true;
    // 自动保存由后端在业务事务完成时记录；前端轮询只负责更新状态提示。
    async function refreshAutosave() {
      try {
        const status = await getAutosaveStatus(project.projectPath);
        if (active) setAutosave(status);
      } catch (cause) {
        if (active) setHistoryError(normalizeAppError(cause));
      }
    }
    void refreshAutosave();
    const timer = window.setInterval(() => void refreshAutosave(), 4_000);
    return () => {
      active = false;
      window.clearInterval(timer);
    };
  }, [historyBusy, project.projectPath]);

  async function openHistory() {
    setHistoryOpen(true);
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      setSnapshots(await listProjectSnapshots(project.projectPath));
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  async function restoreSnapshot(snapshot: ProjectSnapshot) {
    const accepted = await confirm(
      `将项目恢复到“${snapshot.label}”（${formatTimestamp(snapshot.createdAt)}）。恢复前会自动保存当前状态。`,
      {
        title: "恢复历史快照",
        kind: "warning",
        okLabel: "创建安全备份并恢复",
        cancelLabel: "取消",
      },
    );
    if (!accepted) return;
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      // 后端会先为当前数据库创建安全快照，再用带回滚文件的替换流程恢复目标快照。
      const restored = await restoreProjectSnapshot(project.projectPath, snapshot.id);
      setAutosave(restored.autosave);
      await onProjectDataChanged();
      setSnapshots(await listProjectSnapshots(project.projectPath));
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  function openTask(task: SortTaskOverview) {
    // 显式关闭其它工作区，确保侧栏切换不会保留两个活动任务视图。
    setActiveResultTask(null);
    if (task.mode === "slider") {
      setActiveDragTask(null);
      setActiveComparisonTask(null);
      setActiveSliderTask(task);
      return;
    }
    setActiveSliderTask(null);
    if (task.mode === "drag") {
      setActiveComparisonTask(null);
      setActiveDragTask(task);
    } else {
      setActiveDragTask(null);
      setActiveComparisonTask(task);
    }
  }

  function openResult(task: SortTaskOverview) {
    setActiveDragTask(null);
    setActiveComparisonTask(null);
    setActiveSliderTask(null);
    setActiveResultTask(task);
  }

  function closeActiveTask() {
    setActiveDragTask(null);
    setActiveComparisonTask(null);
    setActiveSliderTask(null);
    setActiveResultTask(null);
  }

  async function removeTask(task: SortTaskOverview) {
    const accepted = await confirm(
      `确定永久删除排序任务“${task.name}”吗？该任务的排序、比较记录和快照将一并删除，且无法撤销。`,
      {
        title: "删除排序任务",
        kind: "warning",
        okLabel: "永久删除",
        cancelLabel: "取消",
      },
    );
    if (!accepted) return;
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      await deleteSortTask(project.projectPath, task.id);
      if (
        activeDragTask?.id === task.id ||
        activeComparisonTask?.id === task.id ||
        activeSliderTask?.id === task.id ||
        activeResultTask?.id === task.id
      ) {
        closeActiveTask();
      }
      await onProjectDataChanged();
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  async function removeDataset(dataset: DatasetOverview) {
    const taskCount = sortTasks.filter((task) => task.datasetId === dataset.id).length;
    const accepted = await confirm(
      `确定永久删除数据表“${dataset.name}”吗？${taskCount ? `与它关联的 ${taskCount} 个排序任务也会被删除。` : ""}原始文件副本和本地媒体缓存将一并移除，且无法撤销。`,
      {
        title: "删除数据表",
        kind: "warning",
        okLabel: "永久删除",
        cancelLabel: "取消",
      },
    );
    if (!accepted) return;
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      await deleteDataset(project.projectPath, dataset.id);
      if (
        activeDragTask?.datasetId === dataset.id ||
        activeComparisonTask?.datasetId === dataset.id ||
        activeSliderTask?.datasetId === dataset.id ||
        activeResultTask?.datasetId === dataset.id
      ) {
        closeActiveTask();
      }
      await onProjectDataChanged();
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  async function importImages(dataset: DatasetOverview) {
    const selected = await open({
      directory: true,
      multiple: false,
      title: `为“${dataset.name}”选择图片文件夹`,
    });
    if (!selected) return;
    setMediaImportDatasetId(dataset.id);
    setHistoryError(null);
    setMediaNotice("");
    try {
      const result = await importMediaFolder({
        projectPath: project.projectPath,
        datasetId: dataset.id,
        folderPath: selected,
      });
      setMediaNotice(
        `已扫描 ${result.imageFileCount} 张图片，为 ${result.matchedItemCount} 个条目匹配 ${result.matchedAssetCount} 个图片字段${result.unmatchedItemCount ? `；${result.unmatchedItemCount} 个条目未匹配` : ""}。`,
      );
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setMediaImportDatasetId(null);
    }
  }

  async function quickUnlock(task: SortTaskOverview) {
    const accepted = await confirm(
      `解锁“${task.name}”后可以继续调整顺序；最近一次确认快照仍会保留。`,
      {
        title: "解锁排序结果",
        kind: "warning",
        okLabel: "解锁并编辑",
        cancelLabel: "保持锁定",
      },
    );
    if (!accepted) return;
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      await unlockSortResult({ projectPath: project.projectPath, taskId: task.id });
      await onProjectDataChanged();
      openTask({ ...task, status: "sorting" });
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  async function editTaskCriteria(task: SortTaskOverview) {
    const criteria = window.prompt("修改排序标准", task.criteria)?.trim();
    if (!criteria || criteria === task.criteria) return;
    setHistoryBusy(true);
    setHistoryError(null);
    try {
      await updateSortTaskCriteria({ projectPath: project.projectPath, taskId: task.id, criteria });
      await onProjectDataChanged();
    } catch (cause) {
      setHistoryError(normalizeAppError(cause));
    } finally {
      setHistoryBusy(false);
    }
  }

  function taskNavigator(activeTaskId?: string) {
    return (
      <nav className="task-navigator" aria-label="排序任务">
        <div className="task-navigator-heading">
          <strong>排序任务</strong>
          <small>{sortTasks.length} 个</small>
        </div>
        {datasets.map((dataset) => {
          const tasks = sortTasks.filter((task) => task.datasetId === dataset.id);
          return (
            <section key={dataset.id} className="task-nav-dataset">
              <div>
                <span title={dataset.name}>{dataset.name}</span>
                <button
                  title={`基于 ${dataset.name} 新建任务`}
                  onClick={() => beginTaskCreation(dataset)}
                >
                  ＋
                </button>
              </div>
              {tasks.length === 0 ? (
                <small>暂无任务</small>
              ) : (
                <ul>
                  {tasks.map((task) => (
                    <li key={task.id} className={activeTaskId === task.id ? "active" : ""}>
                      <button
                        className="task-nav-open"
                        title={task.criteria}
                        onClick={() => openTask(task)}
                      >
                        <span>{task.name}</span>
                        <small>
                          {getSortModeDefinition(task.mode).shortLabel} · {statusLabel(task.status)}
                        </small>
                      </button>
                      <button
                        className="task-nav-result"
                        title="查看结果"
                        onClick={() => openResult(task)}
                      >
                        结果
                      </button>
                      <button
                        className="task-nav-delete"
                        title="删除任务"
                        disabled={historyBusy}
                        onClick={() => void removeTask(task)}
                      >
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          );
        })}
      </nav>
    );
  }

  const activeResultDataset = datasets.find(
    (dataset) => dataset.id === activeResultTask?.datasetId,
  );
  if (activeResultTask && activeResultDataset) {
    return (
      <div className="task-view-layout">
        <aside className="task-view-sidebar">
          <button className="task-sidebar-back" onClick={closeActiveTask}>
            ← 项目数据
          </button>
          {taskNavigator(activeResultTask.id)}
        </aside>
        <ResultWorkspace
          project={project}
          dataset={activeResultDataset}
          task={activeResultTask}
          onEdit={() => openTask(activeResultTask)}
          onProjectDataChanged={onProjectDataChanged}
        />
        {taskDataset && (
          <CreateSortTaskDialog
            project={project}
            dataset={taskDataset}
            sourceTasks={sortTasks}
            initialSourceTaskId={taskSourceResultId ?? undefined}
            onClose={closeTaskCreation}
            onCreated={onSortTaskCreated}
          />
        )}
      </div>
    );
  }
  if (activeDragTask) {
    return (
      <div className="task-view-layout">
        <aside className="task-view-sidebar">
          <button className="task-sidebar-back" onClick={closeActiveTask}>
            ← 项目数据
          </button>
          {taskNavigator(activeDragTask.id)}
        </aside>
        <DragSortWorkspace
          project={project}
          task={activeDragTask}
          onReview={() => openResult(activeDragTask)}
          allowNetworkImages={allowNetworkImages}
          mediaFieldNames={
            datasets
              .find((dataset) => dataset.id === activeDragTask.datasetId)
              ?.fields.filter((field) => field.fieldType === "image")
              .map((field) => field.name) ?? []
          }
          onProjectDataChanged={onProjectDataChanged}
        />
        {taskDataset && (
          <CreateSortTaskDialog
            project={project}
            dataset={taskDataset}
            sourceTasks={sortTasks}
            initialSourceTaskId={taskSourceResultId ?? undefined}
            onClose={closeTaskCreation}
            onCreated={onSortTaskCreated}
          />
        )}
      </div>
    );
  }
  if (activeSliderTask) {
    return (
      <div className="task-view-layout">
        <aside className="task-view-sidebar">
          <button className="task-sidebar-back" onClick={closeActiveTask}>
            ← 项目数据
          </button>
          {taskNavigator(activeSliderTask.id)}
        </aside>
        <SliderWorkspace
          project={project}
          task={activeSliderTask}
          onBack={closeActiveTask}
          onReview={() => openResult(activeSliderTask)}
          onFineTune={() => {
            setActiveSliderTask(null);
            setActiveDragTask(activeSliderTask);
          }}
          allowNetworkImages={allowNetworkImages}
          mediaFieldNames={
            datasets
              .find((dataset) => dataset.id === activeSliderTask.datasetId)
              ?.fields.filter((field) => field.fieldType === "image")
              .map((field) => field.name) ?? []
          }
          onProjectDataChanged={onProjectDataChanged}
        />
        {taskDataset && (
          <CreateSortTaskDialog
            project={project}
            dataset={taskDataset}
            sourceTasks={sortTasks}
            initialSourceTaskId={taskSourceResultId ?? undefined}
            onClose={closeTaskCreation}
            onCreated={onSortTaskCreated}
          />
        )}
      </div>
    );
  }
  if (activeComparisonTask) {
    return (
      <div className="task-view-layout">
        <aside className="task-view-sidebar">
          <button className="task-sidebar-back" onClick={closeActiveTask}>
            ← 项目数据
          </button>
          {taskNavigator(activeComparisonTask.id)}
        </aside>
        <ComparisonWorkspace
          project={project}
          task={activeComparisonTask}
          onBack={closeActiveTask}
          onReview={() => openResult(activeComparisonTask)}
          onFineTune={() => {
            setActiveComparisonTask(null);
            setActiveDragTask(activeComparisonTask);
          }}
          allowNetworkImages={allowNetworkImages}
          mediaFieldNames={
            datasets
              .find((dataset) => dataset.id === activeComparisonTask.datasetId)
              ?.fields.filter((field) => field.fieldType === "image")
              .map((field) => field.name) ?? []
          }
          onProjectDataChanged={onProjectDataChanged}
        />
        {taskDataset && (
          <CreateSortTaskDialog
            project={project}
            dataset={taskDataset}
            sourceTasks={sortTasks}
            initialSourceTaskId={taskSourceResultId ?? undefined}
            onClose={closeTaskCreation}
            onCreated={onSortTaskCreated}
          />
        )}
      </div>
    );
  }
  return (
    <main className="project-shell">
      <div className="project-topbar">
        <img className="brand-mark small" src={logo} alt="" aria-hidden="true" />
        <div className="topbar-project-info">
          <strong>{project.name}</strong>
          <span title={project.projectPath}>{project.projectPath}</span>
        </div>
        <span className="topbar-autosave">{autosaveLabel(autosave)}</span>
        <button className="topbar-settings" onClick={() => setSettingsOpen(true)}>
          设置
        </button>
        <button className="topbar-close" onClick={onCloseProject}>
          关闭项目
        </button>
      </div>
      <div className="workspace-layout">
        <aside className="workspace-sidebar">
          {taskNavigator()}
          <button className="snapshot-open-button" onClick={() => void openHistory()}>
            历史快照
            <small>最多保留最近 50 个</small>
          </button>
        </aside>
        <section className="workspace-content">
          {historyError && (
            <div className="error-banner compact" role="alert">
              <strong>{historyError.message}</strong>
              {historyError.detail && <span>{historyError.detail}</span>}
            </div>
          )}
          {mediaNotice && (
            <div className="success-banner" role="status">
              {mediaNotice}
            </div>
          )}
          {initialAutosave?.recoveredUncleanSession && (
            <div className="recovery-banner" role="status">
              <strong>已恢复上次异常关闭前的自动保存状态</strong>
              <span>
                {initialAutosave.lastAutosaveAt
                  ? `${formatTimestamp(initialAutosave.lastAutosaveAt)} · ${initialAutosave.lastAutosaveAction ?? "最近操作"}`
                  : "项目数据库中的已提交操作均已保留。"}
              </span>
            </div>
          )}
          <div className="workspace-actions">
            <button className="primary-button" onClick={() => setImportOpen(true)}>
              ＋ 导入数据
            </button>
          </div>
          {datasets.length === 0 ? (
            <div className="dataset-empty">
              <div className="empty-symbol">↥</div>
              <h2>从一组条目开始</h2>
              <p>支持 TXT、CSV、JSON，或直接粘贴多行文本。原始文件会以只读副本保存在项目中。</p>
              <button className="primary-button" onClick={() => setImportOpen(true)}>
                导入第一份数据
              </button>
            </div>
          ) : (
            <div className="dataset-list">
              {datasets.map((dataset) => {
                const datasetTasks = sortTasks.filter((task) => task.datasetId === dataset.id);
                const primaryField = dataset.fields.find((field) => field.isPrimaryIdentifier);
                const auxiliaryFields = dataset.fields
                  .filter((field) => field.isAuxiliaryIdentifier)
                  .map((field) => field.name)
                  .join("、");
                return (
                  <article className="dataset-row" key={dataset.id}>
                    <section className="dataset-summary" aria-label={`数据表 ${dataset.name}`}>
                      <div className="dataset-card-head">
                        <span className={`source-badge ${dataset.sourceType}`}>
                          {dataset.sourceType.toUpperCase()}
                        </span>
                        <span>
                          {dataset.itemCount} 条 · {dataset.fields.length} 个字段
                        </span>
                        <button
                          className="dataset-delete-button"
                          disabled={historyBusy}
                          title="删除数据表"
                          onClick={() => void removeDataset(dataset)}
                        >
                          删除
                        </button>
                      </div>
                      <h2>{dataset.name}</h2>
                      <dl>
                        <div>
                          <dt>主标识</dt>
                          <dd>{primaryField?.name ?? "—"}</dd>
                        </div>
                        <div>
                          <dt>辅助字段</dt>
                          <dd>{auxiliaryFields || "未选择"}</dd>
                        </div>
                      </dl>
                      <div className="saved-line">
                        <span />
                        已写入项目数据库
                      </div>
                      {dataset.fields.some((field) => field.fieldType === "image") && (
                        <button
                          className="media-folder-button"
                          disabled={mediaImportDatasetId !== null}
                          onClick={() => void importImages(dataset)}
                        >
                          {mediaImportDatasetId === dataset.id
                            ? "正在匹配图片…"
                            : "▧ 导入图片文件夹"}
                        </button>
                      )}
                    </section>
                    <section className="dataset-row-tasks" aria-label={`${dataset.name}的排序任务`}>
                      {datasetTasks.map((task) => (
                        <article
                          className="dataset-task-card"
                          key={task.id}
                          tabIndex={0}
                          aria-label={`打开排序任务“${task.name}”`}
                          onClick={(event) => {
                            if ((event.target as Element).closest("button")) return;
                            openTask(task);
                          }}
                          onKeyDown={(event) => {
                            if (event.target !== event.currentTarget) return;
                            if (event.key !== "Enter" && event.key !== " ") return;
                            event.preventDefault();
                            openTask(task);
                          }}
                        >
                          <div className="dataset-task-meta">
                            <span className={`task-mode ${task.mode}`}>
                              {getSortModeDefinition(task.mode).shortLabel}
                            </span>
                            {task.status === "confirmed" ? (
                              <span
                                className="dataset-task-lock"
                                aria-label="已锁定"
                                title="已锁定"
                              >
                                🔒
                              </span>
                            ) : (
                              <span>{statusLabel(task.status)}</span>
                            )}
                          </div>
                          <div className="dataset-task-open" title={task.criteria}>
                            <strong>{task.name}</strong>
                            <span>{task.criteria}</span>
                          </div>
                          <div className="dataset-task-stats">
                            <span>{task.rankGroupCount} 个排序组</span>
                          </div>
                          <div className="dataset-task-actions">
                            <button onClick={() => openResult(task)}>
                              {task.status === "confirmed" ? "查看结果" : "结果"}
                            </button>
                            <button
                              disabled={historyBusy}
                              onClick={() => void editTaskCriteria(task)}
                            >
                              修改标准
                            </button>
                            {task.status === "confirmed" && (
                              <button disabled={historyBusy} onClick={() => void quickUnlock(task)}>
                                解锁
                              </button>
                            )}
                            <button
                              className="danger"
                              disabled={historyBusy}
                              onClick={() => void removeTask(task)}
                            >
                              删除
                            </button>
                          </div>
                          {task.status === "confirmed" && (
                            <button
                              className="task-result-create-button"
                              disabled={historyBusy}
                              onClick={() => beginTaskCreation(dataset, task)}
                            >
                              <strong>＋ 新建排序任务</strong>
                              <small>将当前锁定结果视作新数据集</small>
                            </button>
                          )}
                        </article>
                      ))}
                      <button
                        className="dataset-task-add"
                        onClick={() => beginTaskCreation(dataset)}
                      >
                        <span>＋</span>
                        <strong>新建排序任务</strong>
                        <small>基于 {dataset.name}</small>
                      </button>
                    </section>
                  </article>
                );
              })}
            </div>
          )}
        </section>
      </div>
      {importOpen && (
        <DataImportDialog
          project={project}
          onClose={() => setImportOpen(false)}
          onImported={onDatasetImported}
        />
      )}
      {taskDataset && (
        <CreateSortTaskDialog
          project={project}
          dataset={taskDataset}
          sourceTasks={sortTasks}
          initialSourceTaskId={taskSourceResultId ?? undefined}
          onClose={closeTaskCreation}
          onCreated={onSortTaskCreated}
        />
      )}
      <Dialog.Root open={settingsOpen} onOpenChange={setSettingsOpen}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="dialog-content">
            <Dialog.Title className="dialog-title">项目设置</Dialog.Title>
            <Dialog.Description className="dialog-description">
              调整当前项目中的媒体显示方式。数据仍只保存在本地。
            </Dialog.Description>
            <label className="setting-row network-image-setting">
              <span>
                加载网络图片
                <small>开启后会访问数据表中的图片地址，单张图片 8 秒超时</small>
              </span>
              <input
                type="checkbox"
                checked={allowNetworkImages}
                onChange={(event) => onAllowNetworkImagesChange?.(event.target.checked)}
              />
            </label>
            <div className="setting-row project-storage-setting">
              <span>
                项目存储位置
                <small title={project.projectPath}>{project.projectPath}</small>
              </span>
              <strong>本地</strong>
            </div>
            <div className="dialog-actions">
              <Dialog.Close asChild>
                <button className="primary-button">完成</button>
              </Dialog.Close>
            </div>
            <Dialog.Close className="dialog-close" aria-label="关闭">
              ×
            </Dialog.Close>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
      <Dialog.Root open={historyOpen} onOpenChange={setHistoryOpen}>
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="dialog-content snapshot-dialog">
            <Dialog.Title className="dialog-title">历史快照</Dialog.Title>
            <Dialog.Description className="dialog-description">
              关键业务节点会保存完整项目副本。恢复前会自动创建当前状态的安全备份。
            </Dialog.Description>
            {historyError && (
              <div className="error-banner" role="alert">
                <strong>{historyError.message}</strong>
                {historyError.detail && <span>{historyError.detail}</span>}
              </div>
            )}
            {historyBusy && snapshots.length === 0 ? (
              <div className="snapshot-empty">正在读取快照…</div>
            ) : snapshots.length === 0 ? (
              <div className="snapshot-empty">尚未创建历史快照</div>
            ) : (
              <div className="snapshot-list">
                {snapshots.map((snapshot) => (
                  <article key={snapshot.id} className="snapshot-row">
                    <div>
                      <strong>{snapshot.label}</strong>
                      <span>{formatTimestamp(snapshot.createdAt)}</span>
                      <small>{formatBytes(snapshot.sizeBytes)}</small>
                    </div>
                    <button
                      className="secondary-button"
                      disabled={historyBusy}
                      onClick={() => void restoreSnapshot(snapshot)}
                    >
                      恢复
                    </button>
                  </article>
                ))}
              </div>
            )}
            <div className="dialog-actions">
              <Dialog.Close asChild>
                <button className="primary-button" disabled={historyBusy}>
                  完成
                </button>
              </Dialog.Close>
            </div>
            <Dialog.Close className="dialog-close" aria-label="关闭" disabled={historyBusy}>
              ×
            </Dialog.Close>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </main>
  );
}

function statusLabel(status: SortTaskOverview["status"]): string {
  if (status === "confirmed") return "🔒 已锁定";
  if (status === "sorting") return "进行中";
  return "草稿";
}

function autosaveLabel(status: AutosaveStatus | null): string {
  if (!status?.lastAutosaveAt) return "自动保存已启用";
  return `已自动保存 ${new Intl.DateTimeFormat("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  }).format(new Date(status.lastAutosaveAt))}`;
}

function formatTimestamp(value: string): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

function formatBytes(value: number): string {
  if (value < 1024 * 1024) return `${Math.max(1, Math.round(value / 1024))} KB`;
  return `${(value / (1024 * 1024)).toFixed(1)} MB`;
}
