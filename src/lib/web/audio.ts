// Client-side audio playback for the web build.
//
// The desktop app played tick/alert sounds inside the Rust process. In the
// browser there is no audio backend on the server, so this module plays the
// bundled sounds (served from `static/audio/`) — or an uploaded custom sound
// (`/media/audio/<cue>`) — in response to the same WebSocket events.
//
// Settings and the current round type are cached here and refreshed from the
// REST API, so the event shim can stay decoupled from Svelte stores.

type RoundType = 'work' | 'short-break' | 'long-break';

interface AudioSettings {
  volume: number;
  tick_sounds_during_work: boolean;
  tick_sounds_during_break: boolean;
}

const settings: AudioSettings = {
  volume: 1,
  tick_sounds_during_work: false,
  tick_sounds_during_break: false,
};

type Custom = Record<'work_alert' | 'short_break_alert' | 'long_break_alert', string | null>;
const custom: Custom = {
  work_alert: null,
  short_break_alert: null,
  long_break_alert: null,
};

let currentRound: RoundType = 'work';

const DEFAULT_FILE: Record<RoundType, string> = {
  work: '/audio/alert-work.mp3',
  'short-break': '/audio/alert-short-break.mp3',
  'long-break': '/audio/alert-long-break.mp3',
};

const CUE: Record<RoundType, keyof Custom> = {
  work: 'work_alert',
  'short-break': 'short_break_alert',
  'long-break': 'long_break_alert',
};

function clampVolume(v: number): number {
  if (Number.isNaN(v) || v < 0) return 0;
  if (v > 1) return 1;
  return v;
}

function play(url: string): void {
  try {
    const audio = new Audio(url);
    audio.volume = clampVolume(settings.volume);
    void audio.play().catch(() => {
      /* autoplay policy: ignored until the user interacts once */
    });
  } catch {
    /* ignore */
  }
}

function playTick(): void {
  const enabled =
    currentRound === 'work'
      ? settings.tick_sounds_during_work
      : settings.tick_sounds_during_break;
  if (enabled) play('/audio/tick.mp3');
}

function playAlert(round: RoundType): void {
  const cue = CUE[round];
  play(custom[cue] ? `/media/audio/${cue}` : DEFAULT_FILE[round]);
}

async function refreshCustom(): Promise<void> {
  try {
    const res = await fetch('/api/audio');
    if (res.ok) Object.assign(custom, await res.json());
  } catch {
    /* ignore */
  }
}

/** Called for every WebSocket frame by the event shim. */
export function handleFrame(event: string, payload: unknown): void {
  switch (event) {
    case 'settings:changed':
      Object.assign(settings, payload as Partial<AudioSettings>);
      void refreshCustom();
      break;
    case 'timer:round-change':
    case 'timer:reset': {
      const round = (payload as { round_type?: RoundType })?.round_type;
      if (round) {
        if (event === 'timer:round-change') playAlert(round);
        currentRound = round;
      }
      break;
    }
    case 'timer:tick':
      playTick();
      break;
  }
}

async function init(): Promise<void> {
  try {
    const res = await fetch('/api/settings');
    if (res.ok) Object.assign(settings, await res.json());
  } catch {
    /* ignore */
  }
  try {
    const res = await fetch('/api/timer');
    if (res.ok) currentRound = ((await res.json()).round_type as RoundType) ?? 'work';
  } catch {
    /* ignore */
  }
  void refreshCustom();
}

if (typeof window !== 'undefined') void init();
