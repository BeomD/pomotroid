// Web replacement for `@tauri-apps/api/core`.
//
// The desktop app talks to Rust through Tauri's `invoke()` IPC. In the browser
// there is no IPC, so this shim maps each Tauri command name onto the matching
// HTTP endpoint exposed by `server/`. Command names and argument shapes are
// kept identical so `$lib/ipc/index.ts` (and every component) needs no changes.

type Args = Record<string, unknown>;

async function get<T>(path: string): Promise<T> {
  const res = await fetch(path, { headers: { Accept: 'application/json' } });
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return res.status === 204 ? (undefined as T) : ((await res.json()) as T);
}

async function post(path: string, body?: unknown): Promise<void> {
  const res = await fetch(path, {
    method: 'POST',
    headers: body === undefined ? undefined : { 'Content-Type': 'application/json' },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
}

async function postJson<T>(path: string, body: unknown): Promise<T> {
  const res = await fetch(path, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  });
  if (!res.ok) throw new Error(`${path} → ${res.status}`);
  return (await res.json()) as T;
}

async function upload(cue: string, file: File): Promise<string> {
  const form = new FormData();
  form.append('file', file, file.name);
  const res = await fetch(`/api/audio/${encodeURIComponent(cue)}`, {
    method: 'POST',
    body: form,
  });
  if (!res.ok) throw new Error(`audio upload → ${res.status}`);
  return (await res.json()) as string;
}

function notify(title: string, body: string) {
  if (typeof Notification === 'undefined') return;
  if (Notification.permission === 'granted') {
    new Notification(title, { body });
  } else if (Notification.permission === 'default') {
    Notification.requestPermission().then((p) => {
      if (p === 'granted') new Notification(title, { body });
    });
  }
}

export async function invoke<T = unknown>(cmd: string, args?: Args): Promise<T> {
  switch (cmd) {
    // Timer
    case 'timer_get_state':
      return get<T>('/api/timer');
    case 'timer_toggle':
      return post('/api/timer/toggle') as Promise<T>;
    case 'timer_reset':
      return post('/api/timer/reset') as Promise<T>;
    case 'timer_restart_round':
      return post('/api/timer/restart') as Promise<T>;
    case 'timer_skip':
      return post('/api/timer/skip') as Promise<T>;

    // Settings
    case 'settings_get':
      return get<T>('/api/settings');
    case 'settings_set':
      return postJson<T>('/api/settings', {
        key: args?.key,
        value: args?.value,
      });
    case 'settings_reset_defaults':
      return postJson<T>('/api/settings/reset', {});

    // Themes
    case 'themes_list':
      return get<T>('/api/themes');

    // Sessions + stats
    case 'sessions_clear':
      return post('/api/sessions/clear') as Promise<T>;
    case 'stats_get_detailed':
      return get<T>('/api/stats/detailed');
    case 'stats_get_heatmap':
      return get<T>('/api/stats/heatmap');

    // Audio
    case 'audio_get_custom_info':
      return get<T>('/api/audio');
    case 'audio_set_custom': {
      const cue = String(args?.cue);
      const file = args?.srcPath as File;
      return upload(cue, file) as Promise<T>;
    }
    case 'audio_clear_custom': {
      const cue = encodeURIComponent(String(args?.cue));
      const res = await fetch(`/api/audio/${cue}`, { method: 'DELETE' });
      if (!res.ok) throw new Error(`audio clear → ${res.status}`);
      return undefined as T;
    }

    // Notifications (browser-native)
    case 'notification_show':
      notify(String(args?.title ?? ''), String(args?.body ?? ''));
      return undefined as T;

    // Diagnostics / platform — stubs for the web
    case 'app_version':
      return get<T>('/api/version');
    case 'tray_supported':
      return false as T;
    case 'accessibility_trusted':
      return true as T;
    case 'check_update':
      return null as T;
    case 'get_log_dir':
      return '' as T;
    case 'open_log_dir':
    case 'install_update':
    case 'window_set_visibility':
    case 'shortcuts_reload':
      return undefined as T;

    default:
      console.warn(`[web] unhandled invoke command: ${cmd}`);
      return undefined as T;
  }
}

/** Present so code that probes for the Tauri runtime behaves correctly. */
export const isTauri = false;
