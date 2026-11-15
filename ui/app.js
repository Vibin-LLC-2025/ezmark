// @ts-check
// The view layer. Rust owns every decision; this file shows what Rust says to
// show and forwards nothing a document controls. Document HTML arrives already
// sanitized by mdcore.

import { blockedImage } from './placeholders.js';
import { captureAnchor, readBlocks, reconcile, restorePlan } from './reload.js';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const content = /** @type {HTMLElement} */ (document.getElementById('content'));
const status = /** @type {HTMLElement} */ (document.getElementById('status'));

/** A render slower than this gets a "still rendering" line. */
const STILL_RENDERING_AFTER_MS = 700;
/** How long "Updated" stays up after a reload. */
const UPDATED_FOR_MS = 1500;
/** How long late layout (an image arriving) may still move the page back into place. */
const SETTLE_FOR_MS = 500;

let shownGeneration = -1;
let awaitedGeneration = -1;
/** @type {ReturnType<typeof setTimeout> | undefined} */
let stillRenderingTimer;
/** @type {ReturnType<typeof setTimeout> | undefined} */
let updatedTimer;
/** Stops the position hold of the last reload, if one is still running. */
let releaseHold = () => {};

/**
 * The document in the page, kept so that a reload can change only what changed.
 * @type {{ path: string, notices: HTMLElement[], blocks: import('./reload.js').Block[] } | null}
 */
let shown = null;

// The one status line says the most important thing that is true right now.
const says = { missing: '', rendering: '', updated: '' };

function paintStatus() {
  const text = says.missing || says.rendering || says.updated;
  if (text) status.textContent = text;
  status.hidden = !text;
}

/** @param {View} view */
function render(view) {
  // Generations only grow, so an older one is a late reply to an earlier fetch.
  if (view.generation <= shownGeneration) return;
  shownGeneration = view.generation;
  if (view.generation >= awaitedGeneration) stopWaiting();
  // Whatever a previous reload was holding in place is about to change.
  releaseHold();
  // Whether the open file is missing is Rust's to say (`file-status`), and a
  // reload does not unsay it: the file can go while its last version is
  // being rendered. A view of anything else starts with a clean slate.
  if (!(view.kind === 'document' && shown?.path === view.path)) says.missing = '';

  let reloaded = false;
  switch (view.kind) {
    case 'document':
      reloaded = showDocument(view);
      break;
    case 'error':
      shown = null;
      content.replaceChildren(notice('Could not open file', view.message));
      content.className = 'notice error';
      break;
    case 'empty':
      shown = null;
      content.replaceChildren(notice('ezmark', 'Drop a markdown file here to read it.'));
      content.className = 'notice empty';
      break;
  }
  if (!reloaded) window.scrollTo(0, 0);
  paintStatus();
  window.dispatchEvent(
    new CustomEvent('ezmark:rendered', {
      detail: { generation: view.generation, kind: view.kind, reloaded },
    }),
  );
}

/**
 * Show a document. If it is the one already in the page (the file changed on
 * disk), only the changed blocks are replaced and the reader stays put.
 * @param {Extract<View, { kind: 'document' }>} view
 * @returns {boolean} whether this was a reload of the document in the page
 */
function showDocument(view) {
  // A template's content is inert: nothing in it loads until it is inserted,
  // so blocked images are swapped for text before the webview ever sees them.
  const template = document.createElement('template');
  template.innerHTML = view.html; // sanitized in Rust (mdcore)
  for (const img of template.content.querySelectorAll('img')) {
    replaceIfBlocked(img);
  }
  const blocks = readBlocks(template.content);
  const notices = view.notices.map(documentNotice);
  content.className = 'document';

  if (!shown || shown.path !== view.path) {
    content.replaceChildren(...notices, ...blocks.map((block) => block.node));
    shown = { path: view.path, notices, blocks };
    return false;
  }

  const before = readingPosition(shown.blocks);
  const oldKeys = shown.blocks.map((block) => block.key);
  for (const bar of shown.notices) bar.remove();
  const merged = reconcile(content, shown.blocks, blocks);
  for (const bar of notices) content.insertBefore(bar, merged[0]?.node ?? null);
  shown = { path: view.path, notices, blocks: merged };
  restore(before, oldKeys, merged);
  flashUpdated();
  return true;
}

/**
 * Where the reader is, in the three forms a restore may need.
 * @param {readonly import('./reload.js').Block[]} blocks
 */
function readingPosition(blocks) {
  const room = document.documentElement.scrollHeight - window.innerHeight;
  return {
    atTop: window.scrollY <= 0,
    anchor: captureAnchor(blocks),
    ratio: room > 0 ? window.scrollY / room : 0,
  };
}

/**
 * Put the reader back (plan section 9, "Anchor restore").
 * @param {ReturnType<typeof readingPosition>} before
 * @param {readonly string[]} oldKeys
 * @param {readonly import('./reload.js').Block[]} blocks  what is in the page now
 */
function restore(before, oldKeys, blocks) {
  // At the very top there is no place to keep: stay at the top.
  if (before.atTop) return;
  const plan = before.anchor
    ? restorePlan(
        oldKeys,
        blocks.map((block) => block.key),
        before.anchor.index,
      )
    : null;
  if (!plan || !before.anchor) {
    const room = document.documentElement.scrollHeight - window.innerHeight;
    window.scrollTo(0, before.ratio * Math.max(room, 0));
    return;
  }
  const node = /** @type {Element} */ (blocks[plan.index].node);
  const offset = plan.exact ? before.anchor.offset : 0;
  const place = () => window.scrollBy(0, node.getBoundingClientRect().top - offset);
  place();
  holdInPlace(place);
}

/**
 * Layout can still move for a moment after a reload (an image arriving, a
 * font). Re-apply the position while it does, and stop the instant the reader
 * scrolls: from then on the position is theirs.
 * @param {() => void} place
 */
function holdInPlace(place) {
  if (typeof ResizeObserver === 'undefined') return;
  const observer = new ResizeObserver(place);
  const inputs = ['wheel', 'keydown', 'touchstart', 'pointerdown'];
  const stop = () => {
    observer.disconnect();
    clearTimeout(timer);
    for (const input of inputs) window.removeEventListener(input, stop);
    if (releaseHold === stop) releaseHold = () => {};
  };
  for (const input of inputs) window.addEventListener(input, stop, { passive: true });
  observer.observe(content);
  const timer = setTimeout(stop, SETTLE_FOR_MS);
  // The next view ends this hold: its anchor node may be gone by then, and
  // another document's layout is not this one's to correct.
  releaseHold = stop;
}

function flashUpdated() {
  clearTimeout(updatedTimer);
  says.updated = 'Updated';
  updatedTimer = setTimeout(() => {
    says.updated = '';
    paintStatus();
  }, UPDATED_FOR_MS);
}

/** @param {HTMLImageElement} img */
function replaceIfBlocked(img) {
  const blocked = blockedImage(img.getAttribute('src'), img.hasAttribute('srcset'));
  if (!blocked) return;
  // With a local `srcset` of its own, or inside a <picture> that still has a
  // usable <source>, the image has something to show and has to stay; only
  // its dead `src` goes.
  const picture = img.parentElement?.matches('picture') ? img.parentElement : null;
  if (img.hasAttribute('srcset') || picture?.querySelector('source[srcset]')) {
    img.removeAttribute('src');
    return;
  }
  const box = document.createElement('span');
  box.className = 'image-placeholder';
  box.setAttribute('data-reason', blocked.reason);
  box.append(labelled('span', 'image-placeholder-label', blocked.label));
  const alt = img.getAttribute('alt');
  if (alt) box.append(labelled('span', 'image-placeholder-alt', alt));
  if (blocked.detail) {
    box.append(labelled('code', 'image-placeholder-detail', blocked.detail));
    box.title = blocked.detail;
  }
  img.replaceWith(box);
}

/**
 * @param {string} tag
 * @param {string} className
 * @param {string} text
 */
function labelled(tag, className, text) {
  const element = document.createElement(tag);
  element.className = className;
  element.textContent = text;
  return element;
}

/** @param {string} text */
function documentNotice(text) {
  const bar = labelled('p', 'document-notice', text);
  bar.setAttribute('role', 'note');
  return bar;
}

/**
 * @param {string} heading
 * @param {string} text
 */
function notice(heading, text) {
  const wrapper = document.createElement('div');
  wrapper.append(labelled('h1', '', heading), labelled('p', '', text));
  return wrapper;
}

/**
 * A render has started. If its view has not arrived shortly, say so: a large
 * or hostile file can take seconds, and a window that shows nothing looks hung.
 * @param {Loading} loading
 */
function waitFor(loading) {
  if (loading.generation <= shownGeneration) return;
  stopWaiting();
  awaitedGeneration = loading.generation;
  stillRenderingTimer = setTimeout(() => {
    says.rendering = `Still rendering ${loading.title}…`;
    paintStatus();
  }, STILL_RENDERING_AFTER_MS);
}

function stopWaiting() {
  clearTimeout(stillRenderingTimer);
  says.rendering = '';
  paintStatus();
}

/**
 * The open file went away, or came back unchanged. The page keeps showing
 * what was last read; the reader is told that it may be out of date.
 * @param {FileStatus} fileStatus
 */
function fileChanged(fileStatus) {
  says.missing = fileStatus.missing
    ? `${fileStatus.title} was moved or deleted. This is the last version ezmark read.`
    : '';
  paintStatus();
}

async function refresh() {
  render(/** @type {View} */ (await invoke('current_view')));
}

/**
 * The link a mouse event landed on, unless it only jumps within the page.
 * @param {Event} event
 */
function outboundLink(event) {
  const link = event.target instanceof Element ? event.target.closest('a[href]') : null;
  if (!link || (link.getAttribute('href') ?? '').startsWith('#')) return null;
  return link;
}

// No link ever navigates the webview. In-page anchors scroll natively;
// following other links is M2 work and goes through Rust.
/** @param {Event} event */
function interceptLinks(event) {
  if (outboundLink(event)) event.preventDefault();
}
document.addEventListener('click', interceptLinks);
document.addEventListener('auxclick', interceptLinks);
// The webview's own menu on a link offers "Save link as" and "Open link in new
// window", which act on a URL the document chose. No menu on links; the
// default menu (copy, select all) stays everywhere else.
document.addEventListener('contextmenu', interceptLinks);

await listen('ezmark://view-changed', (event) => {
  // The payload is the new view's generation; skip the fetch if it is not news.
  if (/** @type {number} */ (event.payload) > shownGeneration) void refresh();
});
await listen('ezmark://loading', (event) => waitFor(/** @type {Loading} */ (event.payload)));
await listen('ezmark://file-status', (event) => fileChanged(/** @type {FileStatus} */ (event.payload)));

const ready = /** @type {Ready} */ (await invoke('frontend_ready'));
if (ready.loading) waitFor(ready.loading);
// A file opened at launch is still rendering: leave the window blank for the
// moment rather than flash the drop-a-file screen before the document.
if (!(ready.loading && ready.view.kind === 'empty')) render(ready.view);
