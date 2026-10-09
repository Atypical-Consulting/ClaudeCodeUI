import re, pathlib
base = pathlib.Path(__file__).parent
p = base / 'graphite.src.html'
s = p.read_text(encoding='utf8')

# 1. screen 2 prose -> Markdig output
s = re.sub(r'<div class="prose">\s*<p>La session vit entièrement.*?</ol>\s*</div>',
           '<div class="prose md">{{MD:running}}</div>', s, count=1, flags=re.S)
s = s.replace('.k{color:var(--syn-kw)}.ty{color:var(--syn-type)}.st{color:var(--ok)}.fn{color:var(--syn-fn)}.cm{color:var(--syn-com);font-style:italic}.nu{color:var(--syn-num)}\n', '')

css = (base / 'md_themes.css').read_text(encoding='utf8')
s = s.replace('</style>', css + '</style>', 1)

themes = [('graphite', 'Graphite'), ('encre', 'Encre'), ('ristretto', 'Ristretto'), ('mousse', 'Mousse'), ('contraste', 'Contraste élevé')]
bar = ('<div class="themebar" role="group" aria-label="Thème"><span class="lab">Thème</span>'
       + ''.join(f'<button type="button" data-set-theme="{k}" aria-pressed="false"><i data-theme="{k}"></i>{n}</button>' for k, n in themes)
       + '</div>\n  ')
s = s.replace('<nav><a href="#s1">', bar + '<nav><a href="#s1">', 1)
s = s.replace('<a href="#s5">5 · Sélecteur et crash</a></nav>',
              '<a href="#s5">5 · Sélecteur et crash</a><a href="#s6">6 · Markdown</a><a href="#s7">7 · Apparence</a></nav>')
s = s.replace('Cinq écrans de la même console', 'Sept écrans de la même console')
s = s.replace("et passer de l'une à l'autre au clavier.", "passer de l'une à l'autre au clavier, lire une réponse riche, et choisir son thème.")

screens = (base / 'screens_6_7.html').read_text(encoding='utf8')
s = s.replace('\n</div>\n<script>const m=', '\n' + screens + '\n</div>\n<script>const m=', 1)

s = s.replace('<div class="pfoot">',
              '<div class="pi"><span class="ic"><svg class="i sm"><use href="#i-panel"/></svg></span><div><div class="n">Thème : Graphite</div></div><span class="r">changer…</span></div>\n    <div class="pfoot">', 1)

script = (base / 'enhance.js').read_text(encoding='utf8')
s = s.replace('<script>const m=', '<script src="../../wwwroot/lib/highlight/highlight.min.js"></script>\n<script>\n' + script + '\n</script>\n<script>const m=', 1)
s = s.replace('<symbol id="i-check"',
              '<symbol id="i-copy" viewBox="0 0 24 24"><rect x="9" y="9" width="12" height="12" rx="2"/><path d="M5 15H4a1 1 0 0 1-1-1V4a1 1 0 0 1 1-1h10a1 1 0 0 1 1 1v1"/></symbol>\n  <symbol id="i-check"', 1)
p.write_text(s, encoding='utf8')
print(s.count('{{MD:'), s.count('{{THEMECARDS}}'), s.count('<section class="screen"'))
