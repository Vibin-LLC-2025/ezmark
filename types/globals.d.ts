// Types for the contributor type-check (`npm run typecheck`). Not shipped.

/** What Rust tells the view to show. Mirrors `documents::View`. */
type View = { generation: number } & (
  | { kind: 'empty' }
  | { kind: 'document'; title: string; path: string; html: string }
  | { kind: 'error'; title: string; message: string }
);

/** The subset of `window.__TAURI__` (enabled by `app.withGlobalTauri`) we use. */
interface TauriGlobal {
  core: {
    invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  };
  event: {
    listen<T>(event: string, handler: (event: { payload: T }) => void): Promise<() => void>;
  };
}

/** Injected by the test-harness build only. */
interface HarnessConfig {
  renders: number;
  inject: boolean;
  probeAsset: string | null;
  hold: boolean;
}

interface HarnessCspReport {
  blockedURI: string;
  effectiveDirective: string;
  violatedDirective: string;
  disposition: string;
  sourceFile: string;
}

interface Window {
  __TAURI__: TauriGlobal;
  __TAURI_INTERNALS__: {
    invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  };
  __EZMARK_HARNESS_CONFIG__?: HarnessConfig;
}
