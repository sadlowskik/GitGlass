// Generates placeholder GitGlass app icons (solid violet with a rounded feel).
// Run: node scripts/gen-icons.mjs
// Replace later with real artwork via: npm run tauri icon <source.png>
import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const outDir = join(dirname(fileURLToPath(import.meta.url)), "..", "src-tauri", "icons");
mkdirSync(outDir, { recursive: true });

// Brand accent (violet) with a subtle diagonal to blue, alpha rounded corners.
const ACCENT = [139, 122, 255];
const BLUE = [88, 166, 255];

function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return (~c) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const typeBuf = Buffer.from(type, "ascii");
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(Buffer.concat([typeBuf, data])));
  return Buffer.concat([len, typeBuf, data, crc]);
}

function makePng(size) {
  const radius = Math.round(size * 0.22);
  const raw = Buffer.alloc(size * (size * 4 + 1));
  let p = 0;
  for (let y = 0; y < size; y++) {
    raw[p++] = 0; // filter: none
    for (let x = 0; x < size; x++) {
      const t = (x + y) / (2 * size);
      const r = Math.round(ACCENT[0] * (1 - t) + BLUE[0] * t);
      const g = Math.round(ACCENT[1] * (1 - t) + BLUE[1] * t);
      const b = Math.round(ACCENT[2] * (1 - t) + BLUE[2] * t);
      // Rounded-corner alpha mask.
      const inCorner =
        (x < radius && y < radius && dist(x, y, radius, radius) > radius) ||
        (x >= size - radius && y < radius && dist(x, y, size - radius, radius) > radius) ||
        (x < radius && y >= size - radius && dist(x, y, radius, size - radius) > radius) ||
        (x >= size - radius && y >= size - radius && dist(x, y, size - radius, size - radius) > radius);
      raw[p++] = r;
      raw[p++] = g;
      raw[p++] = b;
      raw[p++] = inCorner ? 0 : 255;
    }
  }

  const sig = Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(size, 0);
  ihdr.writeUInt32BE(size, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type RGBA
  const idat = deflateSync(raw, { level: 9 });
  return Buffer.concat([
    sig,
    chunk("IHDR", ihdr),
    chunk("IDAT", idat),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

function dist(x, y, cx, cy) {
  return Math.hypot(x - cx, y - cy);
}

// ICO that embeds a PNG (supported Vista+); good enough as a placeholder.
function makeIco(pngBuffers) {
  const count = pngBuffers.length;
  const header = Buffer.alloc(6);
  header.writeUInt16LE(0, 0);
  header.writeUInt16LE(1, 2); // type: icon
  header.writeUInt16LE(count, 4);
  const entries = [];
  let offset = 6 + count * 16;
  const sizes = [16, 32, 48, 256];
  pngBuffers.forEach((png, i) => {
    const e = Buffer.alloc(16);
    const s = sizes[i] >= 256 ? 0 : sizes[i];
    e[0] = s;
    e[1] = s;
    e[4] = 1; // color planes
    e.writeUInt16LE(32, 6); // bpp
    e.writeUInt32LE(png.length, 8);
    e.writeUInt32LE(offset, 12);
    offset += png.length;
    entries.push(e);
  });
  return Buffer.concat([header, ...entries, ...pngBuffers]);
}

const targets = {
  "32x32.png": 32,
  "128x128.png": 128,
  "128x128@2x.png": 256,
  "icon.png": 512,
  "Square150x150Logo.png": 150,
  "Square44x44Logo.png": 44,
  "StoreLogo.png": 50,
};

for (const [name, size] of Object.entries(targets)) {
  writeFileSync(join(outDir, name), makePng(size));
}

writeFileSync(
  join(outDir, "icon.ico"),
  makeIco([makePng(16), makePng(32), makePng(48), makePng(256)]),
);

console.log(`Generated placeholder icons in ${outDir}`);
