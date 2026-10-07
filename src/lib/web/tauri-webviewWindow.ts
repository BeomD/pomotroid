// Web replacement for `@tauri-apps/api/webviewWindow`.
//
// The desktop app opens settings/stats in separate native windows. In the
// browser, "open window" becomes a new browser tab and window controls are
// no-ops (or map to the Fullscreen API).

interface WebviewWindowOptions {
  url?: string;
  title?: string;
  width?: number;
  height?: number;
  [key: string]: unknown;
}

class WebviewWindowHandle {
  label: string;

  constructor(label: string, options?: WebviewWindowOptions) {
    this.label = label;
    if (options?.url && typeof window !== 'undefined') {
      window.open(options.url, '_blank', 'noopener');
    }
  }

  static async getByLabel(_label: string): Promise<null> {
    return null;
  }

  static getCurrent(): WebviewWindowHandle {
    return getCurrentWebviewWindow();
  }
}

export function getCurrentWebviewWindow() {
  return {
    label: 'main',
    show: async () => {},
    hide: async () => {},
    setFocus: async () => {},
    minimize: async () => {},
    toggleMaximize: async () => {
      if (document.fullscreenElement) await document.exitFullscreen();
      else await document.documentElement.requestFullscreen?.();
    },
    isMaximized: async () => !!document.fullscreenElement,
    close: async () => window.close(),
    setFullscreen: async (value: boolean) => {
      if (value) await document.documentElement.requestFullscreen?.();
      else await document.exitFullscreen?.();
    },
    onResized: async (_cb?: () => void) => () => {},
    startResizeDragging: async (_direction?: string) => {},
  };
}

export { WebviewWindowHandle as WebviewWindow };
