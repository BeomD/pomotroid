// Web replacement for `@tauri-apps/plugin-log`. Logs to the browser console.

type Msg = string | Error | unknown;

function fmt(message: Msg): string {
  return message instanceof Error ? message.message : String(message);
}

export async function trace(message: Msg): Promise<void> {
  console.debug(fmt(message));
}
export async function debug(message: Msg): Promise<void> {
  console.debug(fmt(message));
}
export async function info(message: Msg): Promise<void> {
  console.info(fmt(message));
}
export async function warn(message: Msg): Promise<void> {
  console.warn(fmt(message));
}
export async function error(message: Msg): Promise<void> {
  console.error(fmt(message));
}
