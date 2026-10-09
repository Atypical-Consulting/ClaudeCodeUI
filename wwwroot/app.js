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
        // A new permission card: keys stay inert for 300 ms (a held or doubled key must not answer a card nobody has
        // read), then focus moves to the card unless the user is typing a draft or using a dialog.
        armPermission(el) {
            if (!el) return;
            el._armedAt = performance.now() + 300;
            const a = document.activeElement;
            const busy = a instanceof Element && (a.closest('[role=dialog],[contenteditable]:not([contenteditable=false])')
                || (a.closest('input,textarea,select') && a.value));
            if (!busy) el.focus({ preventScroll: true });
        },
        reveal(el, id) { el?.querySelector('#' + id)?.scrollIntoView({ block: 'nearest' }); },
        // Pin a scroller to its bottom after each render, unless the user scrolled up.
        scrollEnd(el) {
            if (!el) return;
            if (!el.onscroll) el.onscroll = () => el._free = el.scrollHeight - el.scrollTop - el.clientHeight > 40;
            if (!el._free) el.scrollTop = el.scrollHeight;
        },
    };

    function highlight() {
        queued = false;
        if (!window.hljs) return;
        document.querySelectorAll('pre>code:not(.hljs)').forEach(code => {
            const lang = (code.className.match(/language-(\S+)/) || [])[1];
            if (!lang || hljs.getLanguage(lang)) hljs.highlightElement(code);
            else code.classList.add('hljs');
        });
        // Hand-built code views (file viewer, diff): highlight each line in place, gutters stay outside.
        document.querySelectorAll('[data-hl]').forEach(view => {
            const language = view.dataset.hl;
            if (!hljs.getLanguage(language)) return;
            view.querySelectorAll('code.lc:not([data-done])').forEach(c => {
                c.dataset.done = 1;
                c.innerHTML = hljs.highlight(c.textContent, { language, ignoreIllegals: true }).value;
            });
        });
    }
    new MutationObserver(() => { if (!queued) { queued = true; requestAnimationFrame(highlight); } })
        .observe(document.body, { childList: true, subtree: true });
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
        else if (e.key === 'Escape') {
            // Esc interrupts the turn only from the composer, the palette or nothing focused; elsewhere it first
            // leaves the focused control (or the text selection), so a stray Esc never kills a turn.
            const t = e.target instanceof Element ? e.target : null, sel = getSelection();
            if (t?.closest('.composer:has(.pop)')) { }   // the Composer closes its popover
            else if (t && t !== document.body && !t.closest('.composer,.palette')) t.blur();
            else if (t === document.body && sel && !sel.isCollapsed) sel.removeAllRanges();
            else key = 'Escape';
        }
        else if (!ctrl && !e.altKey && (e.key === 'Enter' || e.key === 'Delete') && !e.repeat && !isField(e.target)) {
            // Answer only from inside the card (it takes focus when it arrives), and only once it is armed.
            const card = e.target instanceof Element && e.target.closest('[data-permission]');
            if (card && performance.now() >= card._armedAt) key = e.key === 'Enter' && e.shiftKey ? 'Shift+Enter' : e.key;
        }
        if (!key) return;
        if (key !== 'Escape') e.preventDefault();
        net.invokeMethodAsync('OnShortcut', key).catch(() => { });
    });
})();
