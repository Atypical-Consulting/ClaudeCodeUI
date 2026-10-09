// Theme, code size, copy buttons, syntax highlighting and global shortcuts. Blazor owns the markup:
// never replace a node it rendered (the .cb wrapping is done server-side by Md.cs).
(() => {
    const THEMES = ['graphite', 'encre', 'ristretto', 'mousse', 'contraste'], THEME_KEY = 'claude-ui.theme', SIZE_KEY = 'claude-ui.code-size';
    const root = document.documentElement;
    let net = null, queued = false, opener = null;

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
        // The command palette is modal: the app behind it is inert while it is open, and on close focus goes back
        // to what had it (unless something, e.g. FocusOnNavigate, already took it).
        modal(open, el) {
            if (open) opener = document.activeElement;
            const app = document.querySelector('.app');
            if (app) app.inert = open;
            if (open) el?.focus();
            else if (opener?.isConnected && (!document.activeElement || document.activeElement === document.body)) opener.focus();
        },
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

    // Tabs and radio groups: arrows / Home / End move focus and selection together (roving tabindex in the markup).
    document.addEventListener('keydown', e => {
        const item = e.target instanceof Element ? e.target.closest('[role=tab],[role=radio]') : null;
        const group = item?.closest('[role=tablist],[role=radiogroup]');
        if (!group || e.ctrlKey || e.metaKey || e.altKey) return;
        const items = [...group.querySelectorAll('[role=tab],[role=radio]')].filter(x => !x.disabled);
        const i = items.indexOf(item), n = items.length;
        const j = { ArrowRight: i + 1, ArrowDown: i + 1, ArrowLeft: i - 1, ArrowUp: i - 1, Home: 0, End: n - 1 }[e.key];
        if (j === undefined) return;
        e.preventDefault();
        const next = items[(j + n) % n];
        next.focus();
        next.click();
    });

    // Space on a focusable table row selects it (Blazor handler) without also scrolling its scroller.
    document.addEventListener('keydown', e => {
        if (e.key === ' ' && e.target instanceof Element && e.target.matches('tr[tabindex]')) e.preventDefault();
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
