// Draws ezmark's 1024px source icon with no dependencies (Node 18+).
// Regenerate the platform icons from it with the pinned tauri-cli:
//   node scripts/make-icon.mjs crates/app/icons/source.png
//   cargo tauri icon crates/app/icons/source.png -o crates/app/icons
import { writeFileSync } from 'node:fs';
import { deflateSync } from 'node:zlib';

const SIZE = 1024;
const SAMPLES = 4; // 4x4 supersampling for smooth edges
const BACKGROUND = [45, 108, 223];
const GLYPH = [255, 255, 255];

/** An "M" and a down arrow: markdown, read-only. */
const shapes = [
  [[220, 700], [220, 330], [310, 330], [415, 470], [520, 330], [610, 330], [610, 700],
   [520, 700], [520, 480], [415, 620], [310, 480], [310, 700]],
  [[700, 330], [790, 330], [790, 540], [840, 540], [745, 700], [650, 540], [700, 540]],
];

function insideRoundedRect(x, y) {
  const [min, max, r] = [64, 960, 180];
  if (x < min || x > max || y < min || y > max) return false;
  const cx = Math.min(Math.max(x, min + r), max - r);
  const cy = Math.min(Math.max(y, min + r), max - r);
  return (x - cx) ** 2 + (y - cy) ** 2 <= r * r;
}

function insidePolygon(x, y, points) {
  let inside = false;
  for (let i = 0, j = points.length - 1; i < points.length; j = i++) {
    const [xi, yi] = points[i];
    const [xj, yj] = points[j];
    if (yi > y !== yj > y && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) inside = !inside;
  }
  return inside;
}

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  const row = y * (SIZE * 4 + 1);
  raw[row] = 0; // filter: none
  for (let x = 0; x < SIZE; x++) {
    let bg = 0;
    let glyph = 0;
    for (let sy = 0; sy < SAMPLES; sy++) {
      for (let sx = 0; sx < SAMPLES; sx++) {
        const px = x + (sx + 0.5) / SAMPLES;
        const py = y + (sy + 0.5) / SAMPLES;
        if (!insideRoundedRect(px, py)) continue;
        if (shapes.some((shape) => insidePolygon(px, py, shape))) glyph++;
        else bg++;
      }
    }
    const total = SAMPLES * SAMPLES;
    const alpha = (bg + glyph) / total;
    const i = row + 1 + x * 4;
    for (let c = 0; c < 3; c++) {
      const covered = bg + glyph;
      raw[i + c] = covered ? Math.round((BACKGROUND[c] * bg + GLYPH[c] * glyph) / covered) : 0;
    }
    raw[i + 3] = Math.round(alpha * 255);
  }
}

const crcTable = Array.from({ length: 256 }, (_, n) => {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  return c >>> 0;
});
function crc32(buffer) {
  let c = 0xffffffff;
  for (const byte of buffer) c = crcTable[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const length = Buffer.alloc(4);
  length.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([length, body, crc]);
}

const header = Buffer.alloc(13);
header.writeUInt32BE(SIZE, 0);
header.writeUInt32BE(SIZE, 4);
header.set([8, 6, 0, 0, 0], 8); // 8-bit RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', header),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);
writeFileSync(process.argv[2] ?? 'icon-source.png', png);
