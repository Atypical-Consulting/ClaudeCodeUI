const fs=require('fs'),path=require('path');
const [dir,src,out]=process.argv.slice(2);
let h=fs.readFileSync(src,'utf8').replace('{{RAIL_DEF}}','');
let uid=0;
const svg=(n,s)=>{let x=fs.readFileSync(path.join(dir,n+'.html'),'utf8');if(s)x=x.replace(/width="120" height="120"/,`width="${s}" height="${s}"`);
  const u='u'+(uid++);x=x.replace(/id="([^"]+)"/g,(_,i)=>`id="${i}-${u}"`).replace(/url\(#([^)]+)\)/g,(_,i)=>`url(#${i}-${u})`);return x;};
const S={ // id: [name, dir, state, right]
 store:['session-store','feat/session-store'], auth:['fix-flaky-auth-test','fix/flaky-auth'], bump:['bump-markdig','chore/bump-markdig'],
 docs:['docs-readme','main · site'], migr:['migrate-net10','chore/net10']};
const V={
 new:{on:'new',st:{store:'idle',auth:'run',bump:'idle',docs:'idle',migr:'idle'},duck:['duck-happy','Tout est calme','Aucune décision en attente']},
 run:{on:'store',st:{store:'run',auth:'run',bump:'idle',docs:'idle',migr:'idle'},duck:['duck-excited','Claude travaille','2 sessions en cours']},
 wait:{on:'store',st:{store:'wait',auth:'wait',bump:'run',docs:'idle',migr:'err'},duck:['duck-shocked','2 décisions t’attendent','Ctrl ⇧ A pour la suivante']},
 ov:{on:'ov',st:{store:'wait',auth:'wait',bump:'run',docs:'idle',migr:'err'},duck:['duck-shocked','2 décisions t’attendent','Ctrl ⇧ A pour la suivante']},
 md:{on:'store',st:{store:'idle',auth:'run',bump:'idle',docs:'idle',migr:'idle'},duck:['duck-blissful','Tour terminé','4 tests passent']},
 set:{on:'none',st:{store:'idle',auth:'run',bump:'idle',docs:'idle',migr:'idle'},duck:['duck-happy','Tout est calme','Aucune décision en attente']},
 wt:{on:'wt',st:{store:'wait',auth:'wait',bump:'run',docs:'idle',migr:'err'},duck:['duck-blissful','1,34 Go à récupérer','4 worktrees sans risque']},
 comp:{on:'store',st:{store:'idle',auth:'run',bump:'run',docs:'idle',migr:'idle'},duck:['duck-happy','Prêt pour la suite','2 sessions en cours']},
 ultra:{on:'none',st:{store:'idle',auth:'run',bump:'run',docs:'idle',migr:'idle'},duck:['duck-excited','6 agents au travail','review-session-store']},
 ext:{on:'ext',st:{store:'idle',auth:'run',bump:'idle',docs:'idle',migr:'idle'},duck:['duck-sad','1 serveur MCP en échec','github']},
 err:{on:'migr',st:{store:'wait',auth:'wait',bump:'run',docs:'idle',migr:'err'},duck:['duck-sad','Une session est tombée','migrate-net10 · exit 1']},
};
const right=st=>st==='wait'?'<span class="pill wait">1</span>':st==='run'?'<span class="c" style="color:var(--ok)">en cours</span>':st==='err'?'<span class="pill err">exit 1</span>':'';
const rail=v=>{const c=V[v];const row=k=>`<div class="s${c.on===k?' on':''}"><span class="dot ${c.st[k]}"></span><div><div class="n">${S[k][0]}</div><div class="branch"><svg class="i"><use href="#i-branch"/></svg>${S[k][1]}</div></div>${right(c.st[k])||'<span class="c">'+({docs:'$0.42',store:'',bump:'',migr:'$1.87',auth:''}[k])+'</span>'}</div>`;
 const active=['store','auth','bump'].filter(k=>c.st[k]!=='idle'||k==='store');
 return `<div class="pane">
  <div class="ph"><span class="brand"><i></i>Claude Code</span></div>
  <div class="rail-actions">
   <div class="btn block${c.on==='new'?' is-hover':''}"><svg class="i sm"><use href="#i-plus"/></svg>Nouvelle session<kbd>Ctrl N</kbd></div>
   <div class="search"><svg class="i sm"><use href="#i-search"/></svg>Rechercher<kbd>Ctrl K</kbd></div>
   <div class="btn block ghost${c.on==='ov'?' is-hover':''}"><svg class="i sm"><use href="#i-grid"/></svg>Vue d'ensemble</div>
   <div class="btn block ghost${c.on==='wt'?' is-hover':''}"><svg class="i sm"><use href="#i-branch"/></svg>Worktrees<span class="tag" style="margin-left:auto">4 à nettoyer</span></div>
   <div class="btn block ghost${c.on==='ext'?' is-hover':''}"><svg class="i sm"><use href="#i-plug"/></svg>Extensions${c.on==='ext'||true?'<span class="pill err" style="margin-left:auto">1</span>':''}</div>
  </div>
  <div class="grp"><span class="cap">Actives</span></div>
  <div class="list">${['store','auth','bump'].map(row).join('')}</div>
  <div class="grp"><span class="cap">Récentes</span></div>
  <div class="list">${['docs','migr'].map(row).join('')}</div>
  <div class="quota" title="rate_limit_event"><div class="qh"><span class="cap">Utilisation</span><span>réinit. 18:00</span></div><div class="qrow"><span>5 h</span><span class="bar"><i style="width:32%"></i></span><span>32 %</span></div><div class="qrow"><span>7 j</span><span class="bar"><i style="width:25%"></i></span><span>25 %</span></div></div>
  <div class="mascot"><div class="duck">${svg(c.duck[0],48)}</div><div><b>${c.duck[1]}</b>${c.duck[2]}</div></div>
 </div>`;};
h=h.replace(/\{\{RAIL:(\w+)\}\}/g,(_,v)=>rail(v));
h=h.replace(/\{\{(duck-[a-z]+)-(\d+)\}\}/g,(_,n,s)=>svg(n,s));
const MD=process.argv[5];
h=h.replace(/\{\{MD:(\w+)\}\}/g,(_,n)=>{let x=fs.readFileSync(path.join(MD,n+'.html'),'utf8');if(n==='running')x=x.replace(/dans git<\/li>/,'dans git<span class="caret"></span></li>');return x;});
const TH=[['graphite','Graphite'],['encre','Encre'],['ristretto','Ristretto'],['mousse','Mousse'],['contraste','Contraste élevé']];
h=h.replace('{{THEMECARDS}}',TH.map(([k,n])=>`<div class="tc" data-theme="${k}" data-set-theme="${k}" role="button" tabindex="0" aria-label="Thème ${n}"><div class="pv"><div class="rl"><i></i><i></i><i></i><i></i></div><div class="bd"><div class="ln"><span style="color:var(--read)">Read</span> <span style="color:var(--fg-2)">Store.cs</span></div><div class="ln"><span style="color:var(--edit)">Edit</span> <span style="color:var(--fg-2)">Home.razor</span></div><div class="ln"><span style="color:var(--syn-kw)">static</span> <span style="color:var(--syn-type)">Store</span> <span style="color:var(--syn-fn)">Load</span><span style="color:var(--fg-3)">(</span><span style="color:var(--syn-str)">"cwd"</span><span style="color:var(--fg-3)">)</span></div><div class="ln" style="color:var(--syn-com);font-style:italic">// ${n.toLowerCase()}</div><span class="btnp">Autoriser</span></div></div><div class="nm"><b>${n}</b><span class="sw"><i style="background:var(--syn-kw)"></i><i style="background:var(--syn-str)"></i><i style="background:var(--syn-fn)"></i><i style="background:var(--syn-type)"></i></span></div></div>`).join(''));
fs.writeFileSync(out,h);
