// Local-only approximation of GitHub Markdown for light/dark and mobile visual QA.
import {createServer} from 'node:http';
import {readFile,writeFile,mkdir} from 'node:fs/promises';
import path from 'node:path';
import MarkdownIt from 'markdown-it';
import {root,here,openBrowser,localPage} from './build.mjs';

const md=new MarkdownIt({html:true,linkify:false});
const fence=md.renderer.rules.fence;
md.renderer.rules.fence=(tokens,index,options,env,self)=>tokens[index].info.trim()==='mermaid'?`<pre class="mermaid">${md.utils.escapeHtml(tokens[index].content)}</pre>`:fence(tokens,index,options,env,self);
const css=`*{box-sizing:border-box}body{margin:0;color:#1f2328;background:#fff;font:16px/1.5 -apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif}body.dark{color:#e6edf3;background:#0d1117}main{max-width:1040px;margin:32px auto;padding:32px;border:1px solid #d0d7de;border-radius:6px}body.dark main{border-color:#30363d}h1,h2{padding-bottom:.3em;border-bottom:1px solid #d0d7de;line-height:1.25}h1{font-size:32px;margin-top:0}h2{font-size:24px;margin-top:28px}p{margin:16px 0}a{color:#0969da;text-decoration:none}body.dark a{color:#58a6ff}img{max-width:100%;height:auto;vertical-align:middle}picture{display:block;margin:16px 0}pre{padding:16px;overflow:auto;background:#f6f8fa;border-radius:6px;font-size:13px}body.dark pre{background:#161b22}code{font-family:ui-monospace,Consolas,monospace;font-size:85%;background:#afb8c133;padding:.15em .3em;border-radius:4px}pre code{padding:0;background:none}table{display:block;max-width:100%;overflow:auto;border-collapse:collapse;font-size:15px}th,td{padding:10px 14px;border:1px solid #d0d7de;min-width:140px;text-align:left;vertical-align:top}tr:nth-child(2n){background:#f6f8fa}body.dark tr:nth-child(2n){background:#161b22}body.dark td,body.dark th{border-color:#30363d}blockquote{border-left:4px solid #0969da;margin:16px 0;padding:0 16px;color:#57606a}body.dark blockquote{color:#adbac7}blockquote.alert-caution{border-color:#cf222e}blockquote.alert-warning{border-color:#9a6700}blockquote.alert-important{border-color:#8250df}details{margin:16px 0}summary{cursor:pointer;font-weight:600}.ci{display:inline-block;border-radius:4px;background:#142b3f;color:#f0f6fc;font-size:11px;padding:3px 10px}.mermaid{background:transparent!important}ul,ol{padding-left:24px}@media(max-width:640px){main{margin:0;padding:20px 16px;border:0;border-radius:0}h1{font-size:30px}body{font-size:15px}th,td{min-width:150px;padding:8px 12px}}`;

function html(source,theme,lang){
  const local=source.replace(/\[!\[CI workflow\]\([^\n]+\)\]\([^\n]+\)/,'<span class="ci">CI · offline preview</span>').replace(/\[!\[Flujo de CI\]\([^\n]+\)\]\([^\n]+\)/,'<span class="ci">CI · vista local</span>');
  let body=md.render(local);
  body=body.replace(/<blockquote>\s*<p>\[!(\w+)\]/g,(_,kind)=>`<blockquote class="alert-${kind.toLowerCase()}"><p><strong>${kind}</strong><br>`);
  return `<!doctype html><html lang="${lang}"><meta charset="utf-8"><meta name="viewport" content="width=device-width"><style>${css}</style><body class="${theme}"><main>${body}</main></body></html>`;
}
const server=createServer(async(req,res)=>{
  try{
    const url=new URL(req.url,'http://127.0.0.1');
    if(url.pathname==='/'){
      const file=url.searchParams.get('lang')==='es'?'README.es.md':'README.md';
      res.setHeader('Content-Type','text/html; charset=utf-8');res.end(html(await readFile(path.join(root,file),'utf8'),url.searchParams.get('theme')??'light',url.searchParams.get('lang')??'en'));return;
    }
    const file=path.resolve(root,'.'+decodeURIComponent(url.pathname));
    if(!file.startsWith(root+path.sep)){res.writeHead(403).end();return;}
    const types={'.svg':'image/svg+xml','.png':'image/png','.gif':'image/gif','.mp4':'video/mp4','.js':'text/javascript','.mjs':'text/javascript'};
    res.setHeader('Content-Type',types[path.extname(file)]??'text/plain');res.end(await readFile(file));
  }catch{res.writeHead(404).end();}
});
await new Promise(resolve=>server.listen(0,'127.0.0.1',resolve));
await mkdir(path.join(here,'.qa'),{recursive:true});
const browser=await openBrowser();
const findings=[];
try{
  for(const lang of ['en','es']) for(const theme of ['light','dark']) for(const width of [1184,390]){
    const page=await localPage(browser,width,900);
    await page.emulateMediaFeatures([{name:'prefers-color-scheme',value:theme},{name:'prefers-reduced-motion',value:'reduce'}]);
    await page.goto(`http://127.0.0.1:${server.address().port}/?lang=${lang}&theme=${theme}`,{waitUntil:'networkidle0'});
    const name=`${lang}-${theme}-${width}`;
    await page.screenshot({path:path.join(here,'.qa',name+'-top.png')});
    await page.screenshot({path:path.join(here,'.qa',name+'-full.png'),fullPage:true});
    if(lang==='en') {
      const pictures=await page.$$('picture');
      for(const [label,index] of [['pipeline',1],['gate',2],['results',3]]) await pictures[index].screenshot({path:path.join(here,'.qa',name+'-'+label+'.png')});
    }
    findings.push({name,...await page.evaluate(()=>({overflow:document.documentElement.scrollWidth>innerWidth,brokenImages:[...document.images].filter(i=>!i.complete||i.naturalWidth===0).map(i=>i.getAttribute('src'))}))});
    await page.evaluate(()=>document.querySelectorAll('details').forEach(d=>d.open=true));
    await page.addScriptTag({path:path.join(here,'node_modules/mermaid/dist/mermaid.min.js')});
    const diagramError=await page.evaluate(async theme=>{try{mermaid.initialize({startOnLoad:false,theme:theme==='dark'?'dark':'default',securityLevel:'strict'});await mermaid.run();return null;}catch(error){return error?.message??JSON.stringify(error);}},theme);
    if(diagramError) throw new Error(diagramError);
    const sequence=await page.$('.mermaid');
    await sequence.screenshot({path:path.join(here,'.qa',name+'-sequence.png')});
    await writeFile(path.join(here,'.qa',name+'.html'),await page.content());
    await page.close();
  }
  await writeFile(path.join(here,'.qa/findings.json'),JSON.stringify(findings,null,2)+'\n');
  console.log(JSON.stringify(findings,null,2));
  if(findings.some(f=>f.overflow||f.brokenImages.length)) throw new Error('README rendering has overflow or missing images');
}finally{await browser.close();await new Promise(resolve=>server.close(resolve));}
