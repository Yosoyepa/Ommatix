import {readFile,writeFile,mkdir,stat} from 'node:fs/promises';
import {existsSync} from 'node:fs';
import {fileURLToPath,pathToFileURL} from 'node:url';
import path from 'node:path';
import {spawn,execFileSync} from 'node:child_process';
import {createHash} from 'node:crypto';
import puppeteer from 'puppeteer-core';
import ffprobe from 'ffprobe-static';
import {optimize} from 'svgo';
import {hero,pipeline,results,gate,loop,social,loadContract,svg} from './artwork.mjs';
import {composition} from './storyboard.mjs';

export const here=path.dirname(fileURLToPath(import.meta.url));
export const root=path.resolve(here,'../..');
export const output=path.join(root,'docs/assets/readme');
export const chrome=process.env.HYPERFRAMES_BROWSER_PATH ?? path.join(here,'.cache/chrome-headless-shell/win64-129.0.6668.58/chrome-headless-shell-win64/chrome-headless-shell.exe');
export const browserArgs=['--disable-background-networking','--disable-component-update','--no-first-run','--disable-sync','--disable-features=OptimizationHints,MediaRouter'];

export async function run(command,args,options={}) {
  await new Promise((resolve,reject)=>{
    const child=spawn(command,args,{cwd:here,stdio:'inherit',windowsHide:true,...options});
    child.once('error',reject);child.once('exit',code=>code===0?resolve():reject(new Error(`${path.basename(command)} failed (${code})`)));
  });
}
export async function openBrowser() {
  if(!existsSync(chrome)) throw new Error('Install Chrome 129.0.6668.58, or set HYPERFRAMES_BROWSER_PATH to that version. No browser is downloaded during build.');
  if(!/^chrome-headless-shell(?:\.exe)?$/.test(path.basename(chrome))) throw new Error('Use the dedicated chrome-headless-shell binary.');
  const browser=await puppeteer.launch({executablePath:chrome,headless:true,args:browserArgs});
  const version=await browser.version();
  if(!version.endsWith('/129.0.6668.58')) {await browser.close();throw new Error('Reproduction requires Chrome 129.0.6668.58; see README.md.');}
  return browser;
}
export async function localPage(browser,width,height) {
  const page=await browser.newPage();
  await page.setViewport({width,height,deviceScaleFactor:1});
  await page.setRequestInterception(true);
  page.on('request',request=>{
    const url=new URL(request.url());
    if(['file:','data:','about:'].includes(url.protocol)||['127.0.0.1','localhost'].includes(url.hostname)) request.continue(); else request.abort('blockedbyclient');
  });
  return page;
}

async function main() {
  if(process.versions.node!=='22.23.1') throw new Error('Reproduction requires Node 22.23.1.');
  const ffmpegVersion=execFileSync('ffmpeg',['-version'],{encoding:'utf8',windowsHide:true}).split(/\r?\n/)[0];
  if(!ffmpegVersion.includes('2024-04-10-git-0e4dfa4709')) throw new Error('Reproduction requires FFmpeg 2024-04-10-git-0e4dfa4709.');
  await mkdir(output,{recursive:true});await mkdir(path.join(here,'.build'),{recursive:true});
  const generated=[];
  async function asset(name,body) {
    const result=optimize(body,{multipass:true,plugins:[{name:'preset-default',params:{overrides:{removeDesc:false,cleanupIds:false,removeUnknownsAndDefaults:{keepRoleAttr:true}}}}]});
    await writeFile(path.join(output,name),result.data+'\n');generated.push(name);
  }
  const data=loadContract(root);
  const rust=await readFile(path.join(root,'rust-toolchain.toml'),'utf8');
  const rustVersion=rust.match(/channel = "([^"]+)"/)[1];
  for(const [name,label] of [['code','Code: MIT OR Apache-2.0'],['docs','Docs: CC BY 4.0'],['rust','Rust '+rustVersion],['mcp','MCP / stdio'],['status','Research prototype'],['platform','Windows 11 x64']]) {
    const width=Math.round(label.length*6.7+24);
    await asset(`badge-${name}.svg`,svg(width,24,label,label,`<rect width="${width}" height="24" rx="5" fill="#142b3f"/><text x="12" y="16" font-size="11" fill="#f0f6fc" font-weight="600">${label}</text>`));
  }
  for(const theme of ['light','dark']) for(const mobile of [false,true]) {
    const suffix=`${mobile?'mobile-':''}${theme}.svg`;
    for(const [name,make] of [['hero',hero],['pipeline',pipeline],['results',results],['gate',gate]]) await asset(`${name}-${suffix}`,make(theme,mobile));
  }
  for(const mobile of [false,true]) await asset(`protocol-loop${mobile?'-mobile':''}.svg`,loop(data,mobile));
  for(const mobile of [false,true]) await asset(`protocol-still${mobile?'-mobile':''}.svg`,loop(data,mobile).replace('</style>','.step{animation:none!important;opacity:0}.step0{opacity:1}</style>'));
  await writeFile(path.join(here,'social-preview.svg'),social());
  await writeFile(path.join(here,'.build/index.html'),composition(data));
  await writeFile(path.join(here,'.build/hyperframes.json'),JSON.stringify({name:'ommatix-protocol'},null,2)+'\n');
  const browser=await openBrowser();
  try {
    const page=await localPage(browser,1280,720);
    await page.goto(pathToFileURL(path.join(here,'.build/index.html')).href);
    await page.evaluate(()=>window.draw(15));
    await page.screenshot({path:path.join(output,'protocol-poster.png')});generated.push('protocol-poster.png');
    await page.setViewport({width:1280,height:640,deviceScaleFactor:1});
    await page.goto(pathToFileURL(path.join(here,'social-preview.svg')).href);
    await page.screenshot({path:path.join(output,'social-preview.png')});generated.push('social-preview.png');
    for(const mobile of [false,true]) {
      const kind=mobile?'mobile':'desktop';
      await page.setViewport({width:mobile?600:1200,height:mobile?336:224,deviceScaleFactor:1});
      await page.emulateMediaFeatures([{name:'prefers-reduced-motion',value:'no-preference'}]);
      await page.goto(pathToFileURL(path.join(output,`protocol-loop${mobile?'-mobile':''}.svg`)).href);
      for(let index=0;index<6;index++) {
        await page.evaluate(time=>document.getAnimations().forEach(animation=>{animation.pause();animation.currentTime=time;}),index*5000+250);
        await page.screenshot({path:path.join(here,`.build/loop-${kind}-${index}.png`)});
      }
      const name=`protocol-loop${mobile?'-mobile':''}.gif`;
      await run('ffmpeg',['-y','-loglevel','error','-framerate','1/5','-i',`.build/loop-${kind}-%d.png`,'-filter_complex','split[a][b];[a]palettegen=max_colors=128:reserve_transparent=0[p];[b][p]paletteuse=dither=bayer','-loop','0','-threads','1',path.join(output,name)]);generated.push(name);
    }
  } finally {await browser.close();}
  const env={...process.env,HYPERFRAMES_BROWSER_PATH:chrome,HYPERFRAMES_FFPROBE_PATH:ffprobe.path,HYPERFRAMES_NO_TELEMETRY:'1',DO_NOT_TRACK:'1',HYPERFRAMES_NO_UPDATE_CHECK:'1',HYPERFRAMES_NO_AUTO_INSTALL:'1',CI:'1'};
  await run(process.execPath,['--import',pathToFileURL(path.join(here,'offline.mjs')).href,path.join(here,'node_modules/hyperframes/bin/hyperframes.mjs'),'render','.build','--output','.build/protocol-raw.mp4','--fps','24','--workers','1','--quality','standard','--crf','24','--no-page-side-compositing','--experimental-fast-capture=false'],{env});
  await run('ffmpeg',['-y','-loglevel','error','-i','.build/protocol-raw.mp4','-map_metadata','-1','-fflags','+bitexact','-flags:v','+bitexact','-c:v','libx264','-preset','slow','-crf','24','-pix_fmt','yuv420p','-threads','1','-an','-movflags','+faststart',path.join(output,'protocol-demo.mp4')]);generated.push('protocol-demo.mp4');
  let binaryTotal=0;
  const manifest={schema:1,inputs:{contract:'docs/thesis/contratos/ejemplos',results:'docs/thesis/09_EVALUACION_EXPERIMENTAL.md#911',source_commit:'875495fad62cf69e4729a548f4e2186310f31943'},runtime:{node:'22.23.1',chrome:'129.0.6668.58',hyperframes:'0.8.178',ffmpeg:'2024-04-10-git-0e4dfa4709'},assets:{}};
  for(const name of generated.sort()) {
    const bytes=await readFile(path.join(output,name)),size=(await stat(path.join(output,name))).size;
    manifest.assets[name]={bytes:size,sha256:createHash('sha256').update(bytes).digest('hex')};
    if(/\.(png|mp4|gif)$/.test(name)) {binaryTotal+=size;if(size>5_000_000) throw new Error(`${name} exceeds 5 MB`);}
  }
  if(binaryTotal>12_000_000) throw new Error('The binaries exceed 12 MB in total');
  if(manifest.assets['social-preview.png'].bytes>=1_000_000) throw new Error('The social preview exceeds 1 MB');
  await writeFile(path.join(output,'manifest.json'),JSON.stringify(manifest,null,2)+'\n');
  console.log(`Generated ${generated.length} assets; binary total ${binaryTotal} bytes.`);
}
if(process.argv[1]&&path.resolve(process.argv[1])===fileURLToPath(import.meta.url)) await main();
