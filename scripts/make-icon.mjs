/**
 * Renders the FORGE app mark to a 1024x1024 PNG with no image dependencies —
 * Node's zlib is all a PNG encoder needs. Run once; `tauri icon` derives the
 * .ico / .icns / png set from the output.
 *
 *   node scripts/make-icon.mjs src-tauri/icons/source.png
 */
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname } from "node:path";

const OUT = process.argv[2] ?? "src-tauri/icons/source.png";
const SIZE = 1024;
const SS = 3; // supersample factor -> antialiasing without a raster library

const BG = [0x0b, 0x0d, 0x12];
const BORDER = [0x2a, 0x34, 0x46];
const ACCENT = [0x2e, 0x8b, 0xff];
const ACCENT_HI = [0x5c, 0xb8, 0xff];
const GLOW = [0x2e, 0x8b, 0xff];

// Arrow outline in a 24x24 box, matching the in-app mark.
const ARROW = [
  [5, 3.2],
  [19, 12.4],
  [12.9, 13.5],
  [16, 19.6],
  [13.4, 20.9],
  [10.3, 14.7],
  [5, 19.1],
];

const lerp = (a, b, t) => a + (b - a) * t;
const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);

function insidePolygon(px, py, poly) {
  let hit = false;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    if (yi > py !== yj > py && px < ((xj - xi) * (py - yi)) / (yj - yi) + xi) hit = !hit;
  }
  return hit;
}

/** Signed distance to the polygon boundary (positive outside). */
function distanceToPolygon(px, py, poly) {
  let best = Infinity;
  for (let i = 0, j = poly.length - 1; i < poly.length; j = i++) {
    const [xi, yi] = poly[i];
    const [xj, yj] = poly[j];
    const dx = xj - xi;
    const dy = yj - yi;
    const len2 = dx * dx + dy * dy || 1;
    const t = clamp01(((px - xi) * dx + (py - yi) * dy) / len2);
    const cx = xi + t * dx;
    const cy = yi + t * dy;
    best = Math.min(best, Math.hypot(px - cx, py - cy));
  }
  return best;
}

function roundedRectAlpha(x, y, w, h, r) {
  const dx = Math.max(Math.abs(x - w / 2) - (w / 2 - r), 0);
  const dy = Math.max(Math.abs(y - h / 2) - (h / 2 - r), 0);
  return Math.hypot(dx, dy) - r; // signed distance, negative inside
}

// ── render at SS x resolution, then box-downsample ───────────────
const big = SIZE * SS;
const acc = new Float32Array(SIZE * SIZE * 4);

// arrow placed centred, occupying ~58% of the tile
const scale = (big * 0.58) / 24;
const offX = (big - 24 * scale) / 2;
const offY = (big - 24 * scale) / 2;
const arrowBig = ARROW.map(([x, y]) => [offX + x * scale, offY + y * scale]);
const cx = big / 2;
const cy = big / 2;
const glowRadius = big * 0.42;

for (let py = 0; py < big; py++) {
  for (let px = 0; px < big; px++) {
    const sd = roundedRectAlpha(px, py, big, big, big * 0.18);
    if (sd > 0) continue; // outside the tile -> fully transparent

    let r, g, b;
    const edge = -sd; // distance inside the tile edge
    const borderW = big * 0.006;
    if (edge < borderW) {
      const t = edge / borderW;
      r = lerp(BORDER[0], BG[0], t);
      g = lerp(BORDER[1], BG[1], t);
      b = lerp(BORDER[2], BG[2], t);
    } else {
      [r, g, b] = BG;
    }

    // radial accent glow behind the mark
    const gl = clamp01(1 - Math.hypot(px - cx, py - cy) / glowRadius) ** 2 * 0.35;
    r = lerp(r, GLOW[0], gl);
    g = lerp(g, GLOW[1], gl);
    b = lerp(b, GLOW[2], gl);

    if (insidePolygon(px, py, arrowBig)) {
      const d = distanceToPolygon(px, py, arrowBig);
      const rim = clamp01(1 - d / (big * 0.012)); // brighter along the outline
      r = lerp(ACCENT[0], ACCENT_HI[0], rim);
      g = lerp(ACCENT[1], ACCENT_HI[1], rim);
      b = lerp(ACCENT[2], ACCENT_HI[2], rim);
    }

    const ox = (py / SS) | 0;
    const oy = (px / SS) | 0;
    const idx = (ox * SIZE + oy) * 4;
    acc[idx] += r;
    acc[idx + 1] += g;
    acc[idx + 2] += b;
    acc[idx + 3] += 255;
  }
}

const n = SS * SS;
const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter type: none
  for (let x = 0; x < SIZE; x++) {
    const s = (y * SIZE + x) * 4;
    const d = y * (SIZE * 4 + 1) + 1 + x * 4;
    raw[d] = Math.round(acc[s] / n);
    raw[d + 1] = Math.round(acc[s + 1] / n);
    raw[d + 2] = Math.round(acc[s + 2] / n);
    raw[d + 3] = Math.round(acc[s + 3] / n);
  }
}

// ── minimal PNG container ────────────────────────────────────────
const CRC = (() => {
  const t = new Int32Array(256);
  for (let i = 0; i < 256; i++) {
    let c = i;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    t[i] = c;
  }
  return (buf) => {
    let c = -1;
    for (const byte of buf) c = t[(c ^ byte) & 0xff] ^ (c >>> 8);
    return (c ^ -1) >>> 0;
  };
})();

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(CRC(body));
  return Buffer.concat([len, body, crc]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

mkdirSync(dirname(OUT), { recursive: true });
writeFileSync(OUT, png);
console.log(`${OUT}  ${SIZE}x${SIZE}  ${(png.length / 1024).toFixed(1)} KB`);
