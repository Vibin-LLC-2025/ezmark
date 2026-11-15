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
    this.nodeType = 1;
    /** What a block shows, as the test wrote it. */
    this.html = '';
    /** @type {FakeElement[]} */
    this.images = [];
    // A <template>: its content holds whatever blocks the test put in the view.
    this.content = {
      childNodes: /** @type {FakeElement[]} */ ([]),
      querySelectorAll: () => this.content.childNodes.flatMap((node) => node.images),
    };
  }
  /**
   * Blocks are written in test HTML separated by `¶`, each optionally led by
   * its key in braces: `{k1}First¶{k2}Second`. A key becomes a marker element
   * before the block, as Rust writes it.
   */
  set innerHTML(html) {
    this.content.childNodes = html.split('¶').flatMap((/** @type {string} */ part) => {
      const [, key, body] = /** @type {RegExpExecArray} */ (/^(?:\{([^}]*)\})?([^]*)$/.exec(part));
      const block = new FakeElement('div');
      block.html = body;
      block.images = FakeElement.imagesIn(body);
      if (key === undefined) return [block];
      const marker = new FakeElement('ezmark-block');
      marker.attributes.key = key;
      return [marker, block];
    });
  }
  get outerHTML() {
    return this.html;
  }
  querySelectorAll() {
    return [];
  }
  /** Every child of the page is 100px tall, stacked from the top. */
  getBoundingClientRect() {
    const index = this.parentElement ? this.parentElement.children.indexOf(this) : 0;
    const top = index * 100 - /** @type {any} */ (globalThis.window).scrollY;
    return { top, bottom: top + 100 };
  }
  remove() {
    const parent = this.parentElement;
    if (parent) parent.children = parent.children.filter((child) => child !== this);
    this.parentElement = null;
  }
  insertBefore(/** @type {FakeElement} */ node, /** @type {FakeElement | null} */ before) {
    const at = before ? this.children.indexOf(before) : this.children.length;
    this.children.splice(at, 0, node);
    node.parentElement = this;
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
    for (const node of nodes) if (node instanceof FakeElement) node.parentElement = this;
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
  /** @type {{ id: number, run: () => void, ms: number, cancelled: boolean }[]} */
  const timers = [];
  /** Every scroll the app asked for: `['to', y]` or `['by', dy]`. */
  /** @type {[string, number][]} */
  const scrolls = [];

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
    scrollY: 0,
    innerHeight: 600,
    scrollTo: (/** @type {number} */ _x, /** @type {number} */ y) => scrolls.push(['to', y]),
    scrollBy: (/** @type {number} */ _x, /** @type {number} */ y) => scrolls.push(['by', y]),
    addEventListener() {},
    removeEventListener() {},
    dispatchEvent: (/** @type {any} */ event) => rendered.push(event.detail),
  });
  globalThis.document = /** @type {any} */ ({
    documentElement: { scrollHeight: 3000 },
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
  // Ids are never reused, as in a browser: clearing a timer that already ran
  // must not cancel a later one.
  let timerIds = 0;
  globalThis.setTimeout = /** @type {any} */ (
    (/** @type {() => void} */ run, /** @type {number} */ ms) => {
      timerIds += 1;
      timers.push({ id: timerIds, run, ms, cancelled: false });
      return timerIds;
    }
  );
  globalThis.clearTimeout = /** @type {any} */ (
    (/** @type {number | undefined} */ id) => {
      const timer = timers.find((candidate) => candidate.id === id);
      if (timer) timer.cancelled = true;
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
    scrolls,
    /** Pretend the reader has scrolled this far down. */
    scrollTo: (/** @type {number} */ y) => (/** @type {any} */ (globalThis.window).scrollY = y),
    /** The blocks in the page, as the test wrote them. */
    blocks: () => content.children.filter((/** @type {any} */ c) => c.localName === 'div'),
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
  app.replies[1]({ ...doc(3, 'QUICK'), path: 'quick.md' });
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

/**
 * Show `html` as generation `generation` of the same file, as live reload does.
 * @param {Awaited<ReturnType<typeof loadApp>>} app
 */
async function reload(app, /** @type {number} */ generation, /** @type {string} */ html, /** @type {string[]} */ notices = []) {
  app.emit('ezmark://view-changed', generation);
  /** @type {any} */ (app.replies.shift())(doc(generation, html, notices));
  await settle();
}

test('a reload replaces only the blocks that changed', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{a}One¶{b}Two¶{c}Three¶{d}Four') });
  const [one, two, three, four] = app.blocks();
  assert.deepEqual(app.scrolls, [['to', 0]], 'a newly opened document starts at the top');
  assert.ok(one && four);

  await reload(app, 2, '{a}One¶{b2}Two, edited¶{new}Added¶{c}Three¶{d}Four');
  const after = app.blocks();
  assert.deepEqual(after.map((b) => b.html), ['One', 'Two, edited', 'Added', 'Three', 'Four']);
  assert.equal(after[0], one, 'an unchanged block before the edit is the same node');
  assert.equal(after[3], three, 'and so are the ones after it');
  assert.equal(after[4], four);
  assert.notEqual(after[1], two);
  assert.equal(two.parentElement, null, 'the old block left the page');
  assert.equal(app.content.children.some((/** @type {any} */ c) => c.localName === 'ezmark-block'), false, 'markers never reach the page');
  assert.deepEqual(app.rendered.at(-1), { generation: 2, kind: 'document', reloaded: true });
  assert.deepEqual(app.scrolls, [['to', 0]], 'a reader at the top stays at the top');
});

test('a reload keeps the reader on the block they were reading', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{a}One¶{b}Two¶{c}Three') });
  const three = app.blocks()[2];
  // Blocks are 100px tall: at 230, Three is the first one still reaching into
  // the window, with 30px of it scrolled past.
  app.scrollTo(230);
  app.scrolls.length = 0;

  // A block is added above, which pushes Three 100px down the page.
  await reload(app, 2, '{a}One¶{x}Inserted¶{b}Two¶{c}Three');
  assert.equal(app.blocks()[3], three);
  assert.deepEqual(app.scrolls, [['by', 100]], 'scrolled by exactly what was inserted above');
  assert.equal(app.status.textContent, 'Updated');
  assert.equal(app.status.hidden, false);
  app.runTimers();
  assert.equal(app.status.hidden, true, 'the note goes away by itself');
});

test('a reload never lands the reader on one of several identical blocks', { timeout: 5000 }, async () => {
  // The plan's counterexample: A X X B, reading the first X, and an X is inserted above.
  const app = await loadApp({ view: doc(1, '{a}A¶{x}X¶{x}X¶{b}B') });
  app.scrollTo(120); // 20px into the first X
  app.scrolls.length = 0;

  await reload(app, 2, '{a}A¶{x}X¶{x}X¶{x}X¶{b}B');
  // X is not unique, so the nearest unique block above, A, goes to the top
  // of the window: its top, 120px above, is scrolled to offset zero.
  assert.deepEqual(app.scrolls, [['by', -120]]);
});

test('with nothing unique above the reader, a reload keeps the scroll ratio', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{x}X¶{x}X¶{x}X') });
  app.scrollTo(1200); // half of the 2400px there is to scroll, past every block
  app.scrolls.length = 0;
  await reload(app, 2, '{x}X¶{x}X¶{x}X¶{x}X');
  assert.deepEqual(app.scrolls, [['to', 1200]]);
});

test('opening another file starts at the top and forgets the old blocks', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{a}One¶{b}Two') });
  const [one] = app.blocks();
  app.scrollTo(400);
  app.scrolls.length = 0;
  app.emit('ezmark://view-changed', 2);
  /** @type {any} */ (app.replies.shift())({ ...doc(2, '{a}One¶{b}Two'), path: 'another.md' });
  await settle();
  assert.notEqual(app.blocks()[0], one, 'same content, different file: nothing is reused');
  assert.deepEqual(app.scrolls, [['to', 0]]);
  assert.equal(app.rendered.at(-1).reloaded, false);
});

test('notices are replaced on reload and stay above the blocks', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{a}One', ['First notice']) });
  await reload(app, 2, '{a}One¶{b}Two', ['Second notice']);
  assert.deepEqual(
    app.content.children.map((/** @type {any} */ c) => c.textContent || c.html),
    ['Second notice', 'One', 'Two'],
  );
});

test('a file that goes missing is announced until it is read again', { timeout: 5000 }, async () => {
  const app = await loadApp({ view: doc(1, '{a}One') });
  app.emit('ezmark://file-status', { missing: true, title: 'notes.md' });
  assert.equal(app.status.hidden, false);
  assert.match(app.status.textContent, /^notes\.md was moved or deleted\./);

  // A slow render elsewhere does not push the message away.
  app.emit('ezmark://loading', { generation: 2, title: 'notes.md' });
  app.runTimers();
  assert.match(app.status.textContent, /moved or deleted/);

  // A reload does not unsay it. The file can go while its last version is
  // still being rendered, and that view then arrives after the message.
  await reload(app, 2, '{a}One, again');
  assert.match(app.status.textContent, /moved or deleted/, 'only Rust withdraws the message');

  app.emit('ezmark://file-status', { missing: false, title: 'notes.md' });
  assert.equal(app.status.textContent, 'Updated', 'the file is back: the message is withdrawn');

  // Another file is another matter: its view starts with a clean slate.
  app.emit('ezmark://file-status', { missing: true, title: 'notes.md' });
  app.emit('ezmark://view-changed', 3);
  /** @type {any} */ (app.replies.shift())({ ...doc(3, '{b}Two'), path: 'another.md' });
  await settle();
  app.runTimers(); // let the earlier "Updated" note run out
  assert.equal(app.status.hidden, true);
});

test('a position hold ends when the next view arrives', { timeout: 5000 }, async () => {
  /** @type {{ run: () => void, connected: boolean }[]} */
  const observers = [];
  /** @type {any} */ (globalThis).ResizeObserver = class {
    constructor(/** @type {() => void} */ run) {
      this.entry = { run, connected: false };
      observers.push(this.entry);
    }
    observe() {
      this.entry.connected = true;
    }
    disconnect() {
      this.entry.connected = false;
    }
  };
  try {
    const app = await loadApp({ view: doc(1, '{a}One¶{b}Two¶{c}Three') });
    app.scrollTo(230);
    await reload(app, 2, '{a}One¶{x}Inserted¶{b}Two¶{c}Three');
    assert.equal(observers.length, 1);
    assert.equal(observers[0].connected, true, 'held while late layout may still move the page');

    // A second reload replaces the hold instead of adding to it.
    await reload(app, 3, '{a}One¶{x}Inserted¶{y}Again¶{b}Two¶{c}Three');
    assert.deepEqual(observers.map((o) => o.connected), [false, true]);

    // Another document: nothing of the old one may move it.
    app.emit('ezmark://view-changed', 4);
    /** @type {any} */ (app.replies.shift())({ ...doc(4, '{q}Other'), path: 'another.md' });
    await settle();
    assert.deepEqual(observers.map((o) => o.connected), [false, false]);
    app.scrolls.length = 0;
    for (const observer of observers) if (observer.connected) observer.run();
    assert.deepEqual(app.scrolls, []);
  } finally {
    delete (/** @type {any} */ (globalThis).ResizeObserver);
  }
});
