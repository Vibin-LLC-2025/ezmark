// @ts-check
// Security-harness instrumentation (plan section 11). Injected as an
// initialization script in test-harness builds only, ahead of the page's own
// scripts; it is not part of ui/ and never ships in a release build.
//
// It records enforced CSP violations, waits for the viewer to render, runs the
// probes the Rust side configured, and reports everything back over IPC.
//
// Nothing here waits a fixed time and then assumes an event would have arrived
// by now. Expected events are waited for by name, and "nothing else happened"
// is checked by `flushViolations`: it provokes one more violation and waits
// for that one, so every report queued before it has been delivered.
(() => {
  const supplied = window.__EZMARK_HARNESS_CONFIG__;
  if (!supplied) return;
  const config = supplied;

  const SENTINEL = 'http://ezmark-sentinel.invalid/';
  const PATIENCE_MS = 8000;

  /** @type {HarnessCspReport[]} */
  const cspReports = [];
  /** @type {string[]} */
  const errors = [];
  /** @type {Map<string, () => void>} */
  const sentinels = new Map();
  let sentinelsSent = 0;

  document.addEventListener(
    'securitypolicyviolation',
    (event) => {
      const arrived = sentinels.get(event.blockedURI);
      if (arrived) {
        arrived();
        return;
      }
      cspReports.push({
        blockedURI: event.blockedURI,
        effectiveDirective: event.effectiveDirective,
        violatedDirective: event.violatedDirective,
        disposition: event.disposition,
        sourceFile: event.sourceFile,
      });
    },
    true,
  );
  window.addEventListener('error', (event) => errors.push(String(event.message)));
  window.addEventListener('unhandledrejection', (event) => errors.push(String(event.reason)));

  /**
   * @param {string} cmd
   * @param {Record<string, unknown>} [args]
   */
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);

  /** @param {number} ms */
  const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

  /**
   * Resolve once `done()` is true, or give up after `PATIENCE_MS` and say so.
   * @param {() => boolean} done
   * @param {string} what
   */
  async function until(done, what) {
    const deadline = Date.now() + PATIENCE_MS;
    while (!done()) {
      if (Date.now() > deadline) {
        errors.push(`harness: gave up waiting for ${what}`);
        return;
      }
      await sleep(20);
    }
  }

  /**
   * Violation events are delivered in the order they were queued. Queue one
   * more, for a URL only the harness uses, and wait for it: when it arrives,
   * every earlier one has too.
   */
  async function flushViolations() {
    sentinelsSent += 1;
    const url = `${SENTINEL}${sentinelsSent}`;
    let arrived = false;
    sentinels.set(url, () => {
      arrived = true;
    });
    new Image().src = url; // blocked by img-src before any lookup or connection
    await until(() => arrived, 'the sentinel CSP report');
    sentinels.delete(url);
  }

  /** @param {Element} root */
  async function settle(root) {
    const pending = Array.from(root.querySelectorAll('img'))
      .filter((img) => !img.complete)
      .map(
        (img) =>
          new Promise((resolve) => {
            img.addEventListener('load', resolve, { once: true });
            img.addEventListener('error', resolve, { once: true });
            setTimeout(resolve, PATIENCE_MS);
          }),
      );
    await Promise.all(pending);
    await flushViolations();
  }

  /** @param {Element} root */
  function domFacts(root) {
    const elements = Array.from(root.querySelectorAll('*'));
    /** @param {(attr: Attr) => boolean} test */
    const withAttribute = (test) =>
      elements.filter((el) => Array.from(el.attributes).some(test)).length;
    return {
      html: root.innerHTML,
      scripts: root.querySelectorAll('script').length,
      inlineHandlers: withAttribute((attr) => attr.name.toLowerCase().startsWith('on')),
      scriptUrls: withAttribute((attr) => /^\s*(javascript|vbscript):/i.test(attr.value)),
      frames: root.querySelectorAll('iframe, frame, object, embed').length,
      // Every attribute value that names an absolute or network-path URL,
      // except a link's own target (links may point anywhere, because nothing
      // follows them without the user) and the two attributes that are only
      // ever shown as text.
      urlAttributes: elements.flatMap((el) =>
        Array.from(el.attributes)
          .filter((attr) => /^\s*([a-z][a-z0-9+.-]*:|[/\\]{2})/i.test(attr.value))
          .filter((attr) => !(el.localName === 'a' && attr.name === 'href'))
          .filter((attr) => attr.name !== 'title' && attr.name !== 'alt')
          .map((attr) => ({ element: el.localName, name: attr.name, value: attr.value })),
      ),
      placeholders: Array.from(root.querySelectorAll('.image-placeholder')).map((el) => ({
        reason: el.getAttribute('data-reason'),
        text: el.textContent,
      })),
      notices: Array.from(root.querySelectorAll('.document-notice')).map((el) => el.textContent),
      images: Array.from(root.querySelectorAll('img')).map((img) => ({
        src: img.getAttribute('src'),
        alt: img.alt,
        complete: img.complete,
        naturalWidth: img.naturalWidth,
        naturalHeight: img.naturalHeight,
      })),
    };
  }

  /** @param {string} url */
  function loadProbe(url) {
    return new Promise((resolve) => {
      const img = new Image();
      img.onload = () => resolve({ url, loaded: true, naturalWidth: img.naturalWidth });
      img.onerror = () => resolve({ url, loaded: false });
      setTimeout(() => resolve({ url, loaded: false, timedOut: true }), PATIENCE_MS);
      img.src = url;
    });
  }

  /** @param {string} text */
  function toHex(text) {
    return Array.from(new TextEncoder().encode(text), (b) => b.toString(16).padStart(2, '0')).join('');
  }

  /**
   * Try to reach `origin` from script, every way a page can. Each attempt is
   * recorded with whether the page saw it fail; the listener in the test
   * process is the real judge.
   * @param {string} origin
   */
  async function connectProbes(origin) {
    /** @type {Record<string, string>} */
    const outcome = {};
    /**
     * @param {string} name
     * @param {() => unknown} attempt
     */
    const attempt = async (name, attempt) => {
      try {
        const result = await attempt();
        outcome[name] = `no error (${String(result)})`;
      } catch (error) {
        outcome[name] = `refused: ${String(error)}`;
      }
    };
    await attempt('fetch', () => fetch(`${origin}/fetch`, { mode: 'no-cors' }).then((r) => r.type));
    await attempt('xhr', () => {
      const xhr = new XMLHttpRequest();
      xhr.open('GET', `${origin}/xhr`, false);
      xhr.send();
      return xhr.status;
    });
    await attempt('beacon', () => navigator.sendBeacon(`${origin}/beacon`, 'x'));
    await attempt('websocket', () => {
      const socket = new WebSocket(`${origin.replace(/^http/, 'ws')}/websocket`);
      return new Promise((resolve, reject) => {
        socket.onopen = () => resolve('opened');
        socket.onerror = () => reject(new Error('socket error'));
      });
    });
    await attempt('eventsource', () => {
      const source = new EventSource(`${origin}/eventsource`);
      return new Promise((resolve, reject) => {
        source.onopen = () => resolve('opened');
        source.onerror = () => {
          source.close();
          reject(new Error('eventsource error'));
        };
      });
    });
    await attempt('script-element', () => {
      const script = document.createElement('script');
      script.src = `${origin}/script-element.js`;
      return new Promise((resolve, reject) => {
        script.onload = () => resolve('loaded');
        script.onerror = () => reject(new Error('script error'));
        document.head.append(script);
      });
    });
    await attempt('inline-script', () => {
      const script = document.createElement('script');
      script.textContent = "window.__EZMARK_PAYLOAD_RAN__ = 'inline-script'";
      document.head.append(script);
      return window.__EZMARK_PAYLOAD_RAN__ ?? 'did not run';
    });
    await attempt('eval', () => eval("window.__EZMARK_PAYLOAD_RAN__ = 'eval'"));
    return outcome;
  }

  /** @param {string} url */
  const reported = (url) => cspReports.some((report) => report.blockedURI.startsWith(url));

  /**
   * Try to leave the page for `origin`, every way a document could make the
   * page try. The navigation guard in Rust records what it denied.
   * @param {Element} root
   * @param {string} origin
   */
  async function navigationProbes(root, origin) {
    /**
     * Dispatch a cancelable mouse event on the fixture's element `id`; true
     * if the app's own handler cancelled it.
     * @param {string} id
     * @param {string} type
     */
    const cancelled = (id, type) => {
      const target = root.querySelector(`#${id}`);
      if (!target) return `no #${id}`;
      return !target.dispatchEvent(new MouseEvent(type, { bubbles: true, cancelable: true }));
    };
    const outcome = {
      linkClick: cancelled('probe-link', 'click'),
      linkMiddleClick: cancelled('probe-link', 'auxclick'),
      linkContextMenu: cancelled('probe-link', 'contextmenu'),
      newWindowClick: cancelled('probe-new-window', 'click'),
      imageLinkClick: cancelled('probe-image-in-link', 'click'),
      fragmentClick: cancelled('probe-fragment', 'click'),
      textContextMenu: cancelled('probe-text', 'contextmenu'),
      /** @type {string} */
      windowOpen: 'not tried',
      /** @type {string} */
      formSubmit: 'not tried',
    };
    try {
      outcome.windowOpen = window.open(`${origin}/window-open`) ? 'opened' : 'refused';
    } catch (error) {
      outcome.windowOpen = `refused: ${String(error)}`;
    }
    const submit = root.querySelector('#probe-submit');
    if (submit instanceof HTMLElement) {
      submit.click(); // form-action 'none' must stop this
      outcome.formSubmit = 'clicked';
      // A submission is carried out in a later task, and a newer navigation
      // would replace it. Let it be refused before trying the next thing.
      await until(() => reported(`${origin}/form`), 'the form-action report');
    }
    // Last, a script-driven navigation: only the Rust guard can stop this one.
    window.location.href = `${origin}/location`;
    return outcome;
  }

  /** @type {unknown[]} */
  const renders = [];
  let documentRenders = 0;
  let finished = false;
  const startedAt = window.location.href;

  window.addEventListener('ezmark:rendered', (event) => {
    const detail = /** @type {CustomEvent<{generation: number, kind: string}>} */ (event).detail;
    void onRendered(detail).catch((error) => errors.push(String(error)));
  });

  /** @param {{generation: number, kind: string}} detail */
  async function onRendered(detail) {
    const content = document.getElementById('content');
    if (!content) throw new Error('no #content element');
    await settle(content);
    const facts = { ...detail, title: document.title, dom: domFacts(content) };
    renders.push(facts);
    await invoke('harness_report', { kind: 'rendered', data: facts });

    if (detail.kind !== 'empty') documentRenders += 1;
    const done = config.renders === 0 || documentRenders >= config.renders;
    if (!done || finished) return;
    finished = true;

    /** @type {Record<string, unknown>} */
    const report = {
      renders,
      cspReports,
      errors,
      injected: null,
      assetProbes: [],
      connectProbes: null,
      navigationProbes: null,
      aclProbe: null,
    };

    const sample = content.querySelector('img[src]')?.getAttribute('src');
    const match = sample ? /^(.*)\/(\d+)\/([0-9a-f]+)$/.exec(sample) : null;
    if (match) {
      const [, base, scope, hex] = match;
      const probes = /** @type {unknown[]} */ (report.assetProbes);
      probes.push({ kind: 'current', ...(await loadProbe(`${base}/${scope}/${hex}`)) });
      probes.push({ kind: 'stale', ...(await loadProbe(`${base}/${Number(scope) - 1}/${hex}`)) });
      if (config.probeAsset) {
        const url = `${base}/${scope}/${toHex(config.probeAsset)}`;
        probes.push({ kind: 'crafted', reference: config.probeAsset, ...(await loadProbe(url)) });
      }
    }

    if (config.inject) {
      // Enforcement corpus only: raw, unsanitized HTML straight into the DOM,
      // so that the CSP alone has to stop it.
      const raw = /** @type {string} */ (await invoke('harness_inject'));
      content.innerHTML = raw;
      await settle(content);
      report.injected = domFacts(content);
    }

    if (config.connectProbe) report.connectProbes = await connectProbes(config.connectProbe);

    try {
      await invoke('harness_denied_probe');
      report.aclProbe = { rejected: false };
    } catch (error) {
      report.aclProbe = { rejected: true, error: String(error) };
    }

    if (config.navigationProbe) {
      report.navigationProbes = await navigationProbes(content, config.navigationProbe);
    }

    await until(
      () => config.expectBlocked.every(reported),
      `CSP reports for ${config.expectBlocked.filter((url) => !reported(url)).join(', ')}`,
    );
    await flushViolations();

    report.payloadRan = window.__EZMARK_PAYLOAD_RAN__ ?? null;
    report.location = { startedAt, now: window.location.href };
    await invoke('harness_report', { kind: 'final', data: report });
  }
})();
