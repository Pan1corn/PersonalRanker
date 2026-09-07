import { describe, expect, it } from "vitest";
import { projectParentPath } from "./project-path";

describe("projectParentPath", () => {
  it("返回项目文件夹所在目录", () => {
    expect(projectParentPath("D:\\工作\\测试.subject-sort")).toBe("D:\\工作");
    expect(projectParentPath("D:\\测试.subject-sort")).toBe("D:\\");
    expect(projectParentPath("C:/工作/测试.subject-sort/")).toBe("C:/工作");
  });
});
