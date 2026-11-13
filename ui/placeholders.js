// @ts-check
// What to say in place of an image that may not load. mdcore marks such an
// image by setting its `src` to `ezmark-blocked:<reason>:<hex of the original
// reference>` (crates/mdcore/src/render.rs). The scheme is not in the CSP, so
// the marker itself can never load anything; this module only reads it.

const MARKER = /^ezmark-blocked:([a-z]+):((?:[0-9a-f]{2})*)$/;

/** @type {Record<string, string>} */
const LABELS = {
  remote: 'Remote image not loaded',
  outside: 'Image is outside the folder ezmark may read',
  missing: 'Image not found',
  refused: 'Image path not allowed',
};

const NOT_SHOWN = 'Image not shown';

/**
 * @typedef {object} BlockedImage
 * @property {string} reason  `remote`, `outside`, `missing`, `refused`, or `unsupported`
 * @property {string} label   a short sentence for the reader
 * @property {string} detail  the reference the document made, or an empty string
 */

/**
 * @param {string | null} src       the image's `src` attribute
 * @param {boolean} hasSrcset       whether it also has a (local) `srcset`
 * @returns {BlockedImage | null}   `null` if the image is one that loads
 */
export function blockedImage(src, hasSrcset) {
  if (!src) {
    // The sanitizer removed the source outright (a `data:` or `file:` URL,
    // for example). With a srcset left, the image still has something to show.
    return hasSrcset ? null : { reason: 'unsupported', label: NOT_SHOWN, detail: '' };
  }
  const marker = MARKER.exec(src);
  if (!marker) return null;
  const [, reason, hex] = marker;
  return { reason, label: LABELS[reason] ?? NOT_SHOWN, detail: fromHex(hex) };
}

/** @param {string} hex */
function fromHex(hex) {
  const bytes = new Uint8Array(hex.length / 2);
  for (let i = 0; i < bytes.length; i += 1) {
    bytes[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
  }
  return new TextDecoder().decode(bytes);
}
