// Web replacement for `@tauri-apps/plugin-notification`.
// Uses the browser Notification API.

export async function isPermissionGranted(): Promise<boolean> {
  return typeof Notification !== 'undefined' && Notification.permission === 'granted';
}

export async function requestPermission(): Promise<NotificationPermission> {
  if (typeof Notification === 'undefined') return 'denied';
  return Notification.requestPermission();
}

interface Options {
  title: string;
  body?: string;
}

export function sendNotification(options: Options | string): void {
  if (typeof Notification === 'undefined') return;
  const title = typeof options === 'string' ? options : options.title;
  const body = typeof options === 'string' ? undefined : options.body;
  if (Notification.permission === 'granted') {
    new Notification(title, { body });
  } else if (Notification.permission === 'default') {
    Notification.requestPermission().then((p) => {
      if (p === 'granted') new Notification(title, { body });
    });
  }
}
