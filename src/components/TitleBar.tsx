import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Custom window titlebar for Linux. Native window decorations on KWin/Wayland
 * don't deliver their minimize/close button clicks to the WebKitGTK surface, so
 * the buttons are dead. We render our own titlebar (clicks land in the webview,
 * which always works) and drive it through Tauri's window API. On macOS/Windows
 * the native decorations work fine, so this titlebar is not rendered there.
 */
export function TitleBar() {
  const appWindow = getCurrentWindow();

  return (
    <div className="titlebar" data-tauri-drag-region>
      <span className="titlebar-title" data-tauri-drag-region>
        Careless Whisper
      </span>
      <div className="titlebar-controls">
        <button
          type="button"
          className="titlebar-btn"
          aria-label="Minimize"
          onClick={() => void appWindow.minimize()}
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
            <rect x="2" y="5.6" width="8" height="1.2" fill="currentColor" />
          </svg>
        </button>
        <button
          type="button"
          className="titlebar-btn titlebar-btn-close"
          aria-label="Close"
          onClick={() => void appWindow.hide()}
        >
          <svg width="12" height="12" viewBox="0 0 12 12" aria-hidden="true">
            <path
              d="M2.5 2.5 L9.5 9.5 M9.5 2.5 L2.5 9.5"
              stroke="currentColor"
              strokeWidth="1.3"
              strokeLinecap="round"
            />
          </svg>
        </button>
      </div>
    </div>
  );
}
