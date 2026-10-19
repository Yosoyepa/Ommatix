// Synthetic artwork only. Element text and protocol fields come from the dossier examples.
import {readFileSync} from 'node:fs';

export const palette = {
  light: {bg: '#f3f6fa', surface: '#ffffff', ink: '#132b40', muted: '#4f6375', line: '#cad6df', accent: '#006c65', tint: '#d8f3ed', amber: '#805400'},
  dark: {bg: '#0d1b2a', surface: '#142b3f', ink: '#f0f6fc', muted: '#adbdce', line: '#3c5369', accent: '#68e1ce', tint: '#163e42', amber: '#ffd18a'},
};
export const font = 'system-ui, -apple-system, BlinkMacSystemFont, Segoe UI, sans-serif';
export const mono = 'ui-monospace, Consolas, monospace';
export const esc = (text) => String(text).replaceAll('&', '&amp;').replaceAll('<', '&lt;').replaceAll('>', '&gt;').replaceAll('"', '&quot;');
export const text = (x, y, value, size=24, fill='currentColor', extra='') => `<text x="${x}" y="${y}" font-size="${size}" fill="${fill}" ${extra}>${esc(value)}</text>`;
export const rect = (x,y,w,h,fill,stroke='none',r=16,extra='') => `<rect x="${x}" y="${y}" width="${w}" height="${h}" rx="${r}" fill="${fill}" stroke="${stroke}" ${extra}/>`;
export function svg(w,h,title,description,body,p=palette.dark,style='') {
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${w} ${h}" width="${w}" height="${h}" role="img" aria-labelledby="title desc"><title id="title">${esc(title)}</title><desc id="desc">${esc(description)}</desc><style>text{font-family:${font}}${style}</style>${body}</svg>`;
}
const arrow = (x,y,fill) => `<path d="M${x} ${y}h24m-8-7 8 7-8 7" fill="none" stroke="${fill}" stroke-width="2.5"/>`;

export function hero(theme='dark',mobile=false) {
  const p=palette[theme], w=mobile?600:1200, h=mobile?304:224;
  let out=rect(0,0,w,h,p.bg,p.line,24);
  out+=text(40,56,'PERCEPTION / IDENTITY / CONTROL',16,p.accent,'letter-spacing="2" font-weight="650"');
  out+=text(40,112,'A stable interface',mobile?38:44,p.ink,'font-weight="700"');
  out+=text(40,166,'to a changing desktop.',mobile?38:44,p.ink,'font-weight="700"');
  if(mobile) {
    out+=rect(40,208,520,56,p.surface,p.line,12)+text(64,244,'changed region',22,p.muted)+arrow(248,236,p.accent)+text(304,244,'e7 · revision 3',22,p.accent);
  } else {
    for(let row=0;row<4;row++) for(let col=0;col<5;col++) out+=rect(688+col*40,40+row*40,32,32,(row===1&&col===2)||(row===2&&col===2)?p.accent:p.surface,p.line,6);
    out+=arrow(904,112,p.accent)+rect(952,64,208,96,p.surface,p.line,16);
    out+=text(976,103,'e7',28,p.ink,'font-weight="700"')+text(976,138,'revision 3',20,p.accent);
  }
  return svg(w,h,'Ommatix: a stable interface to a changing desktop','A grid highlights local changes; a structured element keeps its identifier and revision.',out,p);
}

export const stages = [
  ['Capture','frame + dirty rectangles'],['Region plan','committed base + halo'],['Detector + OCR','planned regions only'],['Spatial fusion','visual + optional UIA'],['Icon cache','curated; may abstain'],['Identity','session IDs + revisions'],['Atomic snapshot','publish together'],['LLM agent','reasoning via MCP'],['Action gate','policy + freshness'],['OS actuator','authorised input'],['Effect verifier','report uncertainty'],
];
export function pipeline(theme='dark',mobile=false) {
  const p=palette[theme], cols=mobile?2:4, cw=mobile?256:264, ch=128, gap=32, w=mobile?600:1200, rows=Math.ceil(stages.length/cols), h=112+rows*(ch+gap);
  let out=rect(0,0,w,h,p.bg,p.line,24)+text(32,48,'FROM A FRAME TO AN ACCOUNTABLE ACTION',mobile?19:23,p.ink,'font-weight="700"');
  out+=text(32,80,'Perception stays local. Reasoning stays with the agent.',mobile?17:20,p.muted);
  stages.forEach(([title,detail],i)=>{
    const row=Math.floor(i/cols), col=i%cols, reverse=row%2===1, x=24+(reverse?cols-1-col:col)*(cw+gap), y=112+row*(ch+gap);
    out+=rect(x,y,cw,ch,p.surface,p.line,16)+text(x+16,y+28,String(i+1).padStart(2,'0'),16,p.accent,'font-weight="700"');
    out+=text(x+16,y+64,title,23,p.ink,'font-weight="650"')+text(x+16,y+100,detail,17,p.muted);
    if(i<stages.length-1) {
      if(col===cols-1) out+=`<path d="M${x+cw/2} ${y+ch}v24m-7-7 7 7 7-7" fill="none" stroke="${p.accent}" stroke-width="2"/>`;
      else if(!reverse) out+=arrow(x+cw+4,y+ch/2,p.accent);
      else out+=`<path d="M${x-4} ${y+ch/2}h-24m8-7-8 7 8 7" fill="none" stroke="${p.accent}" stroke-width="2"/>`;
    }
  });
  return svg(w,h,'Ommatix observation and action pipeline','Capture, region planning, detector and OCR, spatial fusion, icon cache, identity, atomic snapshot, agent, action gate, actuator and effect verification. Numbered arrows show the order.',out,p);
}

export const scores = [
  {name:'Claude Haiku 5.5',dry:10,live:6},
  {name:'DeepSeek V4.1 Flash*',dry:11,live:7},
  {name:'MiMo V2.6 Flash',dry:10,live:7},
];
export function results(theme='dark',mobile=false) {
  const p=palette[theme], w=mobile?600:1200, h=mobile?808:488, chartX=mobile?32:440, barW=mobile?448:592;
  let out=rect(0,0,w,h,p.bg,p.line,24);
  out+=text(32,48,'AG-02 / PRELIMINARY',22,p.ink,'font-weight="700"')+text(32,80,'Scores out of 11 goals, including correct abstentions',mobile?17:21,p.muted);
  out+=rect(32,104,16,16,p.muted,'none',3)+text(56,120,'Phase 1 · dry run',18,p.muted)+rect(mobile?312:352,104,16,16,p.accent,'none',3)+text(mobile?336:376,120,'Phase 2 · live',18,p.muted);
  scores.forEach((s,i)=>{
    const y=mobile?168+i*184:168+i*72;
    out+=text(32,y+(mobile?0:26),s.name,mobile?24:26,p.ink,'font-weight="600"');
    const by=mobile?y+24:y;
    for(const [j,val,col] of [[0,s.dry,p.muted],[1,s.live,p.accent]]) {
      out+=rect(chartX,by+j*28,barW,16,p.surface,p.line,5)+rect(chartX,by+j*28,barW*val/11,16,col,'none',5);
      out+=text(chartX+barW+16,by+14+j*28,`${val}/11`,18,p.ink,'font-weight="650"');
    }
  });
  out+=text(32,h-72,'One series per model and phase · two synthetic apps · one machine',mobile?16:20,p.muted);
  out+=text(32,h-44,'Phase 1 scores choices; its decoy-goal verdict is exempt.',mobile?16:19,p.muted);
  out+=text(32,h-18,'* Version attributed to provider documentation; the run records an alias.',mobile?14:17,p.muted);
  return svg(w,h,'Preliminary AG-02 scores, not a model ranking','Phase 1: Haiku 10 of 11, DeepSeek 11 of 11, MiMo 10 of 11. Phase 2: 6, 7 and 7 of 11. Scores include abstentions. Phase 1 exempts the decoy verdict. One series per model and phase; no VLM baseline.',out,p);
}

export function gate(theme='dark',mobile=false) {
  const p=palette[theme], w=mobile?600:1200,h=mobile?488:240;
  let out=rect(0,0,w,h,p.bg,p.line,24)+text(32,48,'LIVE INPUT REQUIRES EVERY CHECK',mobile?22:24,p.ink,'font-weight="700"');
  const labels=[['Dry run?','yes → simulated'],['Policy','operator permits'],['Foreground','allowlist + scope'],['Freshness','snapshot + last look'],['Deliver','then verify']];
  labels.forEach(([a,b],i)=>{
    const x=mobile?32+(i%2)*280:24+i*232,y=mobile?88+Math.floor(i/2)*112:88;
    out+=rect(x,y,mobile?256:224,88,p.surface,p.line,12)+text(x+16,y+34,a,23,p.ink,'font-weight="650"')+text(x+16,y+65,b,mobile?17:16,p.muted);
    if(!mobile&&i<4) out+=arrow(x+224,y+44,p.accent);
  });
  out+=text(32,h-24,'Any failed check refuses input. Uncertainty never triggers an automatic retry.',mobile?16:20,p.accent);
  return svg(w,h,'Fail-closed action gate','Default dry run is simulated. Live input requires local policy, foreground attestation, a fresh snapshot and last look before delivery. The result reports delivery and effect separately.',out,p);
}

export function loadContract(root) {
  const path=root+'/docs/thesis/contratos/ejemplos/';
  return Object.fromEntries(['observe_output','click_input','simulated_output','stale_output'].map(name=>[name,JSON.parse(readFileSync(path+name+'.json','utf8'))]));
}
export const beats = [
  {at:0,end:4,heading:'A screenshot is a moment in time.',caption:'Repeated full-frame observations and coordinate actions still need temporal grounding.'},
  {at:4,end:9,heading:'Plan the regions that changed.',caption:'Compare against the committed base. A measured cost rule can select a full refresh.'},
  {at:9,end:14,heading:'Give the agent an element identity.',caption:'Geometry stays inside Ommatix. The agent receives IDs, revisions and actionability.'},
  {at:14,end:20,heading:'Observe. Choose. Ask to act.',caption:'The MCP contract binds an action to a session, snapshot, target and expected revision.'},
  {at:20,end:25,heading:'An old snapshot is refused.',caption:'STALE_SNAPSHOT: observe again. No input was delivered; the target is never silently replaced.'},
  {at:25,end:29,heading:'Report only what is known.',caption:'This example is a dry run: simulated, input_delivered false, verified false.'},
  {at:29,end:32,heading:'Ommatix',caption:'Spatial Indexing and Incremental Perception for GUI Agents'},
];

export function videoScene(index,data,progress=1) {
  const p=palette.dark, beat=beats[index], e=data.observe_output.elements[0];
  let out=rect(0,0,1280,720,p.bg)+text(48,48,'OMMATIX / PROTOCOL ILLUSTRATION',18,p.accent,'letter-spacing="2" font-weight="650"');
  out+=text(1232,48,`${String(index+1).padStart(2,'0')} / 07`,18,p.muted,'text-anchor="end"');
  out+=text(48,120,beat.heading,index===6?72:42,p.ink,'font-weight="700"');
  if(index<3) {
    out+=rect(48,176,704,368,p.surface,p.line,20)+rect(72,200,656,48,p.bg,'none',10)+text(96,232,data.observe_output.elements[1].label,21,p.muted);
    out+=rect(96,296,392,72,p.bg,p.line,10)+text(120,340,'Synthetic document',26,p.muted);
    out+=rect(400,424,296,72,p.tint,p.accent,12)+text(424,468,e.label,24,p.accent,'font-weight="650"');
    if(index===1) {
      for(let r=0;r<4;r++) for(let c=0;c<7;c++) out+=rect(72+c*94,256+r*66,88,60, r>1&&c>3?p.tint:'none',r>1&&c>3?p.accent:p.line,4,'opacity=".55"');
    }
    out+=rect(800,176,432,368,p.surface,p.line,20);
    const lines=index===0?['Full frame','Observe → reason → act','The desktop may change.']:index===1?['Changed regions','Committed base','+ pending dirty tiles','+ halo closure']:['target_id: '+e.target_id,'revision: '+e.revision,'role: '+e.role,'actionability: confirmed'];
    lines.forEach((line,i)=>out+=text(832,232+i*64,line,i===0?30:24,i===0?p.accent:p.ink,`font-family="${mono}"`));
  } else if(index===3) {
    out+=rect(48,176,512,368,p.surface,p.line,20)+text(80,224,'observe_screen → snapshot',24,p.accent,'font-weight="650"');
    ['session_id: "'+data.click_input.session_id+'"','snapshot_id: "'+data.click_input.snapshot_id+'"','target_id: "'+e.target_id+'"','revision: '+e.revision].forEach((line,i)=>out+=text(80,288+i*56,line,23,p.ink,`font-family="${mono}"`));
    out+=arrow(592,356,p.accent)+rect(656,176,576,368,p.surface,p.line,20)+text(688,224,'execute_action · excerpt',24,p.accent,'font-weight="650"');
    ['"snapshot_id": "'+data.click_input.snapshot_id+'",','"target_id": "'+data.click_input.target_id+'",','"expected_revision": '+data.click_input.expected_revision+',','"action": "'+data.click_input.action+'",','"dry_run": true'].forEach((line,i)=>out+=text(688,280+i*48,line,25,p.ink,`font-family="${mono}"`));
  } else if(index===4||index===5) {
    const stale=index===4,obj=stale?data.stale_output:data.simulated_output, color=stale?p.amber:p.accent;
    out+=rect(48,184,1184,360,p.surface,p.line,24)+text(80,240,stale?'snapshot_id: '+obj.snapshot_id+' → current: '+data.observe_output.snapshot_id:'request_id: '+obj.request_id,28,p.muted,`font-family="${mono}"`);
    out+=text(80,320,stale?obj.error.code:obj.status,48,color,'font-weight="700"');
    out+=text(80,392,'input_delivered: '+obj.input_delivered,30,p.ink,`font-family="${mono}"`)+text(80,448,'verified: '+obj.verified,30,p.ink,`font-family="${mono}"`);
    out+=text(80,508,stale?'next_step: '+obj.error.next_step:'A simulation does not claim delivery or an effect.',25,color);
  } else {
    out+=text(48,224,'Local perception. Explicit actions.',40,p.accent,'font-weight="650"');
    out+=text(48,312,'Incremental regions',32,p.ink)+text(48,376,'Stable session IDs + revisions',32,p.ink)+text(48,440,'Fail-closed actuation',32,p.ink);
    out+=rect(896,192,336,296,p.surface,p.line,24)+text(928,264,'e7',64,p.ink,'font-weight="700"')+text(928,328,'revision 3',30,p.accent)+text(928,416,'RESEARCH PROTOTYPE',18,p.muted,'letter-spacing="1"');
  }
  const words=beat.caption.split(' '), lines=[''];
  for(const word of words) { const n=lines.length-1; if((lines[n]+' '+word).trim().length>90) lines.push(word); else lines[n]+=(lines[n]?' ':'')+word; }
  lines.forEach((line,i)=>out+=text(48,592+i*32,line,24,p.ink));
  out+=text(48,684,'Stylised simulated desktop · dossier examples · no screen recording',18,p.muted);
  out+=rect(48,704,1184,4,p.line,'none',2)+rect(48,704,1184*progress,4,p.accent,'none',2);
  return out;
}

export function loop(data,mobile=false) {
  const p=palette.dark,w=mobile?600:1200,h=mobile?336:224;
  let out=rect(0,0,w,h,p.bg,p.line,24)+text(32,40,'SIMULATED DESKTOP / PROTOCOL ILLUSTRATION',mobile?17:18,p.muted);
  const labels=[['01 / REGION PLAN','Re-analyse changed regions'],['02 / IDENTITY','e7 · revision 3'],['03 / OBSERVE','observe_screen → s17'],['04 / ACT','execute_action(e7, revision 3)'],['05 / REFUSE','s16 → STALE_SNAPSHOT'],['06 / REPORT','simulated · no input delivered']];
  let css='';
  labels.forEach(([a,b],i)=>{
    let scene=text(32,mobile?104:96,a,20,p.accent,'font-weight="650" letter-spacing="1"')+text(32,mobile?152:148,b,mobile?25:32,p.ink,'font-weight="650"');
    const x=mobile?32:872,y=mobile?200:64;
    scene+=rect(x,y,mobile?536:296,80,p.surface,i===4?p.amber:p.line,16)+text(x+24,y+35,data.observe_output.elements[0].label,22,p.ink)+text(x+24,y+64,i===4?'refused · observe again':'e7  /  revision 3',19,i===4?p.amber:p.accent);
    out+=`<g class="step step${i}" opacity="${i===0?1:0}">${scene}</g>`;
    const start=i*100/6,end=(i+1)*100/6;
    css+=`.step${i}{animation:step${i} 30s steps(1,end) infinite}@keyframes step${i}{0%{opacity:${i===0?1:0}}${start>0?start+'%{opacity:1}':''}${end<100?end+'%{opacity:0}':''}100%{opacity:0}}`;
  });
  out+=text(32,h-24,'A schematic of the contract, not a live run or a performance measurement.',mobile?15:18,p.muted);
  css+='@media(prefers-reduced-motion:reduce){.step{animation:none;opacity:0}.step0{opacity:1}}';
  return svg(w,h,'Animated illustration of Ommatix on a simulated desktop','Cycles through region planning, stable identity, observation, a dry-run action, a stale snapshot refusal and honest reporting. Reduced-motion mode shows a still frame.',out,p,css);
}

export function social() {
  const p=palette.dark;
  const body=rect(0,0,1280,640,p.bg)+text(64,72,'RUST / MCP / RESEARCH PROTOTYPE',20,p.accent,'letter-spacing="3" font-weight="650"')+text(64,208,'Ommatix',104,p.ink,'font-weight="750"')+text(64,296,'A stable interface',52,p.ink,'font-weight="650"')+text(64,360,'to a changing desktop.',52,p.ink,'font-weight="650"')+text(64,472,'Spatial Indexing and Incremental Perception',28,p.muted)+text(64,516,'for GUI Agents',28,p.muted)+rect(904,160,312,312,p.surface,p.line,24)+text(936,240,'e7',72,p.ink,'font-weight="700"')+text(936,304,'revision 3',28,p.accent)+text(936,408,'Observe → act',24,p.muted)+text(64,596,'Element IDs · incremental perception · fail-closed actuation',24,p.accent);
  return svg(1280,640,'Ommatix social preview','Research prototype for incremental desktop perception and identifier-based MCP control.',body,p);
}
