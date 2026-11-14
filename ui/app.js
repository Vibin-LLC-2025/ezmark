// @ts-check
// The view layer. Rust owns every decision; this file shows what Rust says to
// show and forwards nothing a document controls. Document HTML arrives already
// sanitized by mdcore.

import { blockedImage } from './placeholders.js';

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const content = /** @type {HTMLElement} */ (document.getElementById('content'));
const status = /** @type {HTMLElement} */ (document.getElementById('status'));

/** A render slower than this gets a "still rendering" line. */
const STILL_RENDERING_AFTER_MS = 700;

let shownGeneration = -1;
let awaitedGeneration = -1;
/** @type {ReturnType<typeof setTimeout> | undefined} */
let stillRenderingTimer;

/** @param {View} view */
function render(view) {
  // Generations only grow, so an older one is a late reply to an earlier fetch.
  if (view.generation <= shownGeneration) return;
  shownGeneration = view.generation;
  if (view.generation >= awaitedGeneration) stopWaiting();

  switch (view.kind) {
    case 'document':
      showDocument(view);
      break;
    case 'error':
      content.replaceChildren(notice('Could not open file', view.message));
      content.className = 'notice error';
      break;
    case 'empty':
      content.replaceChildren(notice('ezmark', 'Drop a markdown file here to read it.'));
      content.className = 'notice empty';
      break;
  }
  window.scrollTo(0, 0);
  window.dispatchEvent(
    new CustomEvent('ezmark:rendered', { detail: { generation: view.generation, kind: view.kind } }),
  );
}

/** @param {Extract<View, { kind: 'document' }>} view */
function showDocument(view) {
  // A template's content is inert: nothing in it loads until it is inserted,
  // so blocked images are swapped for text before the webview ever sees them.
  const template = document.createElement('template');
  template.innerHTML = view.html; // sanitized in Rust (mdcore)
  for (const img of template.content.querySelectorAll('img')) {
    replaceIfBlocked(img);
  }
  content.replaceChildren(...view.notices.map(documentNotice), template.content);
  content.className = 'document';
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
    status.textContent = `Still rendering ${loading.title}…`;
    status.hidden = false;
  }, STILL_RENDERING_AFTER_MS);
}

function stopWaiting() {
  clearTimeout(stillRenderingTimer);
  status.hidden = true;
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

const ready = /** @type {Ready} */ (await invoke('frontend_ready'));
if (ready.loading) waitFor(ready.loading);
// A file opened at launch is still rendering: leave the window blank for the
// moment rather than flash the drop-a-file screen before the document.
if (!(ready.loading && ready.view.kind === 'empty')) render(ready.view);
