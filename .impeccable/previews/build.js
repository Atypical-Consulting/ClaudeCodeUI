const fs=require('fs'),path=require('path');
const [dir,src,out]=process.argv.slice(2);
let h=fs.readFileSync(src,'utf8');
const svg=n=>fs.readFileSync(path.join(dir,n+'.html'),'utf8');
h=h.replace(/\{\{flap:([^}]*)\}\}/g,(_,t)=>[...t].map(c=>`<i>${c===' '?'&nbsp;':c}</i>`).join(''));
h=h.replace(/\{\{(duck-[a-z]+)-(\d+)\}\}/g,(_,n,s)=>svg(n).replace(/width="120" height="120"/,`width="${s}" height="${s}"`));
h=h.replace(/\{\{(duck-[a-z]+)\}\}/g,(_,n)=>svg(n));
fs.writeFileSync(out,h);
