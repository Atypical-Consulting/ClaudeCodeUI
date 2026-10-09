// Theme, code size, copy buttons, syntax highlighting and global shortcuts. Blazor owns the markup:
// never replace a node it rendered (the .cb wrapping is done server-side by Md.cs).
(() => {
    const THEMES = ['graphite', 'encre', 'ristretto', 'mousse', 'contraste'], THEME_KEY = 'claude-ui.theme', SIZE_KEY = 'claude-ui.code-size';
    const root = document.documentElement;
    let net = null, queued = false;

    window.claudeUi = {
        getTheme: () => root.dataset.theme || 'graphite',
        setTheme(t) {
            if (!THEMES.includes(t)) t = 'graphite';
            root.dataset.theme = t;
            localStorage.setItem(THEME_KEY, t);
        },
        getCodeSize: () => localStorage.getItem(SIZE_KEY) || '13px',
        setCodeSize(v) {
            root.style.setProperty('--code-size', v);
            localStorage.setItem(SIZE_KEY, v);
        },
        copy: text => navigator.clipboard?.writeText(text),
        registerShortcuts(ref) { net = ref; },
        focus(el) { el?.focus(); },
        reveal(el, id) { el?.querySelector('#' + id)?.scrollIntoView({ block: 'nearest' }); },
        // Pin a scroller (or the .thread holding el) to its bottom after each render, unless the user scrolled up.
        scrollEnd(el) {
            el = el?.closest('.thread') || el;
            if (!el) return;
            if (!el.onscroll) el.onscroll = () => el._free = el.scrollHeight - el.scrollTop - el.clientHeight > 40;
            if (!el._free) el.scrollTop = el.scrollHeight;
        },
    };

    // Highlighting only looks at the nodes each mutation added, never at the whole document.
    // Markdown code blocks are highlighted when first visible, so remounting a long thread does not redo them all.
    const added = new Set(), views = new Set();
    const visible = new IntersectionObserver(entries => entries.forEach(e => {
        if (!e.isIntersecting) return;
        visible.unobserve(e.target);
        paintCode(e.target);
    }));

    function paintCode(code) {
        if (!window.hljs || code.classList.contains('hljs')) return;
        const lang = (code.className.match(/language-(\S+)/) || [])[1];
        if (!lang || hljs.getLanguage(lang)) hljs.highlightElement(code);
        else code.classList.add('hljs');
    }

    // Hand-built code views (file viewer, diff), one code.lc per line, gutters outside: each side (new = context + added,
    // old = context + removed) is highlighted as ONE text, then split back per line, closing and reopening the spans
    // open at each \n. One hljs call per view, and block comments or multi-line strings keep their colour.
    function paintView(view) {
        const language = view.dataset.hl;
        if (!window.hljs || !hljs.getLanguage(language)) return;
        const lines = [...view.querySelectorAll('code.lc:not([data-done])')];
        const side = c => c.parentElement.classList.contains('del') ? -1 : c.parentElement.classList.contains('add') ? 1 : 0;
        const dels = lines.filter(c => side(c) < 0);
        paintLines(language, lines.filter(c => side(c) >= 0), null);
        if (dels.length) paintLines(language, lines.filter(c => side(c) <= 0), new Set(dels));
    }

    function paintLines(language, src, only) {
        if (!src.length) return;
        const html = hljs.highlight(src.map(c => c.textContent).join('\n'), { language, ignoreIllegals: true }).value;
        const out = [], open = [];
        let cur = '';
        for (const [t] of html.matchAll(/<span[^>]*>|<\/span>|\n|[^<\n]+/g)) {
            if (t === '\n') { out.push(cur + '</span>'.repeat(open.length)); cur = open.join(''); continue; }
            cur += t;
            if (t === '</span>') open.pop(); else if (t.startsWith('<span')) open.push(t);
        }
        out.push(cur);
        src.forEach((c, i) => {
            if (only && !only.has(c)) return;
            c.dataset.done = 1;
            c.innerHTML = out[i] ?? '';
        });
    }

    function highlight() {
        queued = false;
        for (const n of added) {
            if (!n.isConnected) continue;
            (n.matches('pre>code') ? [n] : n.querySelectorAll('pre>code:not(.hljs)')).forEach(c => visible.observe(c));
            const view = n.closest('[data-hl]');
            if (view) views.add(view); else n.querySelectorAll('[data-hl]').forEach(v => views.add(v));
        }
        added.clear();
        views.forEach(paintView);
        views.clear();
    }
    new MutationObserver(records => {
        for (const r of records) {
            r.addedNodes.forEach(n => { if (n.nodeType === 1) added.add(n); });
            r.removedNodes.forEach(n => { if (n.nodeType === 1) (n.matches('pre>code') ? [n] : n.querySelectorAll('pre>code:not(.hljs)')).forEach(c => visible.unobserve(c)); });
        }
        if (added.size && !queued) { queued = true; requestAnimationFrame(highlight); }
    }).observe(document.body, { childList: true, subtree: true });
    added.add(document.body);
    highlight();

    // Copy buttons: code blocks and [data-copy]. The label's text node is edited in place, not replaced.
    document.addEventListener('click', e => {
        const b = e.target.closest('.cb .copy, [data-copy]');
        if (!b) return;
        const text = b.hasAttribute('data-copy') ? b.dataset.copy : b.closest('.cb').querySelector('code').innerText;
        navigator.clipboard?.writeText(text);
        const label = [...(b.matches('.cb .copy') ? b.lastElementChild : b).childNodes].reverse().find(n => n.nodeType === 3 && n.nodeValue.trim());
        if (!label || b.classList.contains('done')) return;
        const old = label.nodeValue;
        label.nodeValue = root.lang === 'fr' ? 'Copié' : 'Copied';   // <html lang> follows the UI culture
        b.classList.add('done');
        setTimeout(() => { label.nodeValue = old; b.classList.remove('done'); }, 1400);
    });

    const isField = el => el instanceof Element && el.closest('input,textarea,select,button,a[href],[role=button],[role=option],[contenteditable]:not([contenteditable=false])');

    // Global shortcuts, forwarded to MainLayout.OnShortcut. Alt N doubles Ctrl N (the browser keeps Ctrl N).
    document.addEventListener('keydown', e => {
        if (!net || e.isComposing) return;
        const letter = e.code.startsWith('Key') ? e.code.slice(3) : null;
        const ctrl = e.ctrlKey || e.metaKey;
        let key = null;
        if (ctrl && !e.altKey && e.shiftKey && (letter === 'A' || letter === 'O')) key = 'Ctrl+Shift+' + letter;
        else if (ctrl && !e.altKey && !e.shiftKey && (letter === 'K' || letter === 'I' || letter === 'N')) key = 'Ctrl+' + letter;
        else if (e.altKey && !ctrl && !e.shiftKey && letter === 'N') key = 'Ctrl+N';
        else if (e.key === 'Escape' && !(e.target instanceof Element && e.target.closest('.composer:has(.pop)'))) key = 'Escape';   // the Composer closes its popover
        else if (!ctrl && !e.altKey && (e.key === 'Enter' || e.key === 'Delete') && !isField(e.target) && document.querySelector('[data-permission]'))
            key = e.key === 'Enter' && e.shiftKey ? 'Shift+Enter' : e.key;
        if (!key) return;
        if (key !== 'Escape') e.preventDefault();
        net.invokeMethodAsync('OnShortcut', key).catch(() => { });
    });
})();
