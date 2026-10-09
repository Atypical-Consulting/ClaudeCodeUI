// Builds the English variant of the built mockup: text nodes and title/aria-label/alt attributes are
// replaced from en.json. The layout, CSS and rendering pipeline are untouched.
// usage: node translate-en.js graphite.html graphite.en.html [en.json]
const fs = require('fs');
const [src, out, mapFile = 'en.json'] = process.argv.slice(2);
const map = JSON.parse(fs.readFileSync(mapFile, 'utf8'));
delete map._doc;
const sub = map._sub || {};
delete map._sub;
const raw = map._raw || {}; // whole-file replacements, for text that only exists inside scripts
delete map._raw;
const used = new Set();
const tr = (s) => {
  const t = s.trim();
  if (Object.hasOwn(map, t)) { used.add(t); return s.replace(t, () => map[t]); }
  for (const k in sub) if (s.includes(k)) { used.add(k); s = s.split(k).join(sub[k]); }
  return s;
};
let h = fs.readFileSync(src, 'utf8').replace('<html lang="fr"', '<html lang="en"');
// Only the markup: leave <style> and <script> bodies alone.
h = h.replace(/(<(style|script)[\s\S]*?<\/\2>)|>([^<>]*[^<>\s][^<>]*)</g, (m, block, _t, text) => (block ? m : '>' + tr(text) + '<'));
h = h.replace(/\b(title|aria-label|placeholder|alt)="([^"]+)"/g, (m, a, v) => `${a}="${tr(v)}"`);
for (const k in raw) { if (h.includes(k)) used.add(k); h = h.split(k).join(raw[k]); }
fs.writeFileSync(out, h);
const unused = Object.keys({ ...map, ...sub, ...raw }).filter((k) => !used.has(k));
console.error(`replaced ${used.size} strings; ${unused.length} map keys never matched`);
if (process.env.SHOW_UNUSED) console.error(unused.join('\n'));
