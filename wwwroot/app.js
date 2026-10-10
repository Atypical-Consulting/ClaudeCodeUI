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
        // Composer images: files pasted into or dropped on the box go to its hidden <InputFile>, which streams them to the
        // server in chunks, never as one hub message; the paperclip button opens that input's file picker.
        images(box, input, button) {
            if (!box || !input || box._images) return;
            box._images = true;
            const give = files => {
                if (!files?.length) return false;
                const dt = new DataTransfer();
                [...files].forEach(f => dt.items.add(f));
                input.files = dt.files;
                input.dispatchEvent(new Event('change', { bubbles: true }));
                return true;
            };
            const dragging = e => e.dataTransfer?.types.includes('Files');
            button?.addEventListener('click', () => input.click());
            box.addEventListener('paste', e => { if (give(e.clipboardData?.files)) e.preventDefault(); });
            box.addEventListener('dragover', e => { if (dragging(e)) { e.preventDefault(); box.dataset.drop = ''; } });
            box.addEventListener('dragleave', e => { if (!box.contains(e.relatedTarget)) delete box.dataset.drop; });
            box.addEventListener('drop', e => {
                delete box.dataset.drop;
                if (dragging(e)) { e.preventDefault(); give(e.dataTransfer.files); }
            });
        },
        scrollToEnd(el) { if (el) el.scrollLeft = el.scrollWidth; },
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
        focusIfIdle(el) { const a = document.activeElement; if (!a || a === document.body) el?.focus(); },
        reveal(el, id) { el?.querySelector('#' + id)?.scrollIntoView({ block: 'nearest' }); },
        // The command palette is modal: the app behind it is inert while it is open, and on close focus goes back
        // to what had it (unless something, e.g. FocusOnNavigate, already took it). Its input is its only focusable
        // element and owns the keys: Tab keeps focus there, and a click on an option does not take it away.
        modal(open, el) {
            if (open) opener = document.activeElement;
            const app = document.querySelector('.app');
            if (app) app.inert = open;
            const dlg = el?.closest('[role=dialog]');
            if (open && dlg) {
                dlg.onkeydown = e => { if (e.key === 'Tab') { e.preventDefault(); el.focus(); } };
                dlg.onfocusout = e => { if (!dlg.contains(e.relatedTarget)) queueMicrotask(() => el.isConnected && el.focus()); };
            }
            if (open) el?.focus();
            else if (opener?.isConnected && (!document.activeElement || document.activeElement === document.body)) opener.focus();
        },
        // Pin a scroller (or the .thread holding el) to its bottom after each render, unless the user scrolled up (force: pin anyway);
        // then new output reveals the sibling .dock .jump button, which scrolls back down and pins again.
        scrollEnd(el, force) {
            el = el?.closest('.thread') || el;
            if (!el) return;
            const jump = el.nextElementSibling?.querySelector('.jump');
            if (!el.onscroll) {
                el.onscroll = () => {
                    el._free = el.scrollHeight - el.scrollTop - el.clientHeight > 40;
                    if (!el._free && jump) jump.hidden = true;
                };
                if (jump) jump.onclick = () => { el._free = false; el.scrollTop = el.scrollHeight; jump.hidden = true; };
            }
            if (force) { el._free = false; if (jump) jump.hidden = true; }
            if (!el._free) el.scrollTop = el.scrollHeight;
            else if (jump && el._h !== undefined && el.scrollHeight > el._h) jump.hidden = false;
            el._h = el.scrollHeight;
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
    // open at each \n. One hljs call per hunk (a diff's .hunk rows separate unrelated code), so block comments or
    // multi-line strings keep their colour without leaking into the next hunk.
    function paintView(view) {
        const language = view.dataset.hl;
        if (!window.hljs || !hljs.getLanguage(language)) return;
        const hunks = [[]];
        for (const row of view.children) {
            if (row.classList.contains('hunk')) { hunks.push([]); continue; }
            const c = row.querySelector('code.lc:not([data-done])');
            if (c) hunks[hunks.length - 1].push(c);
        }
        const side = c => c.parentElement.classList.contains('del') ? -1 : c.parentElement.classList.contains('add') ? 1 : 0;
        for (const lines of hunks) {
            const dels = lines.filter(c => side(c) < 0);
            paintLines(language, lines.filter(c => side(c) >= 0), null);
            if (dels.length) paintLines(language, lines.filter(c => side(c) <= 0), new Set(dels));
        }
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
    // The confirmation is also written to a shared visually hidden status node, so screen readers hear it.
    const copied = document.createElement('div');
    copied.className = 'sr-only';
    copied.setAttribute('role', 'status');
    document.body.append(copied);
    document.addEventListener('click', e => {
        const b = e.target.closest('.cb .copy, [data-copy]');
        if (!b) return;
        const text = b.hasAttribute('data-copy') ? b.dataset.copy : b.closest('.cb').querySelector('code').innerText;
        navigator.clipboard?.writeText(text);
        const label = [...(b.matches('.cb .copy') ? b.lastElementChild : b).childNodes].reverse().find(n => n.nodeType === 3 && n.nodeValue.trim());
        if (!label || b.classList.contains('done')) return;
        const old = label.nodeValue;
        const msg = label.nodeValue = root.lang === 'fr' ? 'Copié' : 'Copied';   // <html lang> follows the UI culture
        copied.textContent = '';   // cleared then set next frame: a second copy within 1400 ms is still a change, so it is announced
        requestAnimationFrame(() => copied.textContent = msg);
        b.classList.add('done');
        setTimeout(() => { label.nodeValue = old; copied.textContent = ''; b.classList.remove('done'); }, 1400);
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

    // ↑ ↓ over an open Composer popover move its selection (relayed by the server), not the caret of a multi-line prompt.
    document.addEventListener('keydown', e => {
        if ((e.key === 'ArrowUp' || e.key === 'ArrowDown') && !e.isComposing && e.target instanceof Element && e.target.matches('.composer textarea')
            && e.target.closest('.composer').querySelector('.pop [role=option]')) e.preventDefault();
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
