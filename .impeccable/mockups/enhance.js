// Same enhancer the app would run from its MutationObserver: highlight, wrap in a titled block, localize alerts.
const LANG = { csharp: 'C#', cs: 'C#', bash: 'Bash', sh: 'Shell', shell: 'Shell', json: 'JSON', diff: 'Diff', javascript: 'JavaScript', js: 'JavaScript', typescript: 'TypeScript', ts: 'TypeScript', xml: 'XML', html: 'HTML', css: 'CSS', sql: 'SQL', yaml: 'YAML', python: 'Python' };
const ALERT = { Note: 'Note', Tip: 'Astuce', Important: 'Important', Warning: 'Attention', Caution: 'Prudence' };
function enhance(root) {
  root.querySelectorAll('pre>code:not([data-done])').forEach(code => {
    code.dataset.done = 1;
    const lang = (code.className.match(/language-(\S+)/) || [])[1] || '';
    if (window.hljs && (!lang || hljs.getLanguage(lang))) hljs.highlightElement(code);
    const pre = code.parentElement, box = document.createElement('div');
    box.className = 'cb';
    box.innerHTML = `<div class="h"><span>${LANG[lang] || lang || 'texte'}</span><button class="copy" type="button"><svg class="i sm"><use href="#i-copy"/></svg><span>Copier</span></button></div>`;
    pre.replaceWith(box); box.appendChild(pre);
    box.querySelector('.copy').onclick = e => {
      const b = e.currentTarget, label = b.lastChild;
      navigator.clipboard?.writeText(code.innerText);
      b.classList.add('done'); label.textContent = 'Copié';
      setTimeout(() => { b.classList.remove('done'); label.textContent = 'Copier'; }, 1400);
    };
  });
  // Hand-built code views (inspector file viewer, permission diff): highlight each line in place,
  // keeping line numbers and +/- gutters outside the highlighted text.
  root.querySelectorAll('[data-hl]').forEach(view => {
    const language = view.dataset.hl;
    if (!window.hljs || !hljs.getLanguage(language)) return;
    view.querySelectorAll('code.lc:not([data-done])').forEach(c => {
      c.dataset.done = 1;
      c.innerHTML = hljs.highlight(c.textContent, { language, ignoreIllegals: true }).value;
    });
  });
  root.querySelectorAll('.markdown-alert-title').forEach(t => {
    const n = t.lastChild, k = n && n.textContent.trim();
    if (ALERT[k]) n.textContent = ALERT[k];
  });
}
enhance(document);

// Theme system: one attribute on <html>, remembered in localStorage.
const THEME_KEY = 'claude-ui.theme', SIZE_KEY = 'claude-ui.code-size';
const THEME_NAMES = { graphite: 'Graphite', encre: 'Encre', ristretto: 'Ristretto', mousse: 'Mousse', contraste: 'Contraste élevé' };
function setTheme(t) {
  if (!THEME_NAMES[t]) t = 'graphite';
  document.documentElement.dataset.theme = t;
  localStorage.setItem(THEME_KEY, t);
  document.querySelectorAll('[data-set-theme]').forEach(b => {
    const on = b.dataset.setTheme === t;
    b.setAttribute('aria-pressed', on); b.classList.toggle('on', on);
  });
  document.querySelectorAll('.pi .n').forEach(n => { if (n.textContent.startsWith('Thème :')) n.textContent = 'Thème : ' + THEME_NAMES[t]; });
}
function setCodeSize(v) {
  document.documentElement.style.setProperty('--code-size', v);
  localStorage.setItem(SIZE_KEY, v);
  document.querySelectorAll('[data-code-size]').forEach(x => x.classList.toggle('on', x.dataset.codeSize === v));
}
document.addEventListener('click', e => {
  const b = e.target.closest('[data-set-theme]'); if (b) setTheme(b.dataset.setTheme);
  const z = e.target.closest('[data-code-size]'); if (z) setCodeSize(z.dataset.codeSize);
});
setTheme(new URLSearchParams(location.search).get('theme') || localStorage.getItem(THEME_KEY) || 'graphite');
setCodeSize(localStorage.getItem(SIZE_KEY) || '13px');
