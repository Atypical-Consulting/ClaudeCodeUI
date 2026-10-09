// Claude Code UI · site: theme, ledger rows, streamed lede, download card (index), table of contents (docs).
(() => {
  const root = document.documentElement, $ = s => document.getElementById(s);
  const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
  root.classList.add('js');

  // Theme: same five dark themes as the app.
  const THEMES = ['graphite', 'encre', 'ristretto', 'mousse', 'contraste'], KEY = 'ccui-site.theme';
  const setTheme = (t, save) => {
    if (!THEMES.includes(t)) t = 'graphite';
    root.dataset.theme = t;
    if (save) try { localStorage.setItem(KEY, t); } catch { }
    document.querySelectorAll('.th').forEach(b => b.setAttribute('aria-pressed', b.dataset.t === t));
  };
  setTheme(root.dataset.theme);
  document.querySelectorAll('.th').forEach(b => b.addEventListener('click', () => setTheme(b.dataset.t, true)));

  // Duck mood + what it says.
  const duck = document.querySelector('.duck');
  const mood = (m, say, sub) => {
    if (duck) duck.dataset.mood = m;
    if (say != null && $('duck-say')) $('duck-say').textContent = say;
    if (sub != null && $('duck-sub')) $('duck-sub').textContent = sub;
  };

  // Ledger rows: a button toggles its output (Enter / Space are native).
  document.querySelectorAll('.row').forEach(row => {
    const b = row.querySelector('.t'), out = row.querySelector('.out');
    if (!b || !out) return;
    out.inert = true;
    b.addEventListener('click', () => {
      const open = b.getAttribute('aria-expanded') !== 'true';
      b.setAttribute('aria-expanded', open);
      row.classList.toggle('open', open);
      out.inert = !open;
    });
    if (location.hash === '#' + out.id) b.click();   // deep link to one row
  });

  // Copy buttons on command blocks.
  document.querySelectorAll('.copyable, .cb').forEach(box => {
    const pre = box.querySelector('pre');
    if (!pre || !navigator.clipboard) return;
    const btn = box.querySelector('.copy') || box.appendChild(Object.assign(document.createElement('button'), { className: 'copy', type: 'button', textContent: 'Copier' }));
    btn.addEventListener('click', () => {
      const text = [...pre.childNodes].filter(n => !(n.classList && n.classList.contains('c'))).map(n => n.textContent).join('').replace(/^\s*\n/gm, '').trim();
      navigator.clipboard.writeText(text).then(() => {
        btn.textContent = 'Copié';
        setTimeout(() => btn.textContent = 'Copier', 1400);
      }, () => { });
    });
  });

  // The first assistant block streams in once, with the caret.
  const stream = document.querySelector('[data-stream]');
  if (stream && !calm) {
    const words = stream.textContent.trim().split(' ');
    const caret = Object.assign(document.createElement('span'), { className: 'caret' });
    stream.textContent = '';
    stream.after(caret);
    let i = 0;
    const tick = () => {
      stream.textContent += (i ? ' ' : '') + words[i++];
      if (i < words.length) setTimeout(tick, 28 + Math.random() * 30);
      else setTimeout(() => caret.remove(), 1600);
    };
    setTimeout(tick, 350);
  }

  // Docs: table of contents with scroll-spy, and its toggle on small screens.
  const toc = document.querySelector('.toc');
  if (toc) {
    const links = [...toc.querySelectorAll('a[href^="#"]')];
    const byId = new Map(links.map(a => [a.hash.slice(1), a]));
    const btn = document.querySelector('.toc-btn');
    btn?.addEventListener('click', () => {
      const open = btn.getAttribute('aria-expanded') !== 'true';
      btn.setAttribute('aria-expanded', open);
      toc.classList.toggle('open', open);
    });
    toc.addEventListener('click', e => {
      if (e.target.closest('a') && btn?.getAttribute('aria-expanded') === 'true') btn.click();
    });
    // Right column: the app screen that goes with the section being read.
    const SHOTS = {
      installation: ['04', 'Vue d\'ensemble des sessions', 'Vue d\'ensemble : cinq sessions avec leur état, leur mode, leur dernière action, et la file des décisions en attente.'],
      sessions: ['03', 'Demande de permission', 'Demande de permission : modification de Home.razor avec le diff, et les boutons Autoriser, Toute la session, Refuser.'],
      composer: ['09', 'Composer', 'Composer : sélecteur de modèle, effort, Rapide, Ultracode, et le menu des commandes slash.'],
      'sous-agents': ['10', 'Ultracode et sous-agents', 'Workflow ultracode : phases Revue, Vérification, Synthèse, six agents et le détail de l\'agent sélectionné.'],
      worktrees: ['08', 'Worktrees', 'Worktrees : tableau classé par état avec la raison, la taille, et le plan de nettoyage dans l\'inspecteur.'],
      extensions: ['11', 'Extensions', 'Extensions : serveurs MCP avec leur état, onglets Skills, Agents, Plugins.'],
      themes: ['07', 'Apparence', 'Apparence : les cinq thèmes sombres et le réglage de la taille du code.'],
      raccourcis: ['03', 'Demande de permission', 'Demande de permission : Autoriser ⏎, Toute la session Maj ⏎, Refuser Suppr.'],
      securite: ['06', 'Rendu Markdown', 'Rendu Markdown d\'une réponse : titres, tableau, liste de tâches et bloc de code coloré.'],
    };
    const shot = id => {
      const v = SHOTS[id], img = $('shot-img');
      if (!v || !img) return;
      img.src = $('shot-a').href = `assets/screens/${v[0]}.png`;
      img.alt = v[2];
      $('shot-cap').textContent = v[1];
      $('shot-sub').textContent = byId.get(id).textContent.toLowerCase();
    };
    const mark = id => { shot(id); links.forEach(a => a.hash === '#' + id ? a.setAttribute('aria-current', 'location') : a.removeAttribute('aria-current')); };
    const seen = new Set();
    const io = new IntersectionObserver(entries => {
      entries.forEach(e => e.isIntersecting ? seen.add(e.target.id) : seen.delete(e.target.id));
      const first = [...byId.keys()].find(id => seen.has(id));
      if (first) mark(first);
    }, { rootMargin: '-10% 0px -70% 0px' });
    byId.forEach((_, id) => { const s = $(id); if (s) io.observe(s); });
  }

  // Index: the permission card does the download.
  if (!$('dl')) return;
  const REPO = 'https://github.com/Atypical-Consulting/ClaudeCodeUI', RELEASES = REPO + '/releases';
  const OS = { win: 'Windows', mac: 'macOS', linux: 'Linux' };
  const size = n => new Intl.NumberFormat('fr-FR', { maximumFractionDigits: 1 }).format(n / 1048576) + ' Mo';
  const day = s => new Date(s).toLocaleDateString('fr-FR', { day: 'numeric', month: 'long', year: 'numeric' });
  const osOf = n => /\.(exe|msi)$/i.test(n) ? 'Windows'
    : /\.dmg$|\.app\.tar\.gz$/i.test(n) ? (/aarch64|arm64/i.test(n) ? 'Mac Apple' : 'Mac Intel')
    : /\.(AppImage|deb|rpm)$/i.test(n) ? 'Linux' : 'autre';

  async function detect() {
    let platform = '', arch = '';
    try {
      const d = navigator.userAgentData;
      if (d && d.getHighEntropyValues) ({ platform = '', architecture: arch = '' } = await d.getHighEntropyValues(['architecture', 'platform']));
    } catch { }
    const ua = navigator.userAgent;
    if (/Android|iPhone|iPad|iPod/i.test(ua) || (/Macintosh/.test(ua) && navigator.maxTouchPoints > 1)) return { os: null };
    if (!platform) platform = /Windows/i.test(ua) ? 'Windows' : /Mac/i.test(ua) ? 'macOS' : /Linux|X11/i.test(ua) ? 'Linux' : '';
    const os = platform === 'Windows' ? 'win' : platform === 'macOS' ? 'mac' : platform === 'Linux' ? 'linux' : null;
    const known = arch === 'arm' || arch === 'x86' || /aarch64|arm64/i.test(ua);
    const arm = arch === 'arm' || /aarch64|arm64/i.test(ua) || (os === 'mac' && !known);   // Safari reports Intel on Apple Silicon: guess arm
    return { os, arm, guessed: os === 'mac' && !known };
  }

  // [primary, alternative] file name patterns per system.
  const pick = ({ os, arm }) => os === 'win' ? [/-setup\.exe$/i, /\.msi$/i]
    : os === 'mac' ? (arm ? [/aarch64\.dmg$/i, /x64\.dmg$/i] : [/x64\.dmg$/i, /aarch64\.dmg$/i])
    : os === 'linux' && !arm ? [/\.AppImage$/i, /\.deb$/i] : [];

  const DESC = {
    win: 'Installeur pour Windows 10/11. Non signé : SmartScreen demandera une confirmation (« Informations complémentaires », puis « Exécuter quand même »).',
    mac: 'Image disque pour Mac. Non signée : au premier lancement, fais un clic droit sur l\'app, « Ouvrir », puis confirme.',
    linux: 'AppImage pour Linux x64. Nécessite WebKitGTK 4.1 ; rends-la exécutable avec chmod +x avant de la lancer.',
  };

  const status = (s, text) => { $('dl-status').dataset.s = s; $('dl-status-t').textContent = text; };
  const pill = (cls, text) => { $('dl-pill').className = 'pill ' + cls; $('dl-pill').textContent = text; };
  const go = (href, label) => { $('dl-go').href = href; $('dl-go-t').textContent = label; };
  const more = $('dl-more'), list = $('others');
  more.addEventListener('click', () => {
    const open = more.getAttribute('aria-expanded') !== 'true';
    more.setAttribute('aria-expanded', open);
    list.hidden = !open;
  });
  const openList = () => { more.setAttribute('aria-expanded', 'true'); list.hidden = false; };

  function fill(assets, mine) {
    list.replaceChildren(...assets.map(a => {
      const li = document.createElement('li'), link = document.createElement('a');
      if (a === mine) li.className = 'me';
      link.href = a.browser_download_url;
      for (const [cls, t] of [['os', osOf(a.name)], ['n', a.name], ['z', size(a.size)]]) {
        const s = document.createElement('span'); s.className = cls; s.textContent = t; link.append(s);
      }
      li.append(link);
      return li;
    }));
  }

  function failed(none, rel) {
    more.hidden = true;
    $('dl-file').textContent = 'github.com/Atypical-Consulting/ClaudeCodeUI/releases';
    $('dl-size').textContent = '';
    $('dl-date').textContent = '—';
    if (none) {
      $('dl-title').textContent = rel ? `La version ${rel.tag_name} n'a pas encore ses installeurs` : 'Aucune version publiée pour l\'instant';
      $('dl-desc').textContent = rel
        ? 'Les installeurs sont construits après la publication de la version et y sont attachés au bout de quelques minutes. Reviens un peu plus tard, ou suis la page des versions.'
        : 'Les installeurs Windows, macOS et Linux seront attachés à la première version GitHub dès sa publication. D\'ici là, l\'application se compile depuis les sources.';
      $('dl-ver').textContent = rel ? rel.tag_name : 'aucune';
      $('dl-pill').hidden = true;
      status('idle', 'Claude attend la première version publiée pour te proposer l\'installeur.');
      mood('sleepy', 'Pas encore de version', 'Les installeurs arrivent avec la première version.');
      go(RELEASES, 'Suivre les versions');
      // Nothing to refuse yet: the third verb becomes the way to get the app today.
      const no = $('dl-no');
      no.textContent = 'Compiler depuis les sources';
      no.href = 'docs.html#developpement';
      no.classList.remove('danger');
      $('dl-alt').hidden = true;
    } else {
      $('dl-title').textContent = 'GitHub ne répond pas';
      $('dl-desc').textContent = 'Impossible de lire la dernière version : réseau coupé ou limite de requêtes de l\'API GitHub atteinte. Les installeurs restent disponibles sur la page des versions.';
      $('dl-ver').textContent = 'inconnue';
      pill('err', 'erreur');
      status('err', 'L\'API GitHub n\'a pas répondu : ouvre la page des versions pour télécharger.');
      mood('sad', 'GitHub ne répond pas', 'Passe par la page des versions');
      go(RELEASES + '/latest', 'Ouvrir les versions');
    }
  }

  (async () => {
    const [who, res] = await Promise.all([
      detect(),
      fetch('https://api.github.com/repos/Atypical-Consulting/ClaudeCodeUI/releases/latest', {
        headers: { Accept: 'application/vnd.github+json' },
        signal: AbortSignal.timeout ? AbortSignal.timeout(10000) : undefined,
      }).then(r => r, () => null),
    ]);
    $('dl-os').textContent = who.os ? OS[who.os] + (who.os === 'mac' ? (who.arm ? ' · Apple Silicon' : ' · Intel') : ' · x64') : 'non détecté';
    if (!res || (!res.ok && res.status !== 404)) return failed(false);
    if (res.status === 404) return failed(true);
    let rel;
    try { rel = await res.json(); } catch { return failed(false); }
    const assets = (rel.assets || []).filter(a => !/\.(sig|json)$/i.test(a.name)).sort((a, b) => osOf(a.name).localeCompare(osOf(b.name)) || a.name.localeCompare(b.name));
    if (!assets.length) return failed(true, rel);

    const [p1, p2] = pick(who);
    const mine = p1 && assets.find(a => p1.test(a.name)), alt = p2 && assets.find(a => p2.test(a.name));
    fill(assets, mine);
    $('dl-ver').textContent = rel.tag_name;
    $('dl-date').textContent = day(rel.published_at || rel.created_at);
    pill('wait', '1 sur 1');

    if (!mine) {
      $('dl-title').textContent = 'Choisir un installeur';
      $('dl-desc').textContent = who.os
        ? `Pas d'installeur pour ce système dans la version ${rel.tag_name}. Voici tous les fichiers publiés.`
        : 'Claude Code UI est une application de bureau pour Windows, macOS et Linux. Choisis le fichier de ton ordinateur :';
      $('dl-file').textContent = `${assets.length} fichiers · ${rel.tag_name}`;
      $('dl-size').textContent = '';
      go(rel.html_url || RELEASES + '/latest', 'Voir la version');
      openList();
      status('wait', 'Choisis un installeur dans la liste.');
      mood('happy', `Version ${rel.tag_name}`, 'Choisis ton système');
      return;
    }

    $('dl-sub').textContent = OS[who.os];
    $('dl-title').textContent = `Télécharger Claude Code UI pour ${OS[who.os]} ?`;
    $('dl-desc').textContent = DESC[who.os];
    $('dl-file').textContent = mine.name;
    $('dl-size').textContent = size(mine.size);
    go(mine.browser_download_url, 'Autoriser');
    $('dl-go').setAttribute('aria-label', `Autoriser : télécharger ${mine.name} (${size(mine.size)})`);
    if (alt) {
      const why = who.os === 'mac' ? (who.guessed ? 'Mac Intel ? Prends ' : 'Ou ') : 'Ou ';
      const a = Object.assign(document.createElement('a'), { href: alt.browser_download_url, textContent: alt.name });
      $('dl-alt').replaceChildren(why, a, ` (${size(alt.size)}). Refuser ouvre la documentation d'installation.`);
    }
    status('wait', 'Claude est en pause : le téléchargement attend ton accord.');
    mood('happy', 'Installeur prêt', `${OS[who.os]} · ${rel.tag_name}`);
    $('dl-go').addEventListener('click', () => {
      status('ok', `Téléchargement lancé : ${mine.name}`);
      pill('idle', 'autorisé');
      mood('excited', 'C\'est parti', mine.name);
    });
  })();
})();
