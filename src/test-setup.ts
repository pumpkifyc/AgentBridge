import "@testing-library/jest-dom/vitest";

const pending = new Map<string, unknown>();
const queue = new Map<string, unknown[]>();

function keyFor(command: string, args?: Record<string, unknown>): string {
  const normalized = args && "onEvent" in args
    ? { ...args, onEvent: undefined }
    : args;
  return `${command}:${JSON.stringify(normalized ?? {})}`;
}

Object.defineProperty(globalThis, "__TAURI_INTERNALS__", {
  writable: true,
  configurable: true,
  value: {
    transformCallback: () => 1,
    unregisterCallback: () => {},
    invoke: async (command: string, args?: Record<string, unknown>) => {
      const key = keyFor(command, args);
      const fallback = `${command}:*`;
      const queued = queue.get(key) ?? queue.get(fallback);
      if (queued && queued.length > 0) {
        const value = queued.shift();
        if (value instanceof Error) {
          throw value;
        }
        return value;
      }
      const selected = pending.has(key) ? key : fallback;
      if (pending.has(selected)) {
        const value = pending.get(selected);
        if (value instanceof Error) {
          throw value;
        }
        return value;
      }
      throw new Error(`测试未定义 Tauri 命令：${command}`);
    },
  },
});

export function mockTauri(
  command: string,
  args: Record<string, unknown> | "*",
  value: unknown,
  once = false,
) {
  const key = args === "*" ? `${command}:*` : keyFor(command, args);
  if (once) {
    if (!queue.has(key)) {
      queue.set(key, []);
    }
    queue.get(key)!.push(value);
    return;
  }
  pending.set(key, value);
}

export function resetTauriMocks() {
  pending.clear();
  queue.clear();
}
