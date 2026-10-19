import {beats,font,videoScene} from './artwork.mjs';

export function composition(data) {
  return `<!doctype html><html lang="en"><meta charset="utf-8"><meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; connect-src 'none'"><title>Ommatix protocol illustration</title><style>*{box-sizing:border-box}html,body{margin:0;width:1280px;height:720px;overflow:hidden;background:#0d1b2a}text{font-family:${font}}#root{width:1280px;height:720px}</style><body><div id="root" data-composition-id="protocol" data-start="0" data-duration="32" data-width="1280" data-height="720" data-fps="24"></div><script>
  const data=${JSON.stringify(data)};
  const beats=${JSON.stringify(beats)};
  const esc=${String((v)=>String(v).replaceAll('&','&amp;').replaceAll('<','&lt;').replaceAll('>','&gt;').replaceAll('"','&quot;'))};
  const font=${JSON.stringify(font)};
  const mono='ui-monospace, Consolas, monospace';
  const palette={dark:{bg:'#0d1b2a',surface:'#142b3f',ink:'#f0f6fc',muted:'#adbdce',line:'#3c5369',accent:'#68e1ce',tint:'#163e42',amber:'#ffd18a'}};
  const text=(x,y,value,size=24,fill='currentColor',extra='')=>'<text x="'+x+'" y="'+y+'" font-size="'+size+'" fill="'+fill+'" '+extra+'>'+esc(value)+'</text>';
  const rect=(x,y,w,h,fill,stroke='none',r=16,extra='')=>'<rect x="'+x+'" y="'+y+'" width="'+w+'" height="'+h+'" rx="'+r+'" fill="'+fill+'" stroke="'+stroke+'" '+extra+'/>';
  const arrow=(x,y,fill)=>'<path d="M'+x+' '+y+'h24m-8-7 8 7-8 7" fill="none" stroke="'+fill+'" stroke-width="2.5"/>';
  const videoScene=${String(videoScene)};
  function draw(time){const index=Math.max(0,beats.findIndex(b=>time>=b.at&&time<b.end));document.getElementById('root').innerHTML='<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 1280 720" width="1280" height="720">'+videoScene(time>=32?6:index,data,Math.min(1,time/32))+'</svg>';}
  window.draw=draw;
  // HyperFrames' seekable timeline interface; no clock, random source or animation library.
  let current=0;
  const timeline={duration:()=>32,totalDuration:()=>32,getChildren:()=>[],pause(){return this},paused:()=>true,seek(t){current=t;draw(t);return this},time(t){if(t===undefined)return current;return this.seek(t)},totalTime(t){if(t===undefined)return current;return this.seek(t)}};
  window.__timelines=window.__timelines||{};window.__timelines.protocol=timeline;draw(0);
  </script></body></html>`;
}
