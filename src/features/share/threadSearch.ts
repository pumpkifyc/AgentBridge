import type { ThreadSummary } from "./shareTypes";

export function filterThreads(threads: ThreadSummary[], query: string): ThreadSummary[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) {
    return threads;
  }
  return threads.filter((thread) => [
    thread.title ?? "",
    thread.preview,
    thread.cwd,
    thread.repositoryId ?? "",
  ].some((field) => field.toLocaleLowerCase().includes(needle)));
}
