// Contributor-only tests of ui/ (`npm test`). They run the real files against
// fake Tauri and DOM globals, so they need no browser and no npm dependency.
// Not shipped: the bundle is ui/, and this file is outside it.

import assert from 'node:assert/strict';
import test from 'node:test';

import { blockedImage } from '../../ui/placeholders.js';

let loads = 0;

const settle = () => new Promise((resolve) => setImmediate(resolve));

/** A stand-in for a DOM element: enough for what app.js does with one. */
class FakeElement {
  /** @param {string} tag */
  constructor(tag) {
    this.localName = tag;
    this.className = '';
    this.textContent = '';
    this.title = '';
    this.hidden = false;
    /** @type {Record<string, string>} */
    this.attributes = {};
    /** @type {unknown[]} */
    this.children = [];
    /** @type {FakeElement | null} */
    this.parentElement = null;
    /** @type {FakeElement | null} */
    this.replacedWith = null;
    // A <template>: its content holds whatever images the test put in the view.
    this.content = {
      html: '',
      images: /** @type {FakeElement[]} */ ([]),
      querySelectorAll: () => this.content.images,
    };
  }
  set innerHTML(html) {
    this.content.html = html;
    this.content.images = FakeElement.imagesIn(html);
  }
  /** Images are written in test HTML as `[img src|alt]`, to avoid an HTML parser. */
  static imagesIn(/** @type {string} */ html) {
    return Array.from(html.matchAll(/\[img ([^|\]]*)\|([^\]]*)\]/g), ([, src, alt]) => {
      const img = new FakeElement('img');
      if (src) img.attributes.src = src;
      if (alt) img.attributes.alt = alt;
      return img;
    });
  }
  getAttribute(/** @type {string} */ name) {
    return this.attributes[name] ?? null;
  }
  setAttribute(/** @type {string} */ name, /** @type {string} */ value) {
    this.attributes[name] = value;
  }
  hasAttribute(/** @type {string} */ name) {
    return name in this.attributes;
  }
  removeAttribute(/** @type {string} */ name) {
    delete this.attributes[name];
  }
  append(/** @type {unknown[]} */ ...nodes) {
    this.children.push(...nodes);
  }
  replaceChildren(/** @type {unknown[]} */ ...nodes) {
    this.children = nodes;
  }
  replaceWith(/** @type {FakeElement} */ node) {
    this.replacedWith = node;
  }
  matches() {
    return false;
  }
  /** Everything this element shows, flattened to text. */
  get text() {
    return [this.textContent, ...this.children.map((c) => (c instanceof FakeElement ? c.text : ''))]
      .filter(Boolean)
      .join(' | ');
  }
}

/**
 * Load ui/app.js. Each `current_view` call stays pending until the test
 * replies to it, so the test decides the order replies arrive in.
 * @param {{ view?: object, loading?: object | null }} [ready] the reply to `frontend_ready`
 */
async function loadApp(ready = {}) {
  const content = new FakeElement('main');
  const status = new FakeElement('div');
  status.hidden = true;
  /** @type {((view: unknown) => void)[]} */
  const replies = [];
  /** @type {Record<string, (event: { payload: unknown }) => void>} */
  const tauriListeners = {};
  /** @type {Record<string, (event: object) => void>} */
  const domListeners = {};
  /** @type {{ generation: number, kind: string }[]} */
  const rendered = [];
  /** @type {{ run: () => void, ms: number, cancelled: boolean }[]} */
  const timers = [];

  globalThis.window = /** @type {any} */ ({
    __TAURI__: {
      core: {
        invoke: (/** @type {string} */ cmd) => {
          if (cmd === 'current_view') return new Promise((resolve) => replies.push(resolve));
          if (cmd === 'frontend_ready') {
            return Promise.resolve({
              view: ready.view ?? { generation: 0, kind: 'empty' },
              loading: ready.loading ?? null,
            });
          }
          return Promise.reject(new Error(`unexpected command ${cmd}`));
        },
      },
      event: {
        listen: async (/** @type {string} */ event, /** @type {any} */ handler) => {
          tauriListeners[event] = handler;
          return () => {};
        },
      },
    },
    scrollTo() {},
    dispatchEvent: (/** @type {any} */ event) => rendered.push(event.detail),
  });
  globalThis.document = /** @type {any} */ ({
    getElementById: (/** @type {string} */ id) => (id === 'status' ? status : content),
    addEventListener: (/** @type {string} */ type, /** @type {any} */ handler) => {
      domListeners[type] = handler;
    },
    createElement: (/** @type {string} */ tag) => new FakeElement(tag),
  });
  globalThis.Element = /** @type {any} */ (FakeElement);
  globalThis.CustomEvent = /** @type {any} */ (
    class {
      constructor(/** @type {string} */ type, /** @type {any} */ init) {
        this.type = type;
        this.detail = init.detail;
      }
    }
  );
  // Timers are run by the test, never by the clock.
  globalThis.setTimeout = /** @type {any} */ (
    (/** @type {() => void} */ run, /** @type {number} */ ms) => timers.push({ run, ms, cancelled: false }) - 1
  );
  globalThis.clearTimeout = /** @type {any} */ (
    (/** @type {number | undefined} */ id) => {
      if (id !== undefined && timers[id]) timers[id].cancelled = true;
    }
  );

  // A query string gives each load its own module instance.
  loads += 1;
  await import(`../../ui/app.js?load=${loads}`);

  return {
    content,
    status,
    replies,
    rendered,
    /** Fire a Tauri event from Rust. */
    emit: (/** @type {string} */ event, /** @type {unknown} */ payload) => tauriListeners[event]({ payload }),
    /** Dispatch a DOM event on a target; returns whether the app cancelled it. */
    cancels: (/** @type {string} */ type, /** @type {unknown} */ target) => {
      let cancelled = false;
      domListeners[type]({ target, preventDefault: () => (cancelled = true) });
      return cancelled;
    },
    /** Let every pending timer fire, as if its delay had passed. */
    runTimers: () => {
      for (const timer of timers.splice(0)) if (!timer.cancelled) timer.run();
    },
    pendingTimers: () => timers.filter((timer) => !timer.cancelled).length,
  };
}

/** @param {number} generation @param {string} html @param {string[]} [notices] */
const doc = (generation, html, notices = []) => ({
  generation,
  kind: 'document',
  title: 't',
  path: 'p',
  html,
  notices,
});

/** A link as `closest('a[href]')` would find it. */
const linkTo = (/** @type {string} */ href) => {
  const link = new FakeElement('a');
  link.attributes.href = href;
  const inner = new FakeElement('span');
  /** @type {any} */ (inner).closest = () => link;
  return inner;
};

test('a late reply to an earlier fetch cannot replace a newer view', { timeout: 5000 }, async () => {
  const app = await loadApp();
  app.emit('ezmark://view-changed', 1);
  app.emit('ezmark://view-changed', 2);
  const [first, second] = app.replies.splice(0);
  assert.ok(first && second, 'two fetches in flight');

  second(doc(2, 'NEW'));
  await settle();
  assert.equal(app.content.children.at(-1).html, 'NEW', 'the newer reply renders');

  first(doc(1, 'OLD'));
  await settle();
  assert.equal(app.content.children.at(-1).html, 'NEW', 'the older reply is ignored');
  assert.deepEqual(app.rendered.map((r) => r.generation), [0, 2]);
});

test('a view-changed event for a view already shown costs no fetch', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(3, 'SHOWN') });
  assert.equal(app.replies.length, 0, 'the ready reply carried the view: no fetch at startup');
  app.emit('ezmark://view-changed', 3);
  app.emit('ezmark://view-changed', 2);
  assert.equal(app.replies.length, 0, 'not newer than what is shown');
  app.emit('ezmark://view-changed', 4);
  assert.equal(app.replies.length, 1, 'a newer generation is fetched');
});

test('a file opened at launch does not flash the drop-a-file screen', { timeout: 5000 }, async () => {
  const app = await loadApp({ loading: { generation: 1, title: 'big.md' } });
  assert.deepEqual(app.rendered, [], 'nothing rendered while the document is on its way');
  assert.equal(app.status.hidden, true, 'and no notice yet');

  app.runTimers();
  assert.equal(app.status.hidden, false, 'a slow render is announced');
  assert.equal(app.status.textContent, 'Still rendering big.md…');

  app.emit('ezmark://view-changed', 1);
  app.replies[0](doc(1, 'DOC'));
  await settle();
  assert.equal(app.status.hidden, true, 'the notice goes when the view arrives');
});

test('the still-rendering notice follows the newest request', { timeout: 5000 }, async () => {
  const app = await loadApp();
  app.emit('ezmark://loading', { generation: 1, title: 'one.md' });
  app.emit('ezmark://loading', { generation: 2, title: 'two.md' });
  assert.equal(app.pendingTimers(), 1, 'the first wait was replaced');
  app.runTimers();
  assert.equal(app.status.textContent, 'Still rendering two.md…');

  // The superseded render's view never arrives; an older one must not clear it.
  app.emit('ezmark://loading', { generation: 0, title: 'stale.md' });
  assert.equal(app.status.hidden, false, 'a stale loading event changes nothing');

  app.emit('ezmark://view-changed', 2);
  app.replies[0](doc(2, 'TWO'));
  await settle();
  assert.equal(app.status.hidden, true);

  // A render that finishes quickly never shows the notice at all.
  app.emit('ezmark://loading', { generation: 3, title: 'quick.md' });
  app.emit('ezmark://view-changed', 3);
  app.replies[1](doc(3, 'QUICK'));
  await settle();
  assert.equal(app.pendingTimers(), 0, 'the wait was cancelled');
});

test('only in-page links are left to the webview', { timeout: 5000 }, async () => {
  const app = await loadApp();
  for (const type of ['click', 'auxclick', 'contextmenu']) {
    for (const href of ['https://example.com/', 'http://localhost:3000/', 'other.md', 'mailto:a@b.c', '']) {
      assert.equal(app.cancels(type, linkTo(href)), true, `${type} on ${href || '(empty href)'}`);
    }
    assert.equal(app.cancels(type, linkTo('#user-content-intro')), false, `${type} on a fragment`);
    const plain = new FakeElement('p');
    /** @type {any} */ (plain).closest = () => null;
    assert.equal(app.cancels(type, plain), false, `${type} on plain text`);
    assert.equal(app.cancels(type, {}), false, `${type} on something that is not an element`);
  }
});

test('images that may not load are replaced with a sentence before insertion', { timeout: 5000 }, async () => {
  const remote = `ezmark-blocked:remote:${Buffer.from('https://example.com/a.png').toString('hex')}`;
  const app = await loadApp({
    view: doc(1, `[img ${remote}|A badge] [img http://ezasset.localhost/1/6162|ok] [img |no source]`),
  });
  const [blocked, local, bare] = app.content.children.at(-1).images;

  assert.equal(local.replacedWith, null, 'an image that loads is left alone');
  assert.equal(blocked.replacedWith.className, 'image-placeholder');
  assert.equal(blocked.replacedWith.attributes['data-reason'], 'remote');
  assert.equal(blocked.replacedWith.text, 'Remote image not loaded | A badge | https://example.com/a.png');
  assert.equal(blocked.replacedWith.title, 'https://example.com/a.png');
  assert.equal(bare.replacedWith.text, 'Image not shown | no source');
});

test('an image with a blocked src but a usable srcset is kept', { timeout: 5000 }, async () => {
  const missing = `ezmark-blocked:missing:${Buffer.from('gone.png').toString('hex')}`;
  const app = await loadApp({ view: doc(1, `[img ${missing}|has srcset]`) });
  // The fake parser has no srcset syntax; give the image one and render again.
  const [img] = app.content.children.at(-1).images;
  assert.ok(img.replacedWith, 'without a srcset it is replaced');

  const again = await loadApp();
  const original = FakeElement.imagesIn;
  FakeElement.imagesIn = (html) => original(html).map((image) => ((image.attributes.srcset = 'ezasset://x 2x'), image));
  try {
    again.emit('ezmark://view-changed', 1);
    again.replies[0](doc(1, `[img ${missing}|has srcset]`));
    await settle();
  } finally {
    FakeElement.imagesIn = original;
  }
  const [kept] = again.content.children.at(-1).images;
  assert.equal(kept.replacedWith, null, 'it stays, to show its srcset');
  assert.equal(kept.hasAttribute('src'), false, 'without the dead src');
});

test('notices from Rust are shown above the document as text', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, 'BODY', ['Shown as <b>plain</b> text.']) });
  const [bar, body] = app.content.children;
  assert.equal(bar.className, 'document-notice');
  assert.equal(bar.textContent, 'Shown as <b>plain</b> text.', 'set as text, never parsed as HTML');
  assert.equal(body.html, 'BODY');
  assert.equal(app.content.className, 'document');
});

test('error and empty views are built from text', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: { generation: 1, kind: 'error', title: 't', message: 'File not found: <x>' } });
  assert.equal(app.content.className, 'notice error');
  assert.equal(app.content.text, 'Could not open file | File not found: <x>');

  const empty = await loadApp();
  assert.equal(empty.content.className, 'notice empty');
  assert.match(empty.content.text, /Drop a markdown file here/);
});

test('blocked-image markers are read strictly', () => {
  const hex = (/** @type {string} */ text) => Buffer.from(text).toString('hex');
  assert.deepEqual(blockedImage(`ezmark-blocked:outside:${hex('../ünï.png')}`, false), {
    reason: 'outside',
    label: 'Image is outside the folder ezmark may read',
    detail: '../ünï.png',
  });
  assert.equal(blockedImage(`ezmark-blocked:missing:${hex('a.png')}`, false)?.label, 'Image not found');
  assert.equal(blockedImage(`ezmark-blocked:refused:${hex('C:x')}`, false)?.label, 'Image path not allowed');
  assert.equal(blockedImage('ezmark-blocked:novel:', false)?.label, 'Image not shown', 'an unknown reason');

  // Not markers: ordinary images, and anything malformed.
  for (const src of ['http://ezasset.localhost/1/6162', 'ezasset://localhost/1/6162', 'ezmark-blocked:remote:zz', 'ezmark-blocked:remote:616']) {
    assert.equal(blockedImage(src, false), null, src);
  }
  // No source at all: a placeholder, unless a srcset can still show something.
  assert.equal(blockedImage(null, false)?.reason, 'unsupported');
  assert.equal(blockedImage('', false)?.reason, 'unsupported');
  assert.equal(blockedImage(null, true), null);
});
