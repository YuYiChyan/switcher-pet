// Inspect the actual PE, not just build-script text. No executable is launched.
import { readFileSync, existsSync } from 'node:fs';
import { dirname, join } from 'node:path';
import assert from 'node:assert/strict';
const path = process.argv[2];
assert.ok(path, 'Usage: node scripts/verify-windows-artifact.mjs <exe>');
const b = readFileSync(path);
assert.equal(b.toString('ascii', 0, 2), 'MZ');
const pe = b.readUInt32LE(0x3c);
assert.equal(b.readUInt32LE(pe), 0x4550);
const count = b.readUInt16LE(pe + 6), opts = b.readUInt16LE(pe + 20), opt = pe + 24;
const dirs = opt + (b.readUInt16LE(opt) === 0x20b ? 112 : 96);
const sections = Array.from({ length: count }, (_, i) => {
  const o = opt + opts + i * 40;
  return { rva: b.readUInt32LE(o + 12), size: Math.max(b.readUInt32LE(o + 8), b.readUInt32LE(o + 16)), raw: b.readUInt32LE(o + 20) };
});
function offset(rva) {
  const s = sections.find(s => rva >= s.rva && rva < s.rva + s.size);
  assert.ok(s, 'Invalid resource RVA');
  return s.raw + rva - s.rva;
}
const root = offset(b.readUInt32LE(dirs + 16));
const manifests = [];
function walk(rel, path = []) {
  assert.ok(path.length < 4, 'Invalid resource depth');
  const p = root + rel, n = b.readUInt16LE(p + 12) + b.readUInt16LE(p + 14);
  for (let i = 0; i < n; i++) {
    const e = p + 16 + i * 8, id = b.readUInt32LE(e), v = b.readUInt32LE(e + 4);
    if (path.length === 0 && id !== 24) continue;
    if (v & 0x80000000) walk(v & 0x7fffffff, [...path, id]);
    else {
      const d = root + v, raw = offset(b.readUInt32LE(d)), size = b.readUInt32LE(d + 4);
      manifests.push(b.subarray(raw, raw + size).toString('utf8'));
    }
  }
}
walk(0);
assert.equal(manifests.length, 1, 'EXE must contain exactly one application manifest');
assert.match(manifests[0], /Microsoft.Windows.Common-Controls/);
assert.match(manifests[0], /longPathAware>true</);
assert.match(manifests[0], /requestedExecutionLevel level="asInvoker"/);
assert.ok(existsSync(join(dirname(path), 'WebView2Loader.dll')), 'Missing WebView2Loader.dll');
console.log('PASS: PE has exactly one manifest, Common Controls v6, long paths, asInvoker, and adjacent WebView2 loader.');
