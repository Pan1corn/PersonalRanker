interface Props {
  value: string;
  busy?: boolean;
  compact?: boolean;
  onSave: (value: string) => void;
}

/** 在所有排序工作区提供一致的排序标准编辑入口。 */
export function EditableSortCriteria({ value, busy = false, compact = false, onSave }: Props) {
  function edit() {
    if (busy) return;
    const next = window.prompt("修改排序标准", value);
    if (!next?.trim() || next.trim() === value) return;
    onSave(next.trim());
  }

  return (
    <div
      className={`sort-criteria-hero editable-sort-criteria${compact ? " drag-criteria-card" : ""}`}
      role="button"
      tabIndex={0}
      aria-disabled={busy}
      aria-label={`排序标准：${value}。双击或按 Enter 修改`}
      title="双击修改排序标准；按 Enter 或 F2 也可修改"
      onDoubleClick={edit}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== "F2") return;
        event.preventDefault();
        edit();
      }}
    >
      <small>排序标准</small>
      <strong>{value}</strong>
    </div>
  );
}
