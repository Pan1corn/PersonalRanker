import * as Dialog from "@radix-ui/react-dialog";
import { open } from "@tauri-apps/plugin-dialog";
import { useState } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import type { ProjectMetadata } from "../projects/types";
import {
  commitDatasetImport,
  inspectJsonArrayNodes,
  previewImportFile,
  previewPastedText,
} from "./import-api";
import type {
  DatasetOverview,
  ImportedFieldType,
  ImportResult,
  JsonArrayNode,
  JsonValue,
} from "./types";

interface Props {
  project: ProjectMetadata;
  onClose: () => void;
  onImported: (dataset: DatasetOverview) => void;
}

type SourceMode = "file" | "paste";

const TYPE_LABELS: Record<ImportedFieldType, string> = {
  text: "文本",
  image: "图片",
  number: "数字",
  boolean: "布尔值",
  date: "日期",
  null: "空值",
  object: "对象",
  array: "数组",
  mixed: "混合",
};

export function DataImportDialog({ project, onClose, onImported }: Props) {
  const [mode, setMode] = useState<SourceMode>("file");
  const [filePath, setFilePath] = useState("");
  const [pastedText, setPastedText] = useState("");
  const [datasetName, setDatasetName] = useState("未命名数据集");
  const [jsonNodes, setJsonNodes] = useState<JsonArrayNode[]>([]);
  const [jsonPointer, setJsonPointer] = useState<string | undefined>();
  const [deduplicate, setDeduplicate] = useState(false);
  const [preview, setPreview] = useState<ImportResult | null>(null);
  const [primary, setPrimary] = useState("");
  const [auxiliary, setAuxiliary] = useState<string[]>([]);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<AppErrorPayload | null>(null);

  function applyPreview(result: ImportResult) {
    setPreview(result);
    const recommended =
      result.fields.find(
        (field) => field.emptyCount === 0 && field.uniqueCount === result.items.length,
      ) ?? result.fields[0];
    setPrimary(recommended?.name ?? "");
    setAuxiliary([]);
  }

  async function chooseFile() {
    const selected = await open({
      multiple: false,
      title: "选择要导入的数据文件",
      filters: [{ name: "支持的数据文件", extensions: ["txt", "csv", "json", "md"] }],
    });
    if (!selected) return;
    setFilePath(selected);
    setDatasetName(filenameWithoutExtension(selected));
    setPreview(null);
    setError(null);
    let pointer: string | undefined;
    if (selected.toLowerCase().endsWith(".json")) {
      try {
        const nodes = await inspectJsonArrayNodes(selected);
        setJsonNodes(nodes);
        pointer = nodes[0]?.pointer;
        setJsonPointer(pointer);
      } catch (cause) {
        setError(normalizeAppError(cause));
        return;
      }
    } else {
      setJsonNodes([]);
      setJsonPointer(undefined);
    }
    await loadFilePreview(selected, pointer, deduplicate);
  }

  async function loadFilePreview(path = filePath, pointer = jsonPointer, dedup = deduplicate) {
    if (!path) return;
    setBusy(true);
    setError(null);
    try {
      applyPreview(
        await previewImportFile({
          path,
          deduplicate: dedup,
          ...(pointer === undefined ? {} : { jsonPointer: pointer }),
        }),
      );
    } catch (cause) {
      setPreview(null);
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function loadPastedPreview(dedup = deduplicate) {
    setBusy(true);
    setError(null);
    try {
      applyPreview(await previewPastedText(pastedText, dedup));
      if (datasetName === "未命名数据集") setDatasetName("粘贴文本");
    } catch (cause) {
      setPreview(null);
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
    }
  }

  async function changeDeduplicate(checked: boolean) {
    setDeduplicate(checked);
    if (mode === "file") await loadFilePreview(filePath, jsonPointer, checked);
    else await loadPastedPreview(checked);
  }

  async function changeJsonPointer(pointer: string) {
    setJsonPointer(pointer);
    await loadFilePreview(filePath, pointer, deduplicate);
  }

  function changePrimary(name: string) {
    setPrimary(name);
    setAuxiliary((current) => current.filter((field) => field !== name));
  }

  function toggleAuxiliary(name: string, checked: boolean) {
    setAuxiliary((current) =>
      checked ? [...current, name] : current.filter((field) => field !== name),
    );
  }

  async function confirmImport() {
    if (!preview || !primary || !datasetName.trim()) return;
    setBusy(true);
    setError(null);
    try {
      const saved = await commitDatasetImport({
        projectPath: project.projectPath,
        datasetName: datasetName.trim(),
        source:
          mode === "file"
            ? { kind: "file", path: filePath }
            : { kind: "pasted_text", content: pastedText },
        deduplicate,
        ...(jsonPointer === undefined ? {} : { jsonPointer }),
        primaryIdentifier: primary,
        auxiliaryIdentifiers: auxiliary,
      });
      onImported(saved);
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
          className="dialog-content import-dialog"
          aria-describedby="import-description"
        >
          <Dialog.Title className="dialog-title">导入并配置数据</Dialog.Title>
          <Dialog.Description id="import-description" className="dialog-description">
            先检查数据质量，再选择排序时用于识别条目的字段。
          </Dialog.Description>

          <div className="source-tabs" role="tablist">
            <button
              className={mode === "file" ? "active" : ""}
              onClick={() => {
                setMode("file");
                setPreview(null);
              }}
            >
              文件导入
            </button>
            <button
              className={mode === "paste" ? "active" : ""}
              onClick={() => {
                setMode("paste");
                setPreview(null);
              }}
            >
              粘贴文本
            </button>
          </div>

          {mode === "file" ? (
            <div className="source-picker">
              <button
                className="secondary-button"
                onClick={() => void chooseFile()}
                disabled={busy}
              >
                选择 TXT / CSV / JSON / Markdown
              </button>
              <span title={filePath}>{filePath || "尚未选择文件"}</span>
              {jsonNodes.length > 1 && (
                <label>
                  JSON 数组节点
                  <select
                    value={jsonPointer}
                    onChange={(event) => void changeJsonPointer(event.target.value)}
                  >
                    {jsonNodes.map((node) => (
                      <option key={node.pointer} value={node.pointer}>
                        {node.pointer || "根数组"}（{node.itemCount} 项）
                      </option>
                    ))}
                  </select>
                </label>
              )}
            </div>
          ) : (
            <div className="paste-source">
              <textarea
                value={pastedText}
                onChange={(event) => setPastedText(event.target.value)}
                placeholder="每个非空行会成为一个条目"
                rows={5}
              />
              <button
                className="secondary-button"
                onClick={() => void loadPastedPreview()}
                disabled={busy || !pastedText.trim()}
              >
                解析文本
              </button>
            </div>
          )}

          {error && (
            <div className="error-banner compact" role="alert">
              <strong>{error.message}</strong>
              {error.detail && error.detail !== error.message && <span>{error.detail}</span>}
            </div>
          )}
          {busy && <div className="loading-line">正在处理本地数据…</div>}

          {preview && (
            <div className="preview-area">
              <div className="preview-stats">
                <Stat value={preview.items.length} label="有效条目" />
                <Stat value={preview.fields.length} label="字段" />
                <Stat value={preview.duplicateCount} label="重复条目" />
                <Stat value={preview.removedEmptyCount} label="已移除空条目" />
              </div>
              <label className="deduplicate-option">
                <input
                  type="checkbox"
                  checked={deduplicate}
                  onChange={(event) => void changeDeduplicate(event.target.checked)}
                />
                导入时去除完全重复的条目
              </label>
              <div className="field-table-wrap">
                <table className="field-table">
                  <thead>
                    <tr>
                      <th>主标识</th>
                      <th>辅助</th>
                      <th>字段</th>
                      <th>类型</th>
                      <th>空值</th>
                      <th>唯一值</th>
                      <th>样例</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.fields.map((field) => (
                      <tr key={field.name}>
                        <td>
                          <input
                            aria-label={`${field.name}作为主标识`}
                            type="radio"
                            name="primary"
                            checked={primary === field.name}
                            onChange={() => changePrimary(field.name)}
                          />
                        </td>
                        <td>
                          <input
                            aria-label={`${field.name}作为辅助标识`}
                            type="checkbox"
                            disabled={primary === field.name}
                            checked={auxiliary.includes(field.name)}
                            onChange={(event) => toggleAuxiliary(field.name, event.target.checked)}
                          />
                        </td>
                        <td>
                          <strong>{field.name}</strong>
                        </td>
                        <td>
                          <span className="type-chip">{TYPE_LABELS[field.fieldType]}</span>
                        </td>
                        <td>{field.emptyCount}</td>
                        <td>{field.uniqueCount}</td>
                        <td className="samples">
                          {field.samples.map(formatSample).join(" · ") || "—"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <div className="configuration-row">
                <label>
                  数据集名称
                  <input
                    className="text-input"
                    value={datasetName}
                    onChange={(event) => setDatasetName(event.target.value)}
                  />
                </label>
                <div className="selection-summary">
                  <span>
                    主标识：<strong>{primary}</strong>
                  </span>
                  <span>
                    辅助字段：<strong>{auxiliary.length ? auxiliary.join("、") : "未选择"}</strong>
                  </span>
                </div>
              </div>
            </div>
          )}

          <div className="dialog-actions">
            <button className="secondary-button" onClick={onClose}>
              取消
            </button>
            <button
              className="primary-button"
              disabled={!preview || !primary || !datasetName.trim() || busy}
              onClick={() => void confirmImport()}
            >
              {busy ? "正在保存…" : "确认导入"}
            </button>
          </div>
          <Dialog.Close className="dialog-close" aria-label="关闭">
            ×
          </Dialog.Close>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

function Stat({ value, label }: { value: number; label: string }) {
  return (
    <div className="stat-card">
      <strong>{value}</strong>
      <span>{label}</span>
    </div>
  );
}

function filenameWithoutExtension(path: string): string {
  const filename = path.split(/[\\/]/).pop() ?? "未命名数据集";
  return filename.replace(/\.[^.]+$/, "") || "未命名数据集";
}

function formatSample(value: JsonValue): string {
  if (typeof value === "string") return value;
  return JSON.stringify(value);
}
