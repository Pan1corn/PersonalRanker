import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { type FormEvent, useState } from "react";
import { normalizeAppError } from "../../lib/errors";
import { createProject } from "./project-api";
import { useProjectStore } from "./project-store";

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

function defaultName(): string {
  const date = new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
  })
    .format(new Date())
    .replaceAll("/", "");
  return `未命名排序项目_${date}`;
}

export function CreateProjectDialog({ open: isOpen, onOpenChange }: Props) {
  const [name, setName] = useState(defaultName);
  const [parentDirectory, setParentDirectory] = useState("");
  const { busy, setBusy, setError, setProject } = useProjectStore();

  async function chooseDirectory() {
    const selected = await open({ directory: true, multiple: false, title: "选择项目保存位置" });
    if (selected) setParentDirectory(selected);
  }

  async function submit(event: FormEvent) {
    event.preventDefault();
    if (!name.trim() || !parentDirectory) return;
    setBusy(true);
    setError(null);
    try {
      setProject(await createProject(name.trim(), parentDirectory));
      onOpenChange(false);
    } catch (error) {
      setError(normalizeAppError(error));
    } finally {
      setBusy(false);
    }
  }

  return (
    <Dialog.Root open={isOpen} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="dialog-overlay" />
        <Dialog.Content className="dialog-content" aria-describedby="create-description">
          <Dialog.Title className="dialog-title">新建排序项目</Dialog.Title>
          <Dialog.Description id="create-description" className="dialog-description">
            项目会保存为一个 .subject-sort 文件夹，导入源文件不会被改写。
          </Dialog.Description>
          <form onSubmit={(event) => void submit(event)}>
            <label className="field-label" htmlFor="project-name">
              项目名称
            </label>
            <input
              id="project-name"
              className="text-input"
              value={name}
              onChange={(event) => setName(event.target.value)}
              autoFocus
            />
            <label className="field-label" htmlFor="project-location">
              保存位置
            </label>
            <div className="path-row">
              <input
                id="project-location"
                className="text-input"
                value={parentDirectory}
                readOnly
                placeholder="请选择文件夹"
              />
              <button
                className="secondary-button"
                type="button"
                onClick={() => void chooseDirectory()}
              >
                浏览
              </button>
            </div>
            <div className="dialog-actions">
              <Dialog.Close asChild>
                <button className="secondary-button" type="button">
                  取消
                </button>
              </Dialog.Close>
              <button
                className="primary-button"
                type="submit"
                disabled={busy || !name.trim() || !parentDirectory}
              >
                {busy ? "正在创建…" : "创建项目"}
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
