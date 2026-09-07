const LAST_PROJECT_PARENT_KEY = "subjective-sorter:last-project-parent";

export function projectParentPath(projectPath: string): string {
  const normalized = projectPath.replace(/[\\/]+$/, "");
  const separator = Math.max(normalized.lastIndexOf("\\"), normalized.lastIndexOf("/"));
  if (separator < 0) return projectPath;
  if (separator === 2 && /^[A-Za-z]:/.test(normalized)) return normalized.slice(0, 3);
  return normalized.slice(0, separator);
}

export function loadLastProjectParent(): string | undefined {
  try {
    return window.localStorage.getItem(LAST_PROJECT_PARENT_KEY) ?? undefined;
  } catch {
    return undefined;
  }
}

export function rememberProjectParent(projectPath: string): void {
  try {
    window.localStorage.setItem(LAST_PROJECT_PARENT_KEY, projectParentPath(projectPath));
  } catch {
    // 存储不可用时仍可正常使用系统文件夹选择器。
  }
}
