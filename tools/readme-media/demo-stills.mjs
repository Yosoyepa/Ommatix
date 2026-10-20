// Renders still frames of the demo for review: node demo-stills.mjs OUT_DIR t1 t2 ...
import {writeFile,mkdir} from 'node:fs/promises';
import path from 'node:path';
import {pathToFileURL} from 'node:url';
import {openBrowser,localPage,here,root} from './build.mjs';
import {demo,demoData,demoComposition} from './demo.mjs';

const [out,...times]=process.argv.slice(2);
await mkdir(out,{recursive:true});await mkdir(path.join(here,'.build'),{recursive:true});
const html=path.join(here,'.build/demo-stills.html');
await writeFile(html,demoComposition(demoData(root)));
const browser=await openBrowser();
try {
  const page=await localPage(browser,demo.width,demo.height);
  const errors=[];page.on('pageerror',e=>errors.push(e.message));
  await page.goto(pathToFileURL(html).href);
  for(const t of times) {
    await page.evaluate(v=>window.draw(v),Number(t));
    await page.screenshot({path:path.join(out,`t${String(t).padStart(5,'0')}.png`)});
  }
  if(errors.length) {console.error(errors.join('\n'));process.exitCode=1;}
} finally {await browser.close();}
