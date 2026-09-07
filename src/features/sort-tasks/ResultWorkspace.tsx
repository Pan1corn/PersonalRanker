/**
 * 结果确认、排名/评分写入与导出工作台。
 * 所有破坏性写入都先校验或预览，并由后端事务保证项目工作副本的一致性。
 */
import { confirm, message, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { normalizeAppError, type AppErrorPayload } from "../../lib/errors";
import type { DatasetOverview } from "../import/types";
import type { ProjectMetadata } from "../projects/types";
import {
  confirmSortResult,
  exportSortResult,
  exportTargetExists,
  loadScoreConfig,
  loadResultPreview,
  previewScores,
  saveScoreConfig,
  unlockSortResult,
  writeRankField,
  writeScores,
} from "./result-api";
import type {
  ExportFormat,
  ExportOrder,
  RankRule,
  ResultPreviewState,
  ScoreConfig,
  ScorePreview,
  SortTaskOverview,
  TieScoreRule,
} from "./types";

interface Props {
  project: ProjectMetadata;
  dataset: DatasetOverview;
  task: SortTaskOverview;
  onEdit: () => void;
  onProjectDataChanged: () => Promise<void>;
}

export function ResultWorkspace({ project, dataset, task, onEdit, onProjectDataChanged }: Props) {
  const [preview, setPreview] = useState<ResultPreviewState | null>(null);
  const [busy, setBusy] = useState(false);
  const [busyLabel, setBusyLabel] = useState("");
  const [error, setError] = useState<AppErrorPayload | null>(null);
  const [notice, setNotice] = useState("");
  const [rankFieldName, setRankFieldName] = useState("rank");
  const [rankRule, setRankRule] = useState<RankRule>("competition");
  const [scoreFieldName, setScoreFieldName] = useState("score");
  const [scoreMethod, setScoreMethod] = useState<"linear" | "buckets">("linear");
  const [highestScore, setHighestScore] = useState("5");
  const [lowestScore, setLowestScore] = useState("1");
  const [decimalPlaces, setDecimalPlaces] = useState("2");
  const [highRankHighScore, setHighRankHighScore] = useState(true);
  const [bucketLevels, setBucketLevels] = useState("5, 4, 3, 2, 1");
  const [tieScoreRule, setTieScoreRule] = useState<TieScoreRule>("average");
  const [scorePreview, setScorePreview] = useState<ScorePreview | null>(null);
  const [format, setFormat] = useState<ExportFormat>("csv");
  const [order, setOrder] = useState<ExportOrder>("final");
  const [fieldMode, setFieldMode] = useState<"all" | "selected">("all");
  const [selectedFields, setSelectedFields] = useState<string[]>(
    dataset.fields.map((field) => field.name),
  );
  const [includeRank, setIncludeRank] = useState(true);
  const [includeOriginalIndex, setIncludeOriginalIndex] = useState(true);
  const [includeScore, setIncludeScore] = useState(false);
  const [scoreConfigReady, setScoreConfigReady] = useState(false);

  useEffect(() => {
    let active = true;
    void loadResultPreview({ projectPath: project.projectPath, taskId: task.id })
      .then((result) => active && setPreview(result))
      .catch((cause) => active && setError(normalizeAppError(cause)));
    return () => {
      active = false;
    };
  }, [project.projectPath, task.id]);

  useEffect(() => {
    let active = true;
    void loadScoreConfig({ projectPath: project.projectPath, taskId: task.id })
      .then((config) => {
        if (!active || !config) return;
        setScoreFieldName(config.fieldName);
        setTieScoreRule(config.tieRule);
        setScoreMethod(config.method.kind);
        if (config.method.kind === "linear") {
          setHighestScore(String(config.method.highestScore));
          setLowestScore(String(config.method.lowestScore));
          setDecimalPlaces(String(config.method.decimalPlaces));
          setHighRankHighScore(config.method.highRankHighScore);
        } else {
          setBucketLevels(config.method.levels.join(", "));
        }
      })
      .catch((cause) => active && setError(normalizeAppError(cause)))
      .finally(() => active && setScoreConfigReady(true));
    return () => {
      active = false;
    };
  }, [project.projectPath, task.id]);

  const autosaveScoreConfig = useMemo(
    () =>
      buildScoreConfig({
        scoreFieldName,
        scoreMethod,
        highestScore,
        lowestScore,
        decimalPlaces,
        highRankHighScore,
        bucketLevels,
        tieScoreRule,
      }),
    [
      bucketLevels,
      decimalPlaces,
      highRankHighScore,
      highestScore,
      lowestScore,
      scoreFieldName,
      scoreMethod,
      tieScoreRule,
    ],
  );

  useEffect(() => {
    if (!scoreConfigReady || !autosaveScoreConfig) return;
    // 防抖保存让连续输入只提交最终配置，同时避免初次加载旧配置时反向覆盖数据库。
    const timer = window.setTimeout(() => {
      void saveScoreConfig({
        projectPath: project.projectPath,
        taskId: task.id,
        config: autosaveScoreConfig,
      }).catch((cause) => setError(normalizeAppError(cause)));
    }, 650);
    return () => window.clearTimeout(timer);
  }, [autosaveScoreConfig, project.projectPath, scoreConfigReady, task.id]);

  const fieldNames = useMemo(() => {
    const names = preview?.items[0]?.fields.map((field) => field.name);
    return names?.length ? names : dataset.fields.map((field) => field.name);
  }, [dataset.fields, preview]);
  const hasIntegrityFailure = Boolean(
    preview &&
    (!preview.integrity.allItemsIncluded ||
      !preview.integrity.noUnresolvedComparisons ||
      !preview.integrity.noDuplicateItems ||
      !preview.integrity.noInvalidItems ||
      preview.integrity.problems.length > 0),
  );

  async function run(
    operation: () => Promise<ResultPreviewState>,
    success: string,
    progressLabel: string,
  ) {
    // 统一需要刷新项目概览的结果操作，保证侧栏锁定状态与当前页面同步。
    setBusy(true);
    setBusyLabel(progressLabel);
    setError(null);
    setNotice("");
    try {
      const result = await operation();
      setPreview(result);
      setNotice(success);
      await onProjectDataChanged();
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
      setBusyLabel("");
    }
  }

  async function handleConfirm() {
    let accepted = false;
    try {
      accepted = await confirm(
        "确认后将创建当前排名的只读快照，并锁定排序工作台。之后仍可解锁继续编辑。",
        {
          title: "确认最终结果",
          kind: "info",
          okLabel: "确认并锁定",
          cancelLabel: "返回检查",
        },
      );
    } catch (cause) {
      setError(normalizeAppError(cause));
      return;
    }
    if (!accepted) return;
    await run(
      () => confirmSortResult({ projectPath: project.projectPath, taskId: task.id }),
      "结果已确认并创建快照。",
      "正在检查完整性并创建确认快照…",
    );
  }

  async function handleUnlock() {
    const accepted = await confirm("解锁后可以继续调整顺序。最近一次确认快照会保留，不会被删除。", {
      title: "解锁结果",
      kind: "warning",
      okLabel: "解锁并编辑",
      cancelLabel: "保持锁定",
    });
    if (!accepted) return;
    await run(
      () => unlockSortResult({ projectPath: project.projectPath, taskId: task.id }),
      "结果已解锁，最近确认版本仍已保留。",
      "正在解锁结果…",
    );
    onEdit();
  }

  async function handleWriteRank() {
    const normalized = rankFieldName.trim();
    if (!normalized) {
      setError({ code: "INVALID_FIELD", message: "请输入排名字段名称。" });
      return;
    }
    const exists = fieldNames.includes(normalized);
    let overwrite = false;
    if (exists) {
      overwrite = await confirm(`字段“${normalized}”已存在，确定用最终排名覆盖其当前值吗？`, {
        title: "覆盖已有字段",
        kind: "warning",
        okLabel: "确认覆盖",
        cancelLabel: "取消",
      });
      if (!overwrite) return;
    }
    await run(
      () =>
        writeRankField({
          projectPath: project.projectPath,
          taskId: task.id,
          fieldName: normalized,
          overwrite,
          rankRule,
        }),
      `排名已写入字段“${normalized}”。`,
      `正在写入排名字段“${normalized}”…`,
    );
    setSelectedFields((current) =>
      current.includes(normalized) ? current : [...current, normalized],
    );
  }

  function scoreConfig(): ScoreConfig | null {
    if (autosaveScoreConfig) return autosaveScoreConfig;
    setError({
      code: "INVALID_SCORE_CONFIG",
      message: "请检查评分字段、分数范围、小数位数或分档设置。",
    });
    return null;
  }

  async function handlePreviewScores() {
    const config = scoreConfig();
    if (!config) return;
    setBusy(true);
    setBusyLabel("正在计算评分并读取旧字段值…");
    setError(null);
    setNotice("");
    try {
      setScorePreview(
        await previewScores({ projectPath: project.projectPath, taskId: task.id, config }),
      );
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
      setBusyLabel("");
    }
  }

  async function handleWriteScores() {
    const config = scoreConfig();
    if (!config || !scorePreview) return;
    let overwrite = false;
    if (scorePreview.fieldExists) {
      overwrite = await confirm(`字段“${config.fieldName}”已存在，确定覆盖表格中预览的旧值吗？`, {
        title: "覆盖评分字段",
        kind: "warning",
        okLabel: "确认写入",
        cancelLabel: "返回预览",
      });
      if (!overwrite) return;
    }
    setBusy(true);
    setBusyLabel(`正在写入评分字段“${config.fieldName}”…`);
    setError(null);
    try {
      const written = await writeScores({
        projectPath: project.projectPath,
        taskId: task.id,
        config,
        overwrite,
      });
      setScorePreview(written);
      setNotice(`评分已写入字段“${config.fieldName}”。`);
      setSelectedFields((current) =>
        current.includes(config.fieldName) ? current : [...current, config.fieldName],
      );
      setIncludeScore(true);
      await onProjectDataChanged();
      setPreview(await loadResultPreview({ projectPath: project.projectPath, taskId: task.id }));
    } catch (cause) {
      setError(normalizeAppError(cause));
    } finally {
      setBusy(false);
      setBusyLabel("");
    }
  }

  async function handleExport() {
    if (!preview || preview.status !== "confirmed") return;
    const extension = exportExtension(format);
    const formatLabel = exportFormatLabel(format);
    const selected = [...(fieldMode === "all" ? fieldNames : selectedFields)];
    if (
      includeScore &&
      fieldNames.includes(scoreFieldName.trim()) &&
      !selected.includes(scoreFieldName.trim())
    ) {
      selected.push(scoreFieldName.trim());
    }
    if (selected.length === 0 && !includeRank && !includeOriginalIndex) {
      setError({ code: "EMPTY_EXPORT", message: "请至少选择一个导出字段。" });
      return;
    }
    const defaultPath = `${project.projectPath}\\${defaultFilename(dataset, task, extension)}`;
    let target = await save({
      title: "导出排序结果",
      defaultPath,
      filters: [{ name: formatLabel, extensions: [extension] }],
    });
    // “另存为”可能再次选中已存在文件，因此循环直到得到可写路径或用户取消。
    while (target) {
      target = ensureExtension(target, extension);
      let overwrite = false;
      if (await exportTargetExists(target)) {
        const choice = await message("目标文件已存在。请选择如何继续。", {
          title: "文件已存在",
          kind: "warning",
          buttons: { yes: "覆盖", no: "另存为", cancel: "取消" },
        });
        if (choice === "Cancel" || choice === "取消") return;
        if (choice === "No" || choice === "另存为") {
          target = await save({
            title: "将排序结果另存为",
            defaultPath: target,
            filters: [{ name: formatLabel, extensions: [extension] }],
          });
          continue;
        }
        overwrite = true;
      }
      setBusy(true);
      setBusyLabel(`正在生成 ${formatLabel} 文件…`);
      setError(null);
      setNotice("");
      try {
        const receipt = await exportSortResult({
          projectPath: project.projectPath,
          taskId: task.id,
          targetPath: target,
          format,
          order,
          selectedFields: selected,
          includeRank,
          rankFieldName: rankFieldName.trim() || "rank",
          includeOriginalIndex,
          overwrite,
        });
        setNotice(`已导出 ${receipt.rowCount} 条记录：${receipt.path}`);
      } catch (cause) {
        setError(normalizeAppError(cause));
      } finally {
        setBusy(false);
        setBusyLabel("");
      }
      return;
    }
  }

  return (
    <main className="project-shell result-shell">
      <section className="result-workspace">
        <header className="result-heading">
          <div>
            <p className="eyebrow">RESULT & EXPORT</p>
            <h1>{preview?.taskName ?? task.name}</h1>
            <p>检查最终顺序，确认后写入排名字段或导出独立文件。</p>
          </div>
          <span className={`result-status ${preview?.status ?? "sorting"}`}>
            {preview?.status === "confirmed" ? "已确认 · 已锁定" : "待确认"}
          </span>
        </header>

        {busy && (
          <div className="result-progress" role="status" aria-live="polite">
            <div className="result-progress-track">
              <span />
            </div>
            <strong>{busyLabel || "正在处理…"}</strong>
          </div>
        )}

        {error && (
          <div className="error-banner compact" role="alert">
            <strong>{error.message}</strong>
            {error.detail && <span>{error.detail}</span>}
          </div>
        )}
        {notice && (
          <div className="success-banner" role="status">
            {notice}
          </div>
        )}
        {!preview && !error && <div className="workspace-loading">正在检查排序结果…</div>}

        {preview && (
          <>
            {hasIntegrityFailure && (
              <section className="integrity-alert" role="alert">
                <div>
                  <strong>结果完整性检查未通过</strong>
                  <span>请先修复以下问题，再确认或导出结果。</span>
                </div>
                <ul>
                  {integrityFailureMessages(preview).map((problem) => (
                    <li key={problem}>{problem}</li>
                  ))}
                </ul>
              </section>
            )}

            <section className="result-table-section">
              <div className="section-title-row">
                <div>
                  <h2>最终排名</h2>
                  <p>{preview.items.length} 个条目 · 升降以导入原始顺序为基准</p>
                </div>
                {preview.status === "confirmed" ? (
                  <button
                    className="secondary-button"
                    disabled={busy}
                    onClick={() => void handleUnlock()}
                  >
                    解锁并继续编辑
                  </button>
                ) : (
                  <button
                    className="primary-button"
                    disabled={busy || !preview.canConfirm || hasIntegrityFailure}
                    onClick={() => void handleConfirm()}
                  >
                    {busy ? "正在确认…" : "确认结果并锁定"}
                  </button>
                )}
              </div>
              <div className="result-table-wrap">
                <table className="result-table">
                  <thead>
                    <tr>
                      <th>最终排名</th>
                      <th>条目</th>
                      <th>原始排名</th>
                      <th>排名变化</th>
                      <th>状态</th>
                    </tr>
                  </thead>
                  <tbody>
                    {preview.items.map((item) => (
                      <tr key={item.itemId}>
                        <td>
                          <strong>#{item.rank}</strong>
                        </td>
                        <td>{item.primaryLabel}</td>
                        <td>#{item.originalRank}</td>
                        <td>
                          <RankChange value={item.rankChange} />
                        </td>
                        <td>
                          <span className="ready-chip">
                            {preview.status === "confirmed" ? "已确认" : "待确认"}
                          </span>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </section>

            <div className="result-configuration-flow">
              <div className="result-actions-grid">
                <section className="result-config-card">
                  <div className="config-card-heading">
                    <div>
                      <p className="eyebrow">RANK FIELD</p>
                      <h2>写入排名字段</h2>
                    </div>
                    {preview.rankWrittenAt && (
                      <span
                        className="written-status"
                        title={formatDateTime(preview.rankWrittenAt)}
                      >
                        ✓ 已写入{preview.rankWrittenField ? `“${preview.rankWrittenField}”` : ""}
                      </span>
                    )}
                  </div>
                  <p>在项目工作副本中创建数字字段；原始只读文件不会被修改。</p>
                  <label className="result-label">
                    字段名称
                    <input
                      className="text-input"
                      value={rankFieldName}
                      onChange={(event) => setRankFieldName(event.target.value)}
                    />
                  </label>
                  <label className="result-label">
                    并列排名规则
                    <select
                      className="text-input"
                      value={rankRule}
                      onChange={(event) => setRankRule(event.target.value as RankRule)}
                    >
                      <option value="competition">竞赛排名（1、2、2、4）</option>
                      <option value="dense">密集排名（1、2、2、3）</option>
                      <option value="ordinal">顺序排名（1、2、3、4）</option>
                    </select>
                  </label>
                  <button
                    className="primary-button"
                    disabled={busy || preview.status !== "confirmed"}
                    onClick={() => void handleWriteRank()}
                  >
                    写入排名
                  </button>
                </section>

                <section className="result-config-card export-card">
                  <p className="eyebrow">EXPORT</p>
                  <h2>导出结果</h2>
                  <div className="export-options">
                    <label>
                      格式
                      <select
                        value={format}
                        onChange={(event) => setFormat(event.target.value as ExportFormat)}
                      >
                        <option value="csv">CSV</option>
                        <option value="json">JSON</option>
                        <option value="markdown_table">Markdown 表格</option>
                        <option value="markdown_list">Markdown 列表</option>
                        <option value="text">纯文本</option>
                      </select>
                    </label>
                    <label>
                      记录顺序
                      <select
                        value={order}
                        onChange={(event) => setOrder(event.target.value as ExportOrder)}
                      >
                        <option value="final">按最终排名</option>
                        <option value="original">按原始顺序</option>
                      </select>
                    </label>
                  </div>
                  <div className="field-mode">
                    <label>
                      <input
                        type="radio"
                        checked={fieldMode === "all"}
                        onChange={() => setFieldMode("all")}
                      />
                      全部字段
                    </label>
                    <label>
                      <input
                        type="radio"
                        checked={fieldMode === "selected"}
                        onChange={() => setFieldMode("selected")}
                      />
                      选择字段
                    </label>
                  </div>
                  {fieldMode === "selected" && (
                    <div className="export-fields">
                      {fieldNames.map((field) => (
                        <label key={field}>
                          <input
                            type="checkbox"
                            checked={selectedFields.includes(field)}
                            onChange={() =>
                              setSelectedFields((current) =>
                                current.includes(field)
                                  ? current.filter((name) => name !== field)
                                  : [...current, field],
                              )
                            }
                          />
                          {field}
                        </label>
                      ))}
                    </div>
                  )}
                  <div className="export-includes">
                    <label>
                      <input
                        type="checkbox"
                        checked={includeRank}
                        onChange={(event) => setIncludeRank(event.target.checked)}
                      />
                      包含最终排名
                    </label>
                    <label>
                      <input
                        type="checkbox"
                        checked={includeOriginalIndex}
                        onChange={(event) => setIncludeOriginalIndex(event.target.checked)}
                      />
                      包含原始索引
                    </label>
                    <label className="disabled-option">
                      <input
                        type="checkbox"
                        checked={includeScore}
                        disabled={!fieldNames.includes(scoreFieldName.trim())}
                        onChange={(event) => setIncludeScore(event.target.checked)}
                      />
                      包含评分字段
                    </label>
                  </div>
                  <button
                    className="primary-button"
                    disabled={busy || preview.status !== "confirmed"}
                    onClick={() => void handleExport()}
                  >
                    {busy ? "处理中…" : `导出 ${exportFormatLabel(format)}`}
                  </button>
                </section>
              </div>

              <section className="result-config-card scoring-card">
                <div className="section-title-row">
                  <div>
                    <p className="eyebrow">SCORE MAPPING</p>
                    <h2>评分映射与写入预览</h2>
                    <p>评分只写入项目工作副本；确认前先展示原字段值和新分数。</p>
                    <small className="autosave-hint">配置停止输入后会自动保存</small>
                  </div>
                  <span className="score-method-chip">
                    {scoreMethod === "linear" ? "线性评分" : "等量分档"}
                  </span>
                </div>
                <div className="score-config-grid">
                  <label>
                    评分字段
                    <input
                      className="text-input"
                      value={scoreFieldName}
                      onChange={(event) => {
                        setScoreFieldName(event.target.value);
                        setScorePreview(null);
                      }}
                    />
                  </label>
                  <label>
                    分配方式
                    <select
                      value={scoreMethod}
                      onChange={(event) => {
                        setScoreMethod(event.target.value as "linear" | "buckets");
                        setScorePreview(null);
                      }}
                    >
                      <option value="linear">线性评分</option>
                      <option value="buckets">等量分档</option>
                    </select>
                  </label>
                  {scoreMethod === "linear" ? (
                    <>
                      <label>
                        最高分
                        <input
                          value={highestScore}
                          onChange={(event) => {
                            setHighestScore(event.target.value);
                            setScorePreview(null);
                          }}
                          inputMode="decimal"
                        />
                      </label>
                      <label>
                        最低分
                        <input
                          value={lowestScore}
                          onChange={(event) => {
                            setLowestScore(event.target.value);
                            setScorePreview(null);
                          }}
                          inputMode="decimal"
                        />
                      </label>
                      <label>
                        小数位数
                        <input
                          value={decimalPlaces}
                          onChange={(event) => {
                            setDecimalPlaces(event.target.value);
                            setScorePreview(null);
                          }}
                          inputMode="numeric"
                        />
                      </label>
                      <label className="score-checkbox">
                        <input
                          type="checkbox"
                          checked={highRankHighScore}
                          onChange={(event) => {
                            setHighRankHighScore(event.target.checked);
                            setScorePreview(null);
                          }}
                        />
                        高排名对应高分
                      </label>
                    </>
                  ) : (
                    <label className="score-levels-field">
                      分数档位（从高排名到低排名）
                      <input
                        value={bucketLevels}
                        onChange={(event) => {
                          setBucketLevels(event.target.value);
                          setScorePreview(null);
                        }}
                        placeholder="5, 4, 3, 2, 1"
                      />
                    </label>
                  )}
                  <label>
                    并列组分数
                    <select
                      value={tieScoreRule}
                      onChange={(event) => {
                        setTieScoreRule(event.target.value as TieScoreRule);
                        setScorePreview(null);
                      }}
                    >
                      <option value="average">占据位置的平均分</option>
                      <option value="highest">取最高分</option>
                      <option value="lowest">取最低分</option>
                    </select>
                  </label>
                </div>
                <div className="score-actions">
                  <button
                    className="secondary-button"
                    disabled={busy || preview.status !== "confirmed"}
                    onClick={() => void handlePreviewScores()}
                  >
                    生成新旧值预览
                  </button>
                  {scorePreview && (
                    <button
                      className="primary-button"
                      disabled={busy || !scorePreview.canWrite}
                      onClick={() => void handleWriteScores()}
                    >
                      确认并写入评分
                    </button>
                  )}
                </div>
                {scorePreview && (
                  <div className="score-preview-wrap">
                    <table className="result-table score-preview-table">
                      <thead>
                        <tr>
                          <th>排名</th>
                          <th>条目</th>
                          <th>并列组</th>
                          <th>原字段值</th>
                          <th>新分数</th>
                        </tr>
                      </thead>
                      <tbody>
                        {scorePreview.items.map((item) => (
                          <tr key={item.itemId}>
                            <td>#{item.rank}</td>
                            <td>{item.primaryLabel}</td>
                            <td>{item.groupName ?? "—"}</td>
                            <td>{valueText(item.oldValue)}</td>
                            <td>
                              <strong>{scoreText(item.score, autosaveScoreConfig)}</strong>
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </table>
                  </div>
                )}
              </section>
            </div>
          </>
        )}
      </section>
    </main>
  );
}

function integrityFailureMessages(preview: ResultPreviewState): string[] {
  if (preview.integrity.problems.length > 0) return preview.integrity.problems;
  return [
    !preview.integrity.allItemsIncluded ? "存在未纳入最终结果的条目。" : "",
    !preview.integrity.noUnresolvedComparisons ? "存在尚未完成的比较判断。" : "",
    !preview.integrity.noDuplicateItems ? "存在重复的内部条目 ID。" : "",
    !preview.integrity.noInvalidItems ? "存在无效条目。" : "",
  ].filter(Boolean);
}

function RankChange({ value }: { value: number }) {
  if (value === 0) return <span className="rank-change same">— 持平</span>;
  return (
    <span className={`rank-change ${value > 0 ? "up" : "down"}`}>
      {value > 0 ? "↑" : "↓"} {Math.abs(value)}
    </span>
  );
}

function valueText(value: unknown): string {
  if (value === null || value === undefined || value === "") return "—";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function scoreText(value: number, config: ScoreConfig | null): string {
  return config?.method.kind === "linear"
    ? value.toFixed(config.method.decimalPlaces)
    : String(value);
}

function formatDateTime(value: string): string {
  return new Intl.DateTimeFormat("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}

interface ScoreConfigValues {
  scoreFieldName: string;
  scoreMethod: "linear" | "buckets";
  highestScore: string;
  lowestScore: string;
  decimalPlaces: string;
  highRankHighScore: boolean;
  bucketLevels: string;
  tieScoreRule: TieScoreRule;
}

function buildScoreConfig(values: ScoreConfigValues): ScoreConfig | null {
  const fieldName = values.scoreFieldName.trim();
  if (!fieldName) return null;
  if (values.scoreMethod === "linear") {
    const highest = Number(values.highestScore);
    const lowest = Number(values.lowestScore);
    const decimals = Number(values.decimalPlaces);
    if (
      ![highest, lowest, decimals].every(Number.isFinite) ||
      highest < lowest ||
      !Number.isInteger(decimals) ||
      decimals < 0 ||
      decimals > 6
    ) {
      return null;
    }
    return {
      fieldName,
      tieRule: values.tieScoreRule,
      method: {
        kind: "linear",
        highestScore: highest,
        lowestScore: lowest,
        decimalPlaces: decimals,
        highRankHighScore: values.highRankHighScore,
      },
    };
  }
  const levels = values.bucketLevels
    .split(/[,，\s]+/)
    .filter(Boolean)
    .map(Number);
  if (levels.length === 0 || levels.some((level) => !Number.isFinite(level))) return null;
  return {
    fieldName,
    tieRule: values.tieScoreRule,
    method: { kind: "buckets", levels },
  };
}

function ensureExtension(path: string, extension: string): string {
  return path.toLocaleLowerCase().endsWith(`.${extension}`) ? path : `${path}.${extension}`;
}

function exportExtension(format: ExportFormat): "csv" | "json" | "md" | "txt" {
  if (format === "markdown_table" || format === "markdown_list") return "md";
  if (format === "text") return "txt";
  return format;
}

function exportFormatLabel(format: ExportFormat): string {
  switch (format) {
    case "csv":
      return "CSV";
    case "json":
      return "JSON";
    case "markdown_table":
      return "Markdown 表格";
    case "markdown_list":
      return "Markdown 列表";
    case "text":
      return "纯文本";
  }
}

function defaultFilename(
  dataset: DatasetOverview,
  task: SortTaskOverview,
  extension: string,
): string {
  const source = (dataset.sourceFilename ?? dataset.name)
    .replace(/^[0-9a-f]{32}_/i, "")
    .replace(/\.[^.]+$/, "");
  const timestamp = new Date().toISOString().replace(/[-:]/g, "").replace("T", "_").slice(0, 15);
  return `${safeName(source)}_${safeName(task.name)}_${timestamp}.${extension}`;
}

function safeName(value: string): string {
  const normalized = value
    .split("")
    .map((character) =>
      character.charCodeAt(0) < 32 || '<>:"/\\|?*'.includes(character) ? "_" : character,
    )
    .join("")
    .replace(/[. ]+$/g, "")
    .slice(0, 80);
  return normalized || "result";
}
