// @vitest-environment jsdom

import "@testing-library/jest-dom/vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { EditableSortCriteria } from "./EditableSortCriteria";

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("EditableSortCriteria", () => {
  it("不显示修改按钮，并支持双击或键盘修改排序标准", () => {
    const onSave = vi.fn();
    vi.spyOn(window, "prompt").mockReturnValue("新的排序标准");
    render(<EditableSortCriteria value="旧标准" onSave={onSave} />);

    expect(screen.queryByText("修改")).not.toBeInTheDocument();
    const criteria = screen.getByRole("button", { name: /排序标准：旧标准/ });
    fireEvent.doubleClick(criteria);
    expect(onSave).toHaveBeenLastCalledWith("新的排序标准");
    fireEvent.keyDown(criteria, { key: "Enter" });
    expect(onSave).toHaveBeenCalledTimes(2);
  });
});
