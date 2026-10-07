// Web replacement for `@tauri-apps/plugin-opener`. Opens links in a new tab.

export async function openUrl(url: string, _opts?: unknown): Promise<void> {
  if (typeof window !== 'undefined') window.open(url, '_blank', 'noopener');
}

export async function openPath(path: string, _opts?: unknown): Promise<void> {
  if (typeof window !== 'undefined') window.open(path, '_blank', 'noopener');
}
