// Claude Code UI · site: theme, ledger rows, streamed lede, download card (index), table of contents (docs).
(() => {
  const root = document.documentElement, $ = s => document.getElementById(s);
  const calm = matchMedia('(prefers-reduced-motion: reduce)').matches;
  root.classList.add('js');

  // Language: the page's <html lang> decides; French pages sit at the root, English ones under en/.
  const EN = root.lang === 'en', A = EN ? '../' : '';
  const T = EN ? {
    copy: 'Copy', copied: 'Copied', locale: 'en-US', mb: 'MB',
    shots: {
      installation: ['04', 'Sessions overview', 'Overview: five sessions with their state, mode and last action, and the queue of pending decisions.'],
      sessions: ['03', 'Permission request', 'Permission request: an edit to Home.razor with its diff, and the Allow, Whole session and Deny buttons.'],
      composer: ['09', 'Composer', 'Composer: model picker, effort, Fast, Ultracode, and the slash command menu.'],
      subagents: ['10', 'Ultracode and subagents', 'Ultracode workflow: Review, Verification and Synthesis phases, six agents and the details of the selected agent.'],
      worktrees: ['08', 'Worktrees', 'Worktrees: a table ranked by state with the reason, the size, and the cleanup plan in the inspector.'],
      extensions: ['11', 'Extensions', 'Extensions: MCP servers with their state, and the Skills, Agents and Plugins tabs.'],
      themes: ['07', 'Appearance', 'Appearance: the five dark themes and the code size setting.'],
      shortcuts: ['03', 'Permission request', 'Permission request: Allow ⏎, Whole session Shift ⏎, Deny Del.'],
      security: ['06', 'Markdown rendering', 'Markdown rendering of a reply: headings, a table, a task list and a syntax-highlighted code block.'],
    },
    docsDev: 'docs.html#development',
    noReleaseTitle: tag => tag ? `Release ${tag} has no installers yet` : 'No release published yet',
    noReleaseDesc: tag => tag
      ? 'Installers are built after a release is published and attached to it within a few minutes. Check back shortly, or follow the releases page.'
      : 'The Windows, macOS and Linux installers will be attached to the first GitHub release as soon as it is published. Until then, the app can be built from source.',
    none: 'none',
    waitingStatus: 'Claude is waiting for the first published release to offer you the installer.',
    sleepy: ['No release yet', 'Installers arrive with the first release.'],
    followReleases: 'Follow releases', build: 'Build from source',
    ghDown: 'GitHub is not responding',
    ghDownDesc: 'Could not read the latest release: the network is down or the GitHub API rate limit was reached. Installers are still available on the releases page.',
    unknown: 'unknown', error: 'error',
    ghDownStatus: 'The GitHub API did not respond: open the releases page to download.',
    sad: ['GitHub is not responding', 'Use the releases page'], openReleases: 'Open releases',
    notDetected: 'not detected',
    pick: 'Choose an installer',
    noMatch: tag => `No installer for this system in release ${tag}. Here are all the published files.`,
    anyOs: 'Claude Code UI is a desktop app for Windows, macOS and Linux. Choose the file for your computer:',
    files: (n, tag) => `${n} files · ${tag}`, viewRelease: 'View release',
    pickStatus: 'Choose an installer from the list.',
    version: tag => `Release ${tag}`, pickOs: 'Pick your system',
    dlTitle: os => `Download Claude Code UI for ${os}?`,
    allow: 'Allow', allowLabel: (n, sz) => `Allow: download ${n} (${sz})`,
    or: 'Or ', intelMood: ['Intel Mac detected', 'Apple Silicon only'],
    appleOnly: 'Requires an Apple Silicon Mac; Intel Macs are not supported. Deny opens the installation docs.',
    intelTitle: 'Intel Macs are not supported', intelDesc: 'Claude Code UI ships for Apple Silicon Macs only (macOS 26 is the last macOS version for Intel). Windows and Linux builds are available.',
    intelStatus: 'No installer for Intel Macs.',
    refuse: sz => ` (${sz}). Deny opens the installation docs.`,
    desc: {
      win: 'Installer for Windows 10/11. Unsigned: SmartScreen will ask for confirmation ("More info", then "Run anyway").',
      mac: 'Disk image for Mac. Signed and notarized by Apple: it opens normally.',
      linux: 'AppImage for Linux x64. Requires WebKitGTK 4.1; make it executable with chmod +x before running it.',
    },
    paused: 'Claude is paused: the download is waiting for your approval.',
    ready: 'Installer ready',
    started: n => `Download started: ${n}`, allowed: 'allowed', go: "Here we go",
    day: { day: 'numeric', month: 'long', year: 'numeric' },
  } : {
    copy: 'Copier', copied: 'Copié', locale: 'fr-FR', mb: 'Mo',
    shots: {
      installation: ['04', 'Vue d\'ensemble des sessions', 'Vue d\'ensemble : cinq sessions avec leur état, leur mode, leur dernière action, et la file des décisions en attente.'],
      sessions: ['03', 'Demande de permission', 'Demande de permission : modification de Home.razor avec le diff, et les boutons Autoriser, Toute la session, Refuser.'],
      composer: ['09', 'Composer', 'Composer : sélecteur de modèle, effort, Rapide, Ultracode, et le menu des commandes slash.'],
      'sous-agents': ['10', 'Ultracode et sous-agents', 'Workflow ultracode : phases Revue, Vérification, Synthèse, six agents et le détail de l\'agent sélectionné.'],
      worktrees: ['08', 'Worktrees', 'Worktrees : tableau classé par état avec la raison, la taille, et le plan de nettoyage dans l\'inspecteur.'],
      extensions: ['11', 'Extensions', 'Extensions : serveurs MCP avec leur état, onglets Skills, Agents, Plugins.'],
      themes: ['07', 'Apparence', 'Apparence : les cinq thèmes sombres et le réglage de la taille du code.'],
      raccourcis: ['03', 'Demande de permission', 'Demande de permission : Autoriser ⏎, Toute la session Maj ⏎, Refuser Suppr.'],
      securite: ['06', 'Rendu Markdown', 'Rendu Markdown d\'une réponse : titres, tableau, liste de tâches et bloc de code coloré.'],
    },
    docsDev: 'docs.html#developpement',
    noReleaseTitle: tag => tag ? `La version ${tag} n'a pas encore ses installeurs` : 'Aucune version publiée pour l\'instant',
    noReleaseDesc: tag => tag
      ? 'Les installeurs sont construits après la publication de la version et y sont attachés au bout de quelques minutes. Reviens un peu plus tard, ou suis la page des versions.'
      : 'Les installeurs Windows, macOS et Linux seront attachés à la première version GitHub dès sa publication. D\'ici là, l\'application se compile depuis les sources.',
    none: 'aucune',
    waitingStatus: 'Claude attend la première version publiée pour te proposer l\'installeur.',
    sleepy: ['Pas encore de version', 'Les installeurs arrivent avec la première version.'],
    followReleases: 'Suivre les versions', build: 'Compiler depuis les sources',
    ghDown: 'GitHub ne répond pas',
    ghDownDesc: 'Impossible de lire la dernière version : réseau coupé ou limite de requêtes de l\'API GitHub atteinte. Les installeurs restent disponibles sur la page des versions.',
    unknown: 'inconnue', error: 'erreur',
    ghDownStatus: 'L\'API GitHub n\'a pas répondu : ouvre la page des versions pour télécharger.',
    sad: ['GitHub ne répond pas', 'Passe par la page des versions'], openReleases: 'Ouvrir les versions',
    notDetected: 'non détecté',
    pick: 'Choisir un installeur',
    noMatch: tag => `Pas d'installeur pour ce système dans la version ${tag}. Voici tous les fichiers publiés.`,
    anyOs: 'Claude Code UI est une application de bureau pour Windows, macOS et Linux. Choisis le fichier de ton ordinateur :',
    files: (n, tag) => `${n} fichiers · ${tag}`, viewRelease: 'Voir la version',
    pickStatus: 'Choisis un installeur dans la liste.',
    version: tag => `Version ${tag}`, pickOs: 'Choisis ton système',
    dlTitle: os => `Télécharger Claude Code UI pour ${os} ?`,
    allow: 'Autoriser', allowLabel: (n, sz) => `Autoriser : télécharger ${n} (${sz})`,
    or: 'Ou ', intelMood: ['Mac Intel détecté', 'Apple Silicon uniquement'],
    appleOnly: 'Nécessite un Mac Apple Silicon ; les Mac Intel ne sont pas pris en charge. Refuser ouvre la documentation d\'installation.',
    intelTitle: 'Les Mac Intel ne sont pas pris en charge', intelDesc: 'Claude Code UI est livré pour les Mac Apple Silicon uniquement (macOS 26 est la dernière version de macOS pour Intel). Des versions Windows et Linux sont disponibles.',
    intelStatus: 'Aucun installeur pour les Mac Intel.',
    refuse: sz => ` (${sz}). Refuser ouvre la documentation d'installation.`,
    desc: {
      win: 'Installeur pour Windows 10/11. Non signé : SmartScreen demandera une confirmation (« Informations complémentaires », puis « Exécuter quand même »).',
      mac: 'Image disque pour Mac. Signée et notariée par Apple : elle s\'ouvre normalement.',
      linux: 'AppImage pour Linux x64. Nécessite WebKitGTK 4.1 ; rends-la exécutable avec chmod +x avant de la lancer.',
    },
    paused: 'Claude est en pause : le téléchargement attend ton accord.',
    ready: 'Installeur prêt',
    started: n => `Téléchargement lancé : ${n}`, allowed: 'autorisé', go: 'C\'est parti',
    day: { day: 'numeric', month: 'long', year: 'numeric' },
  };

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
    const btn = box.querySelector('.copy') || box.appendChild(Object.assign(document.createElement('button'), { className: 'copy', type: 'button', textContent: T.copy }));
    btn.addEventListener('click', () => {
      const text = [...pre.childNodes].filter(n => !(n.classList && n.classList.contains('c'))).map(n => n.textContent).join('').replace(/^\s*\n/gm, '').trim();
      navigator.clipboard.writeText(text).then(() => {
        btn.textContent = T.copied;
        setTimeout(() => btn.textContent = T.copy, 1400);
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
    const SHOTS = T.shots;
    const shot = id => {
      const v = SHOTS[id], img = $('shot-img');
      if (!v || !img) return;
      img.src = $('shot-a').href = `${A}assets/screens/${EN ? 'en/' : ''}${v[0]}.png`;
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

  // Docs, known limitations: the open issues, read live so a fixed one leaves the list on its own.
  // The static link in the HTML stays if GitHub is unreachable or rate-limited.
  const issues = $('issues');
  if (issues) fetch('https://api.github.com/repos/Atypical-Consulting/ClaudeCodeUI/issues?state=open&per_page=50', {
    headers: { Accept: 'application/vnd.github+json' },
  }).then(r => r.ok ? r.json() : Promise.reject())
    .then(list => {
      const open = list.filter(i => !i.pull_request && i.user?.type !== 'Bot');
      issues.replaceChildren(...(open.length ? open.map(i => {
        const li = document.createElement('li');
        li.append(Object.assign(document.createElement('span'), { textContent: '#' + i.number }),
                  Object.assign(document.createElement('a'), { href: i.html_url, textContent: i.title }));
        return li;
      }) : [Object.assign(document.createElement('li'), { textContent: EN ? 'No known open issue.' : 'Aucun problème ouvert connu.' })]));
    }, () => {});

  // Index: the permission card does the download.
  if (!$('dl')) return;
  const REPO = 'https://github.com/Atypical-Consulting/ClaudeCodeUI', RELEASES = REPO + '/releases';
  const OS = { win: 'Windows', mac: 'macOS', linux: 'Linux' };
  const size = n => new Intl.NumberFormat(T.locale, { maximumFractionDigits: 1 }).format(n / 1048576) + ' ' + T.mb;
  const day = s => new Date(s).toLocaleDateString(T.locale, T.day);
  const osOf = n => /\.(exe|msi)$/i.test(n) ? 'Windows'
    : /\.dmg$|\.app\.tar\.gz$/i.test(n) ? 'Mac Apple'
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
    return { os, arm, guessed: os === 'mac' && !known, intel: os === 'mac' && arch === 'x86' };   // only Chromium's client hints are sure enough to refuse
  }

  // [primary, alternative] file name patterns per system.
  const pick = ({ os, arm }) => os === 'win' ? [/-setup\.exe$/i, /\.msi$/i]
    : os === 'mac' ? [/aarch64\.dmg$/i]
    : os === 'linux' && !arm ? [/\.AppImage$/i, /\.deb$/i] : [];

  const DESC = T.desc;

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
      $('dl-title').textContent = T.noReleaseTitle(rel && rel.tag_name);
      $('dl-desc').textContent = T.noReleaseDesc(rel && rel.tag_name);
      $('dl-ver').textContent = rel ? rel.tag_name : T.none;
      $('dl-pill').hidden = true;
      status('idle', T.waitingStatus);
      mood('sleepy', ...T.sleepy);
      go(RELEASES, T.followReleases);
      // Nothing to refuse yet: the third verb becomes the way to get the app today.
      const no = $('dl-no');
      no.textContent = T.build;
      no.href = T.docsDev;
      no.classList.remove('danger');
      $('dl-alt').hidden = true;
    } else {
      $('dl-title').textContent = T.ghDown;
      $('dl-desc').textContent = T.ghDownDesc;
      $('dl-ver').textContent = T.unknown;
      pill('err', T.error);
      status('err', T.ghDownStatus);
      mood('sad', ...T.sad);
      go(RELEASES + '/latest', T.openReleases);
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
    $('dl-os').textContent = who.os ? OS[who.os] + (who.os === 'mac' ? (who.intel ? ' · Intel' : ' · Apple Silicon') : ' · x64') : T.notDetected;
    if (who.intel) {
      more.hidden = true; $('dl-pill').hidden = true; $('dl-alt').hidden = true;
      $('dl-title').textContent = T.intelTitle; $('dl-desc').textContent = T.intelDesc;
      $('dl-file').textContent = ''; $('dl-size').textContent = ''; $('dl-date').textContent = '—';
      status('err', T.intelStatus); mood('sad', ...T.intelMood); go(RELEASES + '/latest', T.openReleases);
      return;
    }
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
      $('dl-title').textContent = T.pick;
      $('dl-desc').textContent = who.os ? T.noMatch(rel.tag_name) : T.anyOs;
      $('dl-file').textContent = T.files(assets.length, rel.tag_name);
      $('dl-size').textContent = '';
      go(rel.html_url || RELEASES + '/latest', T.viewRelease);
      openList();
      status('wait', T.pickStatus);
      mood('happy', T.version(rel.tag_name), T.pickOs);
      return;
    }

    $('dl-sub').textContent = OS[who.os];
    $('dl-title').textContent = T.dlTitle(OS[who.os]);
    $('dl-desc').textContent = DESC[who.os];
    $('dl-file').textContent = mine.name;
    $('dl-size').textContent = size(mine.size);
    go(mine.browser_download_url, T.allow);
    $('dl-go').setAttribute('aria-label', T.allowLabel(mine.name, size(mine.size)));
    if (who.os === 'mac' && who.guessed) $('dl-alt').textContent = T.appleOnly;
    else if (alt) {
      const a = Object.assign(document.createElement('a'), { href: alt.browser_download_url, textContent: alt.name });
      $('dl-alt').replaceChildren(T.or, a, T.refuse(size(alt.size)));
    }
    status('wait', T.paused);
    mood('happy', T.ready, `${OS[who.os]} · ${rel.tag_name}`);
    $('dl-go').addEventListener('click', () => {
      status('ok', T.started(mine.name));
      pill('idle', T.allowed);
      mood('excited', T.go, mine.name);
    });
  })();
})();
