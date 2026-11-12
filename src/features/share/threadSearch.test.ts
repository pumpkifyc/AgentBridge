import { describe, expect, it } from "vitest";
import { filterThreads } from "./threadSearch";
import type { ThreadSummary } from "./shareTypes";

const threads: ThreadSummary[] = [
  {
    id: "one",
    title: "设计会话互通方案",
    preview: "方案讨论",
    cwd: "/Users/alice/AgentBridge",
    createdAt: 1,
    updatedAt: 2,
    repositoryId: null,
  },
  {
    id: "two",
    title: null,
    preview: "修复 Windows 安装包",
    cwd: "C:\\projects\\Bridge",
    createdAt: 3,
    updatedAt: 4,
    repositoryId: null,
  },
];

describe("会话搜索", () => {
  it("按标题、预览和工作目录进行不区分大小写的匹配", () => {
    expect(filterThreads(threads, "互通").map((thread) => thread.id)).toEqual(["one"]);
    expect(filterThreads(threads, "WINDOWS").map((thread) => thread.id)).toEqual(["two"]);
    expect(filterThreads(threads, "bridge").map((thread) => thread.id)).toEqual(["one", "two"]);
  });

  it("空白查询保留原有顺序", () => {
    expect(filterThreads(threads, "   ").map((thread) => thread.id)).toEqual(["one", "two"]);
  });
});
