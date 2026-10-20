import {readFileSync} from 'node:fs';
import path from 'node:path';
import {fileURLToPath} from 'node:url';

const here=path.dirname(fileURLToPath(import.meta.url));
export const demo={width:1280,height:720,fps:30,duration:44};

// The scene reads only fields of the dossier's contract examples.
export function demoData(root) {
  const read=(name)=>JSON.parse(readFileSync(path.join(root,'docs/thesis/contratos/ejemplos',name),'utf8'));
  const observe=read('observe_output.json'),click=read('click_input.json'),stale=read('stale_output.json'),sim=read('simulated_output.json');
  return {elements:observe.elements.map(({target_id,revision,role,label,actionability})=>({target_id,revision,role,label,actionability})),click,stale,sim};
}

export function demoComposition(data) {
  const scene=readFileSync(path.join(here,'demo/scene.js'),'utf8');
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'"><title>Ommatix protocol demo</title><style>*{box-sizing:border-box}html,body{margin:0;width:${demo.width}px;height:${demo.height}px;overflow:hidden;background:#0a1520}#root{width:${demo.width}px;height:${demo.height}px}</style><body><div id="root" data-composition-id="protocol" data-start="0" data-duration="${demo.duration}" data-width="${demo.width}" data-height="${demo.height}" data-fps="${demo.fps}"></div><script>window.DATA=${JSON.stringify(data).replaceAll('<','\\u003c')};</script><script>${scene}</script></body></html>`;
}
