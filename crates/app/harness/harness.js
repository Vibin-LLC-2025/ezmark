// @ts-check
// Security-harness instrumentation (plan section 11). Injected as an
// initialization script in test-harness builds only, ahead of the page's own
// scripts; it is not part of ui/ and never ships in a release build.
//
// It records enforced CSP violations, waits for the viewer to render, runs the
// probes the Rust side configured, and reports everything back over IPC.
(() => {
  const supplied = window.__EZMARK_HARNESS_CONFIG__;
  if (!supplied) return;
  const config = supplied;

  /** @type {HarnessCspReport[]} */
  const cspReports = [];
  /** @type {string[]} */
  const errors = [];

  document.addEventListener(
    'securitypolicyviolation',
    (event) => {
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

  /** @param {Element} root */
  async function settleImages(root) {
    const pending = Array.from(root.querySelectorAll('img'))
      .filter((img) => !img.complete)
      .map(
        (img) =>
          new Promise((resolve) => {
            img.addEventListener('load', resolve, { once: true });
            img.addEventListener('error', resolve, { once: true });
            setTimeout(resolve, 5000);
          }),
      );
    await Promise.all(pending);
    // Violation events are dispatched as tasks; give them time to land.
    await sleep(400);
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
      setTimeout(() => resolve({ url, loaded: false, timedOut: true }), 5000);
      img.src = url;
    });
  }

  /** @param {string} text */
  function toHex(text) {
    return Array.from(new TextEncoder().encode(text), (b) => b.toString(16).padStart(2, '0')).join('');
  }

  /** @type {unknown[]} */
  const renders = [];
  let documentRenders = 0;
  let finished = false;

  window.addEventListener('ezmark:rendered', (event) => {
    const detail = /** @type {CustomEvent<{generation: number, kind: string}>} */ (event).detail;
    void onRendered(detail).catch((error) => errors.push(String(error)));
  });

  /** @param {{generation: number, kind: string}} detail */
  async function onRendered(detail) {
    const content = document.getElementById('content');
    if (!content) throw new Error('no #content element');
    await settleImages(content);
    const facts = { ...detail, title: document.title, dom: domFacts(content) };
    renders.push(facts);
    await invoke('harness_report', { kind: 'rendered', data: facts });

    if (detail.kind !== 'empty') documentRenders += 1;
    const done = config.renders === 0 || documentRenders >= config.renders;
    if (!done || finished) return;
    finished = true;

    /** @type {Record<string, unknown>} */
    const report = { renders, cspReports, errors, injected: null, assetProbes: [], aclProbe: null };

    if (config.inject) {
      // Enforcement corpus only: raw, unsanitized HTML straight into the DOM,
      // so that the CSP alone has to stop it.
      const raw = /** @type {string} */ (await invoke('harness_inject'));
      content.innerHTML = raw;
      await settleImages(content);
      report.injected = domFacts(content);
    }

    const sample = content.querySelector('img[src]')?.getAttribute('src');
    const match = sample ? /^(.*)\/(\d+)\/([0-9a-f]+)$/.exec(sample) : null;
    if (match) {
      const [, base, generation, hex] = match;
      const probes = /** @type {unknown[]} */ (report.assetProbes);
      probes.push({ kind: 'current', ...(await loadProbe(`${base}/${generation}/${hex}`)) });
      probes.push({ kind: 'stale', ...(await loadProbe(`${base}/${Number(generation) - 1}/${hex}`)) });
      if (config.probeAsset) {
        const url = `${base}/${generation}/${toHex(config.probeAsset)}`;
        probes.push({ kind: 'crafted', reference: config.probeAsset, ...(await loadProbe(url)) });
      }
    }

    try {
      await invoke('harness_denied_probe');
      report.aclProbe = { rejected: false };
    } catch (error) {
      report.aclProbe = { rejected: true, error: String(error) };
    }

    await sleep(200);
    await invoke('harness_report', { kind: 'final', data: report });
  }
})();
