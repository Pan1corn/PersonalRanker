/**
 * 应用根组件：负责项目打开/关闭、全局设置和顶层数据加载。
 * 具体业务操作下沉到各 feature；这里维护跨工作区共享的项目上下文。
 */
import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import logo from "./assets/logo.png";
import { MarkdownDocument } from "./components/MarkdownDocument";
import { GITHUB_REPOSITORY_URL } from "./config/app-links";
import updateNotes from "./content/UPDATE_NOTES.md?raw";
import { listDatasets } from "./features/import/import-api";
import { CreateProjectDialog } from "./features/projects/CreateProjectDialog";
import { ProjectWorkspace } from "./features/projects/ProjectWorkspace";
import { closeProject, openProject } from "./features/projects/project-api";
import { loadLastProjectParent, rememberProjectParent } from "./features/projects/project-path";
import { useProjectStore } from "./features/projects/project-store";
import { listSortTasks } from "./features/sort-tasks/sort-task-api";
import { normalizeAppError } from "./lib/errors";
import type { AutosaveStatus } from "./features/projects/types";

export function App() {
  const [createOpen, setCreateOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [updateNotesOpen, setUpdateNotesOpen] = useState(false);
  const [lastProjectParent, setLastProjectParent] = useState(loadLastProjectParent);
  const [openAutosave, setOpenAutosave] = useState<AutosaveStatus | null>(null);
  const [allowNetworkImages, setAllowNetworkImages] = useState(
    () => localStorage.getItem("subjective-sorter:network-images") === "enabled",
  );
  const {
    currentProject,
    datasets,
    sortTasks,
    busy,
    error,
    setBusy,
    setError,
    setProject,
    setDatasets,
    addDataset,
    setSortTasks,
    addSortTask,
  } = useProjectStore();

  useEffect(() => {
    document.body.classList.toggle("landing-page", !currentProject);
    return () => document.body.classList.remove("landing-page");
  }, [currentProject]);

  async function refreshProjectData() {
    if (!currentProject) return;
    // 数据表和任务可能被任一子工作区修改，因此统一从 SQLite 重新读取两个集合。
    const [savedDatasets, savedTasks] = await Promise.all([
      listDatasets(currentProject.projectPath),
      listSortTasks(currentProject.projectPath),
    ]);
    setDatasets(savedDatasets);
    setSortTasks(savedTasks);
  }

  async function chooseProject() {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "打开 .subject-sort 项目",
      defaultPath: lastProjectParent,
    });
    if (!selected) return;
    rememberProjectParent(selected);
    setLastProjectParent(loadLastProjectParent());
    setBusy(true);
    setError(null);
    try {
      // 先完成项目结构和版本校验，再加载依赖该项目数据库的概览数据。
      const opened = await openProject(selected);
      const project = opened.project;
      const [savedDatasets, savedTasks] = await Promise.all([
        listDatasets(project.projectPath),
        listSortTasks(project.projectPath),
      ]);
      setProject(project);
      setOpenAutosave(opened.autosave);
      setDatasets(savedDatasets);
      setSortTasks(savedTasks);
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function handleCloseProject() {
    if (!currentProject) return;
    try {
      await closeProject(currentProject.projectPath);
      setProject(null);
      setOpenAutosave(null);
    } catch (cause) {
      setError(normalizeAppError(cause));
    }
  }

  function changeNetworkImages(enabled: boolean) {
    setAllowNetworkImages(enabled);
    localStorage.setItem("subjective-sorter:network-images", enabled ? "enabled" : "disabled");
  }

  if (currentProject) {
    return (
      <ProjectWorkspace
        project={currentProject}
        datasets={datasets}
        sortTasks={sortTasks}
        onDatasetImported={addDataset}
        onSortTaskCreated={addSortTask}
        onProjectDataChanged={refreshProjectData}
        initialAutosave={openAutosave}
        allowNetworkImages={allowNetworkImages}
        onAllowNetworkImagesChange={changeNetworkImages}
        onCloseProject={() => void handleCloseProject()}
      />
    );
  }

  return (
    <main className="landing antialiased">
      <section className="hero">
        <img className="brand-mark" src={logo} alt="主观排序器 Logo" />
        <p className="eyebrow">PERSONAL RANKER</p>
        <h1>
          把难以量化的判断，
          <br />
          变成清晰的顺序。
        </h1>
        <p className="subtitle">
          数据只保存在本地。导入条目，从四种排序方式中选择最适合你的判断方式。
        </p>
      </section>
      <section className="action-panel" aria-label="开始使用">
        <button className="action-card featured" onClick={() => setCreateOpen(true)}>
          <span className="action-icon">＋</span>
          <span>
            <strong>新建排序项目</strong>
            <small>创建一个本地项目文件夹</small>
          </span>
          <span className="arrow">→</span>
        </button>
        <button className="action-card" onClick={() => void chooseProject()} disabled={busy}>
          <span className="action-icon">↗</span>
          <span>
            <strong>{busy ? "正在打开…" : "打开项目文件"}</strong>
            <small>继续已有的 .subject-sort 项目</small>
          </span>
          <span className="arrow">→</span>
        </button>
        <button className="action-card" onClick={() => setSettingsOpen(true)}>
          <span className="action-icon">⌘</span>
          <span>
            <strong>应用设置</strong>
            <small>外观、快捷键与数据选项</small>
          </span>
          <span className="arrow">→</span>
        </button>
        {error && (
          <div className="error-banner" role="alert">
            <strong>{error.message}</strong>
            {error.detail && <span>{error.detail}</span>}
          </div>
        )}
      </section>
      <footer>
        <span>本地优先</span>
        <i />
        所有数据默认不会离开你的电脑
      </footer>
      <CreateProjectDialog open={createOpen} onOpenChange={setCreateOpen} />
      <Dialog.Root
        open={settingsOpen}
        onOpenChange={(open) => {
          setSettingsOpen(open);
          if (!open) setUpdateNotesOpen(false);
        }}
      >
        <Dialog.Portal>
          <Dialog.Overlay className="dialog-overlay" />
          <Dialog.Content className="dialog-content">
            <Dialog.Title className="dialog-title">应用设置</Dialog.Title>
            <Dialog.Description className="dialog-description">
              设置功能将在后续版本逐步开放；当前版本始终采用本地存储。
            </Dialog.Description>
            <div className="setting-row">
              <span>数据存储</span>
              <strong>仅本机</strong>
            </div>
            <label className="setting-row network-image-setting">
              <span>
                加载网络图片
                <small>关闭时不会向图片地址发起任何网络请求</small>
              </span>
              <input
                type="checkbox"
                checked={allowNetworkImages}
                onChange={(event) => changeNetworkImages(event.target.checked)}
              />
            </label>
            <div className="setting-row setting-link-row">
              <span>
                GitHub 仓库
                <small>查看源代码、提交问题与跟踪后续版本</small>
              </span>
              {GITHUB_REPOSITORY_URL ? (
                <a
                  className="setting-link-button"
                  href={GITHUB_REPOSITORY_URL}
                  target="_blank"
                  rel="noreferrer"
                >
                  打开仓库 ↗
                </a>
              ) : (
                <button className="setting-link-button" disabled title="仓库地址尚未配置">
                  待配置
                </button>
              )}
            </div>
            <div className="setting-row setting-link-row">
              <span>
                更新说明
                <small>查看当前版本的新增功能与调整</small>
              </span>
              <Dialog.Root open={updateNotesOpen} onOpenChange={setUpdateNotesOpen}>
                <Dialog.Trigger asChild>
                  <button className="setting-link-button">查看</button>
                </Dialog.Trigger>
                <Dialog.Portal>
                  <Dialog.Overlay className="dialog-overlay update-notes-overlay" />
                  <Dialog.Content className="dialog-content update-notes-dialog">
                    <Dialog.Title className="dialog-title">更新说明</Dialog.Title>
                    <Dialog.Description className="dialog-description">
                      查看当前版本的新增功能、功能调整与修复内容。
                    </Dialog.Description>
                    <section className="update-notes-panel" aria-label="更新说明内容">
                      <MarkdownDocument content={updateNotes} />
                    </section>
                    <div className="dialog-actions">
                      <Dialog.Close asChild>
                        <button className="primary-button">关闭</button>
                      </Dialog.Close>
                    </div>
                  </Dialog.Content>
                </Dialog.Portal>
              </Dialog.Root>
            </div>
            <div className="dialog-actions">
              <Dialog.Close asChild>
                <button className="primary-button">完成</button>
              </Dialog.Close>
            </div>
          </Dialog.Content>
        </Dialog.Portal>
      </Dialog.Root>
    </main>
  );
}
