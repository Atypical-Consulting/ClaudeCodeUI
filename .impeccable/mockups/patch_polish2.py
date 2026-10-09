import re, pathlib
base = pathlib.Path(__file__).parent
p = base / 'graphite.src.html'
s = p.read_text(encoding='utf8')
n0 = len(s)

def rep(old, new, count=1, must=True):
    global s
    if must and old not in s:
        raise SystemExit('missing: ' + old[:80])
    s = s.replace(old, new, count)

# 1. one session identity everywhere: branch · repo · mode
rep('<span class="ttl">session-store</span><span class="sub">C:\\repo\\POC\\ClaudeCodeUI · default</span>',
    '<span class="ttl">session-store</span><span class="sub">feat/session-store · ClaudeCodeUI · default</span>', count=-1)
rep('<span class="ttl">session-store</span><span class="sub">feat/session-store · default</span>',
    '<span class="ttl">session-store</span><span class="sub">feat/session-store · ClaudeCodeUI · default</span>')
rep('<span class="ttl">review-session-store</span><span class="sub">feat/session-store · auto</span>',
    '<span class="ttl">review-session-store</span><span class="sub">feat/session-store · ClaudeCodeUI · auto</span>')
rep('<span class="ttl">migrate-net10</span><span class="sub">~/repo/legacy · auto</span>',
    '<span class="ttl">migrate-net10</span><span class="sub">chore/net10 · legacy · auto</span>')
br = lambda b: f'<div class="branch"><svg class="i"><use href="#i-branch"/></svg>{b}</div>'
for name, path, b in [('session-store', '~/repo/POC/ClaudeCodeUI', 'feat/session-store · ClaudeCodeUI'),
                      ('fix-flaky-auth-test', '~/repo/api', 'fix/flaky-auth · api'),
                      ('bump-markdig', '~/repo/POC/ClaudeCodeUI', 'chore/bump-markdig · ClaudeCodeUI'),
                      ('docs-readme', '~/repo/site', 'main · site'),
                      ('migrate-net10', '~/repo/legacy', 'chore/net10 · legacy')]:
    rep(f'<td><div class="n">{name}</div><div class="d">{path}</div></td>', f'<td><div class="n">{name}</div>{br(b)}</td>')
rep('<div class="d">~/repo/api · attend · Bash</div>', '<div class="d">fix/flaky-auth · api · attend · Bash</div>')
rep('<div class="d">~/repo/api · hier · $0.66</div>', '<div class="d">feat/auth-jwt · api · hier · $0.66</div>')
rep('claude/session-store depuis main', 'claude/persistance-sessions depuis main')

# 2. exact CLI vocabulary
rep('<span>low</span><span>med</span><span class="on">high</span>', '<span>low</span><span>medium</span><span class="on">high</span>')

# 3. palette theme icon + theme-card code that fits
rep('<symbol id="i-check"', '<symbol id="i-palette" viewBox="0 0 24 24"><path d="M12 3a9 9 0 1 0 0 18c1.1 0 1.6-.8 1.6-1.6 0-.9-.7-1.3-.7-2.2 0-.9.7-1.6 1.6-1.6H17a4 4 0 0 0 4-4C21 6.6 17 3 12 3Z"/><circle cx="7.5" cy="11" r="1"/><circle cx="10" cy="7" r="1"/><circle cx="15" cy="7" r="1"/></symbol>\n  <symbol id="i-check"')
rep('<svg class="i sm"><use href="#i-panel"/></svg></span><div><div class="n">Thème : Graphite',
    '<svg class="i sm"><use href="#i-palette"/></svg></span><div><div class="n">Thème : Graphite')

# 4. accessible switches
s = re.sub(r'<span class="toggle( on)?"><span class="sw"></span></span>',
           lambda m: f'<span class="toggle{m.group(1) or ""}" role="switch" aria-checked="{"true" if m.group(1) else "false"}" aria-label="Activer pour cette session"><span class="sw"></span></span>', s)
s = re.sub(r'<span class="toggle( on)?"><span class="sw"></span>([^<]+)</span>',
           lambda m: f'<span class="toggle{m.group(1) or ""}" role="switch" aria-checked="{"true" if m.group(1) else "false"}"><span class="sw"></span>{m.group(2)}</span>', s)

# 5. dead styles
for dead in ['.btn.lg{padding:10px 16px;font-size:14px}\n', '.t.live .m{color:var(--ok)}\n', '.bar.warn i{background:var(--syn-type)}\n', '.toggle.off{opacity:.45}\n']:
    rep(dead, '', must=False)
s = re.sub(r'\.out\{[^}]*\}\n\.out \.ok\{[^}]*\}\n', '', s)

# 6. states + rail details
css = '''
/* polish 2: states for parity components, rail details */
.pick,.effort span,.filters .chip,.subtabs span,.agent,.slash .si,.wtt tbody tr:not(.grp-row),.mcp tbody tr,.tc,.toggle{transition:background-color .15s cubic-bezier(.2,.8,.2,1),border-color .15s,color .15s}
.pick:not(.dis):not(.on):hover,.filters .chip:not(.on):hover{border-color:var(--seam-3);color:var(--fg)}
.pick.on:hover{background:color-mix(in srgb,var(--accent) 18%,transparent)}
.pick.dis{cursor:not-allowed}
.effort span:not(.on):hover,.subtabs span:not(.on):hover{color:var(--fg-2)}
.agent:not(.sel):hover,.slash .si:not(.on):hover,.wtt tbody tr:not(.grp-row):not(.sel):hover td,.mcp tbody tr:not(.sel):hover td,table tbody tr:not(.hot):hover td{background:var(--raise)}
.toggle{cursor:pointer}
.toggle:hover .sw{box-shadow:0 0 0 3px var(--accent-soft)}
.branch{display:block}
.branch .i{display:inline-block;vertical-align:-1px;margin-right:4px}
.quota .qh{display:flex;justify-content:space-between;align-items:baseline;margin-bottom:1px}
.quota .qh span:last-child{font:11px var(--mono);color:var(--fg-5)}
'''
rep('</style>', css + '</style>')
p.write_text(s, encoding='utf8')

b = base / 'build.js'
t = b.read_text(encoding='utf8')
t = t.replace('<div class="quota" title="rate_limit_event"><div class="qrow">',
              '<div class="quota" title="rate_limit_event"><div class="qh"><span class="cap">Utilisation</span><span>réinit. 18:00</span></div><div class="qrow">')
b.write_text(t, encoding='utf8')
print('ok', len(s) - n0)
