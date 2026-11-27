import fs from 'node:fs';
import {marked} from 'marked';
import Slugger from 'github-slugger';

const pages = [
  ['','Overview','Getting started'], ['setup','Install Coven Host','Getting started'],
  ['operations','Operator guide','Guides'], ['install','Advanced installation','Guides'],
  ['acceptance','Platform qualification','Reference'], ['plan','Delivery plan','Reference']
];
if (fs.existsSync('../docs/uploaded-documents.md')) {
  pages.push(['uploaded-documents', 'Uploaded documents', 'Reference']);
}
const pageIds = new Set(pages.map(([id]) => id));
const home = `# Coven Host documentation

Sync an ordinary folder with one authenticated peer. Install Host on your computer, work offline, and recover both versions when edits conflict.

## Start with your folder

<div class="cards"><a href="/setup/"><b>Install Coven Host</b><span>Choose your desktop package and open your first synced folder.</span></a><a href="/operations/#connect-through-ssh"><b>Connect a remote peer</b><span>Use SSH to sync with a Linux machine and resume after disconnecting.</span></a></div>

## How sync works

The desktop installer starts a private peer on your computer. Save a document under \`Coven/Castles/demo/files/\`. Files returned by the peer appear under \`results/\` in the same Castle. The installer uses two local copies by default.

You can keep editing while disconnected. When the peer returns, Host reconciles changes. If a deletion races with an edit, you can inspect the saved version before choosing what to keep. [Recover a deletion or rename](/operations/#recover-a-deletion-or-rename)
`;
const out = 'dist';
fs.rmSync(out, {recursive: true, force: true});
fs.mkdirSync(out,{recursive:true});
fs.cpSync('../docs/assets/fonts',out+'/fonts',{recursive:true});
for(const [src,dst] of [['../docs/assets/crown.svg','crown.svg'],['style.css','style.css']]) fs.copyFileSync(src,out+'/'+dst);
for(const [id,title,group] of pages){
  const slugger=new Slugger(), headings=[];
  const renderer=new marked.Renderer();
  renderer.heading=function({depth,text,tokens}){
    const slug=slugger.slug(text);
    if(depth===2) headings.push([slug,text]);
    return '<h'+depth+' id="'+slug+'">'+this.parser.parseInline(tokens)+'</h'+depth+'>';
  };
  const md=id?fs.readFileSync('../docs/'+id+'.md','utf8'):home;
  const linkedMarkdown = md.replaceAll('](../', '](https://github.com/OpenCoven/coven-host/blob/main/');
  const body=marked.parse(linkedMarkdown.replace(/\]\(([\w-]+)\.md(?=[)#])/g, (link, id) => pageIds.has(id) ? '](/'+id+'/' : link),{renderer});
  const nav=pages.map(([path,label,section],i)=>(pages[i-1]?.[2]!==section?'<h3>'+section+'</h3>':'')+'<a '+(path===id?'aria-current="page"':'')+' href="/'+(path?path+'/':'')+'">'+label+'</a>').join('');
  const html=`<!doctype html><html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>${title} · Coven Host Docs</title><link rel="canonical" href="https://docs.coven.host/${id?id+'/':''}"><link rel="icon" href="/crown.svg"><link rel="stylesheet" href="/style.css"></head><body><a class="skip" href="#content">Skip to content</a><header><a class="brand" href="/"><img src="/crown.svg" alt="">Coven Host <small>Docs</small></a><a href="https://github.com/OpenCoven/coven-host">GitHub</a></header><div class="layout"><aside><nav aria-label="Documentation">${nav}</nav></aside><details class="mobile"><summary>Documentation menu</summary><nav aria-label="Mobile documentation">${nav}</nav></details><main id="content"><div class="crumb">Documentation <i>/</i> ${group}</div>${body}<footer><a href="https://github.com/OpenCoven/coven-host${id?'/blob/main/docs/'+id+'.md':''}">View source on GitHub</a></footer></main><nav class="toc" aria-label="On this page"><h3>On this page</h3>${headings.map(([slug,text])=>'<a href="#'+slug+'">'+text+'</a>').join('')}</nav></div></body></html>`;
  fs.mkdirSync(out+'/'+id,{recursive:true});
  fs.writeFileSync(out+'/'+id+'/index.html',html);
}
