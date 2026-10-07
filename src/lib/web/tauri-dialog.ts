// Web replacement for `@tauri-apps/plugin-dialog`.
//
// `open()` uses a hidden `<input type="file">` and resolves with the selected
// `File` object (the browser equivalent of the desktop file path). The result
// is uploaded to the server by the `audio_set_custom` invoke shim.

interface OpenOptions {
  multiple?: boolean;
  filters?: { name: string; extensions: string[] }[];
}

export async function open(options?: OpenOptions): Promise<File | File[] | null> {
  return new Promise((resolve) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.multiple = !!options?.multiple;

    const exts = options?.filters?.flatMap((f) => f.extensions ?? []) ?? [];
    if (exts.length) {
      input.accept = exts.map((e) => `.${e.replace(/^\./, '')}`).join(',');
    } else {
      input.accept = 'audio/*';
    }

    input.onchange = () => {
      const files = input.files ? Array.from(input.files) : [];
      if (!files.length) resolve(null);
      else resolve(options?.multiple ? files : files[0]);
    };

    input.click();
  });
}
