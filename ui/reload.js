// @ts-check
// Live reload, the view's half (plan section 9 and its amendments).
//
// Two separate questions, answered by two separate identities:
//
// - Which parts of the page can stay as they are? Answered by a block's
//   *signature*, a hash of its HTML. Blocks at the start and the end of the
//   document whose signatures did not change are left in the page untouched,
//   so their images do not reload and an open <details> stays open. A wrong
//   answer here would cost a re-rendered block, nothing more.
//
// - Where was the reader? Answered by a block's *key*, the hash of its
//   markdown source, which Rust sends in a marker element before each
//   top-level block (crates/mdcore/src/blocks.rs). A wrong answer here would
//   silently move the reader, so the rule is strict: a key is only ever
//   matched if exactly one block in the old document and exactly one in the
//   new document carry it. Two snapshots cannot tell identical blocks apart,
//   so identical blocks are never matched at all.

/** The marker element Rust writes before each top-level block. */
const MARKER = 'ezmark-block';

/**
 * One top-level node of the document.
 * @typedef {object} Block
 * @property {ChildNode} node
 * @property {string} key        the source key, or '' for a node without one
 * @property {string} signature  identifies the node's HTML
 */

/**
 * Where to put the reader after a reload: the block at `index` in the new
 * document, with its old offset if it is the very block they were on
 * (`exact`), otherwise at the top of the window. `null` means no block before
 * the reader's survived unambiguously; the caller falls back to scroll ratio.
 *
 * @param {readonly string[]} oldKeys
 * @param {readonly string[]} newKeys
 * @param {number} anchor  index in `oldKeys` of the block at the top of the window
 * @returns {{ index: number, exact: boolean } | null}
 */
export function restorePlan(oldKeys, newKeys, anchor) {
  const oldCount = count(oldKeys);
  /** @type {Map<string, number>} */
  const newCount = new Map();
  /** @type {Map<string, number>} */
  const newIndex = new Map();
  newKeys.forEach((key, index) => {
    newCount.set(key, (newCount.get(key) ?? 0) + 1);
    newIndex.set(key, index);
  });
  // Step 1 is the anchor itself; step 2 walks back from it.
  for (let i = Math.min(anchor, oldKeys.length - 1); i >= 0; i -= 1) {
    const key = oldKeys[i];
    if (key && oldCount.get(key) === 1 && newCount.get(key) === 1) {
      return { index: /** @type {number} */ (newIndex.get(key)), exact: i === anchor };
    }
  }
  return null;
}

/** @param {readonly string[]} keys */
function count(keys) {
  /** @type {Map<string, number>} */
  const counts = new Map();
  for (const key of keys) counts.set(key, (counts.get(key) ?? 0) + 1);
  return counts;
}

/**
 * How many blocks at the start (`head`) and at the end (`tail`) are the same
 * in both documents. Everything between them is replaced.
 *
 * @param {readonly string[]} oldSignatures
 * @param {readonly string[]} newSignatures
 */
export function unchangedEnds(oldSignatures, newSignatures) {
  const shorter = Math.min(oldSignatures.length, newSignatures.length);
  let head = 0;
  while (head < shorter && oldSignatures[head] === newSignatures[head]) head += 1;
  let tail = 0;
  while (
    tail < shorter - head &&
    oldSignatures[oldSignatures.length - 1 - tail] === newSignatures[newSignatures.length - 1 - tail]
  ) {
    tail += 1;
  }
  return { head, tail };
}

/**
 * A short stand-in for a string of HTML: its length and a 53-bit hash
 * (cyrb53). Kept instead of the HTML itself so that a large document is not
 * held in memory twice.
 * @param {string} html
 */
export function signature(html) {
  let h1 = 0xdeadbeef;
  let h2 = 0x41c6ce57;
  for (let i = 0; i < html.length; i += 1) {
    const code = html.charCodeAt(i);
    h1 = Math.imul(h1 ^ code, 2654435761);
    h2 = Math.imul(h2 ^ code, 1597334677);
  }
  h1 = Math.imul(h1 ^ (h1 >>> 16), 2246822507) ^ Math.imul(h2 ^ (h2 >>> 13), 3266489909);
  h2 = Math.imul(h2 ^ (h2 >>> 16), 2246822507) ^ Math.imul(h1 ^ (h1 >>> 13), 3266489909);
  return `${html.length}:${4294967296 * (2097151 & h2) + (h1 >>> 0)}`;
}

/**
 * Split parsed document HTML into its top-level blocks, taking the markers
 * out. A block's key is made of every marker since the previous block,
 * including any that ended up inside it (raw HTML that leaves an element open
 * swallows the blocks that follow, markers and all).
 *
 * @param {DocumentFragment} fragment  a template's content: inert, not yet in the page
 * @returns {Block[]}
 */
export function readBlocks(fragment) {
  /** @type {Block[]} */
  const blocks = [];
  /** @type {string[]} */
  let keys = [];
  for (const node of Array.from(fragment.childNodes)) {
    if (!isElement(node)) {
      // Text between blocks: no key, and it is its own signature.
      blocks.push({ node, key: '', signature: `text:${node.textContent ?? ''}` });
    } else if (node.localName === MARKER) {
      keys.push(node.getAttribute('key') ?? '');
      node.remove();
    } else {
      for (const marker of Array.from(node.querySelectorAll(MARKER))) {
        keys.push(marker.getAttribute('key') ?? '');
        marker.remove();
      }
      blocks.push({ node, key: keys.join(' '), signature: signature(node.outerHTML) });
      keys = [];
    }
  }
  return blocks;
}

/**
 * @param {ChildNode} node
 * @returns {node is Element}
 */
function isElement(node) {
  return node.nodeType === 1;
}

/**
 * Bring the page from `shown` to `next`, touching only what changed. Returns
 * the blocks now in the page: untouched nodes are kept, with their new keys.
 *
 * @param {HTMLElement} container  the element the blocks live in
 * @param {readonly Block[]} shown
 * @param {readonly Block[]} next
 * @returns {Block[]}
 */
export function reconcile(container, shown, next) {
  const { head, tail } = unchangedEnds(
    shown.map((block) => block.signature),
    next.map((block) => block.signature),
  );
  const before = tail > 0 ? shown[shown.length - tail].node : null;
  for (const block of shown.slice(head, shown.length - tail)) block.node.remove();
  const fresh = next.slice(head, next.length - tail);
  for (const block of fresh) container.insertBefore(block.node, before);
  const kept = (/** @type {number} */ from, /** @type {number} */ to, /** @type {number} */ shift) =>
    shown.slice(from, to).map((block, i) => ({ ...block, key: next[from + shift + i].key }));
  return [
    ...kept(0, head, 0),
    ...fresh,
    ...kept(shown.length - tail, shown.length, next.length - shown.length),
  ];
}

/**
 * The block at the top of the window and how far its top is from the
 * window's (negative once it has scrolled past). `null` if no block reaches
 * into the window.
 *
 * @param {readonly Block[]} blocks
 * @returns {{ index: number, offset: number } | null}
 */
export function captureAnchor(blocks) {
  for (let index = 0; index < blocks.length; index += 1) {
    const node = blocks[index].node;
    if (!isElement(node)) continue;
    const rect = node.getBoundingClientRect();
    if (rect.bottom > 0) return { index, offset: rect.top };
  }
  return null;
}
