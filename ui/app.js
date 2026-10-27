// @ts-check
// The view layer. Rust owns every decision; this file shows what Rust says to
// show and forwards nothing a document controls. Document HTML arrives already
// sanitized by mdcore.

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const content = /** @type {HTMLElement} */ (document.getElementById('content'));

let shownGeneration = -1;

/** @param {View} view */
function render(view) {
  if (view.generation === shownGeneration) return;
  shownGeneration = view.generation;

  switch (view.kind) {
    case 'document':
      content.innerHTML = view.html; // sanitized in Rust (mdcore)
      content.className = 'document';
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

/**
 * @param {string} heading
 * @param {string} text
 */
function notice(heading, text) {
  const title = document.createElement('h1');
  title.textContent = heading;
  const body = document.createElement('p');
  body.textContent = text;
  const wrapper = document.createElement('div');
  wrapper.append(title, body);
  return wrapper;
}

async function refresh() {
  render(/** @type {View} */ (await invoke('current_view')));
}

// No link ever navigates the webview. In-page anchors scroll natively;
// following other links is M2 work and goes through Rust.
/** @param {MouseEvent} event */
function interceptLinks(event) {
  const target = event.target instanceof Element ? event.target.closest('a[href]') : null;
  if (!target) return;
  if ((target.getAttribute('href') ?? '').startsWith('#')) return;
  event.preventDefault();
}
document.addEventListener('click', interceptLinks);
document.addEventListener('auxclick', interceptLinks);

await listen('ezmark://view-changed', () => void refresh());
await invoke('frontend_ready');
await refresh();
