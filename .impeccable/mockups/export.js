// Builds the shareable mockup page: docs/mockups/index.html (full document) + an artifact fragment.
const fs = require('fs');
const [src, repoOut, fragOut] = process.argv.slice(2);
let h = fs.readFileSync(src, 'utf8');
h = h.replace('<script src="../../wwwroot/lib/highlight/highlight.min.js"></script>',
  '<script src="https://cdnjs.cloudflare.com/ajax/libs/highlight.js/11.9.0/highlight.min.js"></script>');
h = h.replace(/localStorage\.setItem\(([^;]+)\);/g, 'try { localStorage.setItem($1); } catch {}');
h = h.replace(/localStorage\.getItem\(([^)]+)\)/g, '(() => { try { return localStorage.getItem($1); } catch { return null; } })()');
h = h.replace('<title>Claude Code UI · Console graphite · mockups</title>', '<title>Claude Code UI</title>');
const extra = `
/* shareable page: dark by design, frames scale down to the viewport */
:root{color-scheme:dark}
body{background:var(--page)}
.doc{box-sizing:border-box;max-width:1472px;padding-inline:16px}
.screen>p,.doc>header p{max-width:80ch}
.doc>header h1{font-size:clamp(22px,5vw,30px);text-wrap:balance}
.screen>h2{text-wrap:balance}
.fitbox{overflow:hidden}
@media (prefers-reduced-motion:reduce){*{animation:none!important;transition:none!important}}
`;
h = h.replace('</style>', extra + '</style>');
const fit = `<script>
// Fit the 1440x900 frames into narrow viewports without horizontal page scroll.
function fitFrames(){const w=Math.min(document.documentElement.clientWidth,1472)-32;const k=Math.min(1,w/1440);
document.querySelectorAll('.frame').forEach(f=>{f.style.zoom=k;});}
fitFrames();addEventListener('resize',fitFrames);
</script>`;
h = h.replace('</body>', fit + '\n</body>');
h = h.replace(/data-theme/g, 'data-ui-theme').replace(/dataset.theme/g, 'dataset.uiTheme');
fs.writeFileSync(repoOut, h);
const frag = h.replace(/<!DOCTYPE html>\s*<html[^>]*>\s*<head>/i, '').replace(/<\/head>\s*<body>/i, '').replace(/<\/body>\s*<\/html>\s*$/i, '')
  .replace(/<meta charset="utf-8">\s*/i, '').replace(/<meta name="viewport"[^>]*>\s*/i, '');
if (fragOut) fs.writeFileSync(fragOut, frag);
console.log(h.length, frag.length);
