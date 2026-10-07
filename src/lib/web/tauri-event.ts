// Web replacement for `@tauri-apps/api/event`.
//
// Provides the `listen()` API the frontend uses, backed by a single WebSocket
// to `/ws`. The server sends `{ "event", "payload" }` frames identical to the
// shape of Tauri events, so callbacks receive `{ payload }` unchanged.

import { handleFrame } from './audio';

export type UnlistenFn = () => void;

interface TauriEvent<T> {
  event: string;
  id: number;
  payload: T;
}

type AnyHandler = (event: TauriEvent<unknown>) => void;

const handlers = new Map<string, Set<AnyHandler>>();

let socket: WebSocket | null = null;
let reconnectTimer: ReturnType<typeof setTimeout> | null = null;

function connect(): void {
  if (typeof window === 'undefined') return;
  const proto = location.protocol === 'https:' ? 'wss' : 'ws';
  socket = new WebSocket(`${proto}://${location.host}/ws`);

  socket.onmessage = (msg) => {
    try {
      const frame = JSON.parse(msg.data) as { event: string; payload: unknown };
      emitLocal(frame.event, frame.payload);
    } catch {
      /* ignore malformed frames */
    }
  };

  socket.onclose = () => {
    if (reconnectTimer) return;
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      connect();
    }, 1000);
  };
}

function emitLocal(event: string, payload: unknown): void {
  // Client-side audio playback (ticks / round alerts).
  handleFrame(event, payload);

  const set = handlers.get(event);
  if (!set) return;
  for (const handler of set) handler({ event, id: -1, payload });
}

if (typeof window !== 'undefined') connect();

export async function listen<T>(
  event: string,
  callback: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  const handler = callback as AnyHandler;
  let set = handlers.get(event);
  if (!set) {
    set = new Set();
    handlers.set(event, set);
  }
  set.add(handler);
  return () => {
    set?.delete(handler);
  };
}

export async function once<T>(
  event: string,
  callback: (event: TauriEvent<T>) => void
): Promise<UnlistenFn> {
  const unlisten = await listen<T>(event, (e) => {
    unlisten();
    callback(e);
  });
  return unlisten;
}

/** Client → server events are not used by this app; provided for completeness. */
export async function emit(_event: string, _payload?: unknown): Promise<void> {}
