// Types for the contributor type-check (`npm run typecheck`). Not shipped.

/** What Rust tells the view to show. Mirrors `documents::View`. */
type View = { generation: number } & (
  | { kind: 'empty' }
  | { kind: 'document'; title: string; path: string; html: string; notices: string[] }
  | { kind: 'error'; title: string; message: string }
);

/** A render that has started and not finished. Mirrors `documents::Loading`. */
type Loading = { generation: number; title: string };

/** Whether the open file is still there. Mirrors `documents::FileStatus`. */
type FileStatus = { missing: boolean; title: string };

/** The reply to `frontend_ready`. Mirrors `documents::Ready`. */
type Ready = { view: View; loading: Loading | null };

/** The subset of `window.__TAURI__` (enabled by `app.withGlobalTauri`) we use. */
interface TauriGlobal {
  core: {
    invoke(cmd: string, args?: Record<string, unknown>): Promise<unknown>;
  };
  event: {
    listen(event: string, handler: (event: { payload: unknown }) => void): Promise<() => void>;
  };
}

/** Injected by the test-harness build only. Mirrors `harness::ScriptConfig`. */
interface HarnessConfig {
  renders: number;
  inject: boolean;
  probeAsset: string | null;
  expectBlocked: string[];
  connectProbe: string | null;
  navigationProbe: string | null;
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
  /** Set by an enforcement payload if its script ever runs. */
  __EZMARK_PAYLOAD_RAN__?: string;
}
