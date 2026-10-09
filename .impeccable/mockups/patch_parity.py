import re, pathlib
base = pathlib.Path(__file__).parent
p = base / 'graphite.src.html'
s = p.read_text(encoding='utf8')

# CSS
s = s.replace('</style>', (base / 'parity.css').read_text(encoding='utf8') + '</style>', 1)

# icons
icons = '''<symbol id="i-branch" viewBox="0 0 24 24"><circle cx="6" cy="5" r="2"/><circle cx="6" cy="19" r="2"/><circle cx="18" cy="7" r="2"/><path d="M6 7v10M18 9c0 5-7 3-11.5 8.5"/></symbol>
  <symbol id="i-shield" viewBox="0 0 24 24"><path d="M12 3 4 6v6c0 4.5 3.4 8.4 8 9 4.6-.6 8-4.5 8-9V6Z"/></symbol>
  <symbol id="i-cpu" viewBox="0 0 24 24"><rect x="6" y="6" width="12" height="12" rx="2"/><path d="M9 2v4M15 2v4M9 18v4M15 18v4M2 9h4M2 15h4M18 9h4M18 15h4"/></symbol>
  <symbol id="i-zap" viewBox="0 0 24 24"><path d="M13 2 4 14h7l-1 8 9-12h-7Z"/></symbol>
  <symbol id="i-agents" viewBox="0 0 24 24"><circle cx="12" cy="5" r="2.5"/><circle cx="5" cy="18" r="2.5"/><circle cx="19" cy="18" r="2.5"/><path d="M12 7.5v4M12 11.5 6.5 16M12 11.5l5.5 4.5"/></symbol>
  <symbol id="i-compress" viewBox="0 0 24 24"><path d="M4 14h6v6M20 10h-6V4M14 10l7-7M3 21l7-7"/></symbol>
  <symbol id="i-plug" viewBox="0 0 24 24"><path d="M9 2v6M15 2v6M6 8h12v3a6 6 0 0 1-12 0Z M12 17v5"/></symbol>
  <symbol id="i-check"'''
s = s.replace('<symbol id="i-check"', icons, 1)

# nav + intro
s = s.replace('<a href="#s7">7 · Apparence</a></nav>',
              '<a href="#s7">7 · Apparence</a><a href="#s8">8 · Worktrees</a><a href="#s9">9 · Composer</a><a href="#s10">10 · Ultracode</a><a href="#s11">11 · Extensions</a></nav>')
s = s.replace('Sept écrans de la même console', 'Onze écrans de la même console')
s = s.replace("lire une réponse riche, et choisir son thème.",
              "lire une réponse riche, choisir son thème, faire le ménage dans les worktrees, régler chaque tour, suivre un workflow ultracode et gérer les extensions.")

# screen 1: worktree option
s = s.replace('''        <div class="field">
          <label>Permissions</label>''', '''        <div class="field">
          <label>Isolation</label>
          <div class="input" style="font-family:var(--sans)"><span class="toggle on"><span class="sw"></span>Nouveau worktree</span><span class="branch" style="margin-left:14px"><svg class="i"><use href="#i-branch"/></svg>claude/session-store depuis main</span><span class="b btn ghost">Modifier</span></div>
        </div>
        <div class="field">
          <label>Permissions</label>''', 1)

# syntax highlighting for hand-made code views (inspector file view + permission diff)
s = s.replace('<div class="filev">', '<div class="filev" data-hl="csharp">', 1)
s = s.replace('<div class="diff">\n          <div class="fh"><span>Components/Pages/Home.razor</span>',
              '<div class="diff" data-hl="csharp">\n          <div class="fh"><span>Components/Pages/Home.razor</span>', 1)
def wrap_filev(m):
    return f'{m.group(1)}<code class="lc">{m.group(2)}</code></div>'
s = re.sub(r'(<div(?: class="hl")?><span class="n">\d+</span>)([^<\n]*)</div>', wrap_filev, s)
s = re.sub(r'(<div class="l(?: add| del)?"><span class="n">\d+</span><span(?: class="g")?>[^<]*</span>)([^<\n]*)</div>', wrap_filev, s)

# screens
s = s.replace('\n</div>\n<script src="../../wwwroot', '\n' + (base / 'screens_8_11.html').read_text(encoding='utf8') + '\n</div>\n<script src="../../wwwroot', 1)
p.write_text(s, encoding='utf8')
print('lc:', s.count('class="lc"'), 'screens:', s.count('<section class="screen"'))
