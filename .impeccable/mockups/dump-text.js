// Lists the visible French strings of the site screens, so en.json can be audited for completeness.
// usage: node dump-text.js [graphite.html]
const fs = require('fs');
const h = fs.readFileSync(process.argv[2] || 'graphite.html', 'utf8');
const seen = new Set();
for (const n of [3, 4, 6, 7, 8, 9, 10, 11]) {
  const a = h.indexOf(`<section class="screen" id="s${n}">`);
  const b = h.indexOf('</section>', a);
  const sec = h.slice(a, b).replace(/<svg[\s\S]*?<\/svg>/g, '<svg/>').replace(/<(script|style)[\s\S]*?<\/\1>/g, '');
  const out = [];
  for (const m of sec.matchAll(/>([^<>]*[^<>\s][^<>]*)</g)) out.push(m[1].trim());
  for (const m of sec.matchAll(/\b(?:title|aria-label|placeholder|alt)="([^"]+)"/g)) out.push('ATTR ' + m[1]);
  console.log('== s' + n);
  for (const t of out) if (!seen.has(t)) { seen.add(t); console.log(t); }
}
