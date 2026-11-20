import { deflateSync } from 'node:zlib';
import { writeFileSync } from 'node:fs';

const W = 1024, H = 1024, R = 200;
const raw = Buffer.alloc((W * 4 + 1) * H);

const A = [360, 280], B = [360, 744], C = [790, 512];
const sign = (p1, p2, p3) =>
  (p1[0] - p3[0]) * (p2[1] - p3[1]) - (p2[0] - p3[0]) * (p1[1] - p3[1]);
function inTri(x, y) {
  const p = [x, y];
  const d1 = sign(p, A, B), d2 = sign(p, B, C), d3 = sign(p, C, A);
  const neg = d1 < 0 || d2 < 0 || d3 < 0;
  const pos = d1 > 0 || d2 > 0 || d3 > 0;
  return !(neg && pos);
}

for (let y = 0; y < H; y++) {
  const row = y * (W * 4 + 1);
  for (let x = 0; x < W; x++) {
    const i = row + 1 + x * 4;
    let inside = true;
    if ((x < R || x >= W - R) && (y < R || y >= H - R)) {
      const cx = x < R ? R : W - 1 - R;
      const cy = y < R ? R : H - 1 - R;
      inside = (x - cx) ** 2 + (y - cy) ** 2 <= R * R;
    }
    if (!inside) continue;
    const t = y / H;
    let r = Math.round(0x23 + (0x10 - 0x23) * t);
    let g = Math.round(0x2c + (0x14 - 0x2c) * t);
    let b = Math.round(0x4e + (0x25 - 0x4e) * t);
    if (inTri(x, y)) { r = 0xf2; g = 0xf5; b = 0xfa; }
    raw[i] = r; raw[i + 1] = g; raw[i + 2] = b; raw[i + 3] = 255;
  }
}

const crcTable = [];
for (let n = 0; n < 256; n++) {
  let c = n;
  for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
  crcTable[n] = c >>> 0;
}
function crc32(buf) {
  let c = 0xffffffff;
  for (const b of buf) c = crcTable[(c ^ b) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}
function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const td = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(td));
  return Buffer.concat([len, td, crc]);
}
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8;
ihdr[9] = 6;
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);
writeFileSync(new URL('../app-icon.png', import.meta.url), png);
console.log('app-icon.png written:', png.length, 'bytes');