/* Narrow browser ABI. No source paths, host credentials or application rules. */
window.be2 = {ready:false, errors:[], storage:"unknown"};
const instantiate=WebAssembly.instantiate.bind(WebAssembly);
WebAssembly.instantiate=async (...args)=>{const start=performance.now();const result=await instantiate(...args);be2.wasm_instantiated_at_ms=performance.now();be2.wasm_instantiation_ms=performance.now()-start;return result;};
window.be2Audio={contexts:[],starts:0};
window.addEventListener("error", e=>be2.errors.push(String(e.message)));
window.addEventListener("unhandledrejection",e=>be2.errors.push(String(e.reason)));
const NativeAudioContext=window.AudioContext;
window.AudioContext=class extends NativeAudioContext {
  constructor(...args) {
    super(...args);be2Audio.contexts.push(this);
    const make=this.createBufferSource.bind(this);
    this.createBufferSource=()=>{const source=make();const start=source.start.bind(source);source.start=(...args)=>{start(...args);be2Audio.starts++;};return source;};
  }
};
const controlContract=JSON.parse(document.getElementById("be2-control-config").textContent);
let keyboardCommands=0,keyboardPrimary=false,windowFocused=true;
const gameCanvas=document.getElementById("glcanvas");
function activate() {gameCanvas.focus({preventScroll:true});for(const context of be2Audio.contexts) context.resume().catch(e=>be2.errors.push(String(e)));}
gameCanvas.addEventListener("pointerdown",activate);
gameCanvas.addEventListener("pointerup",activate);
gameCanvas.addEventListener("keydown",activate);
gameCanvas.addEventListener('keydown',e=>{
  if(e.repeat||e.ctrlKey||e.metaKey||e.altKey)return;
  if(e.code===controlContract.primary.key)keyboardPrimary=true;
  for(const [name,c] of Object.entries(controlContract.commands))if(c.key===e.code)keyboardCommands|=name==='start'&&be2.started?controlContract.commands.pause.bit:c.bit;
});
// Miniquad retains pressed keys unless they are explicitly released on focus loss.
const heldKeys=new Map();
gameCanvas.addEventListener('keydown',e=>{if(!e.ctrlKey&&!e.metaKey&&!e.altKey)heldKeys.set(e.code,{key:e.key,code:e.code,keyCode:e.keyCode});});
gameCanvas.addEventListener('keyup',e=>heldKeys.delete(e.code));
function releaseInput(){for(const e of heldKeys.values())gameCanvas.dispatchEvent(new KeyboardEvent('keyup',e));heldKeys.clear();keyboardCommands=0;keyboardPrimary=false;}
gameCanvas.addEventListener('blur',releaseInput);
window.addEventListener('blur',()=>{windowFocused=false;releaseInput();});
window.addEventListener('focus',()=>{windowFocused=true;});
document.addEventListener('visibilitychange',()=>{if(document.hidden)releaseInput();});

// Only focused game controls are consumed. Tab, Ctrl/Cmd shortcuts and ordinary page keys survive.
const keys=new Set(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight","Space"]);
gameCanvas.addEventListener("keydown",e=>{if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&keys.has(e.code))e.preventDefault();});
gameCanvas.addEventListener("contextmenu",e=>e.preventDefault());
// Fullscreen must run synchronously inside a trusted gesture, never a later WASM frame.
const player=document.getElementById('player')||gameCanvas;
document.addEventListener('keydown',e=>{
  if(e.target?.closest?.('input,textarea,select,[contenteditable=true]'))return;
  if(e.code===controlContract.commands.fullscreen.key&&!e.repeat&&!e.ctrlKey&&!e.metaKey&&!e.altKey){
    e.preventDefault();activate();
    const request=document.fullscreenElement?document.exitFullscreen():player.requestFullscreen?.();
    if(request)request.catch(error=>{document.getElementById('status').textContent=`Fullscreen unavailable: ${error.message}`;});
    else document.getElementById('status').textContent='Fullscreen is unavailable in this browser. Try installing the game.';
  }
});
const decode=(ptr,len)=>new TextDecoder().decode(new Uint8Array(wasm_memory.buffer,ptr,len));
let padPrevious=new Set(),padFrame={axes:[0,0,0,0],action:false,commands:0};
function samplePad(){
  const pad=Array.from(navigator.getGamepads?.()||[]).find(p=>p&&p.mapping==="standard");
  const down=new Set((pad?.buttons||[]).flatMap((b,i)=>b.pressed?[i]:[]));
  const edge=i=>down.has(i)&&!padPrevious.has(i);
  let commands=0;
  for(const [name,c] of Object.entries(controlContract.commands))if(c.buttons.some(edge))commands|=name==="start"&&be2.started?controlContract.commands.pause.bit:c.bit;
  padFrame={axes:(pad?.axes||[0,0,0,0]).map(v=>Math.abs(v)>0.18?v:0),action:edge(controlContract.primary.button),commands};
  padPrevious=down;
  if(document.hidden||!windowFocused||document.activeElement!==gameCanvas)padFrame={axes:[0,0,0,0],action:false,commands:0};
}
function be2RegisterPlatform(){
miniquad_add_plugin({register_plugin:imports=>{
  imports.env.be2_storage_read=(ptr,len,out,cap)=>{
    try {
      if(window.be2BlockStorage)throw new Error("Storage blocked by verification");
      const value=localStorage.getItem(decode(ptr,len));be2.storage="available";
      if(value===null)return -1;
      if(value.length>8*1024*1024||value.length%2||!/^[0-9a-f]*$/.test(value))return -2;
      const count=value.length/2;
      if(cap===0)return count;
      if(count>cap)return -2;
      const bytes=new Uint8Array(wasm_memory.buffer,out,count);
      for(let i=0;i<count;i++)bytes[i]=parseInt(value.slice(2*i,2*i+2),16);
      return count;
    }catch(e){be2.storage="unavailable";return -2;}
  };
  imports.env.be2_storage_write=(ptr,len,data,count)=>{
    try{
      if(window.be2BlockStorage)throw new Error("Storage blocked by verification");
      const key=decode(ptr,len),bytes=new Uint8Array(wasm_memory.buffer,data,count);
      if(count>4*1024*1024)return -2;
      let value="";for(const byte of bytes)value+=byte.toString(16).padStart(2,"0");
      localStorage.setItem(key,value);be2.storage="available";return 0;
    }catch(e){be2.storage="unavailable";return -2;}
  };
  imports.env.be2_timestamp=()=>Date.now();
  imports.env.be2_clock=()=>performance.now();
  imports.env.be2_report=(ptr,len)=>{
    if(!be2.keyboardBound && gameCanvas.onkeydown){
      const keyDown=gameCanvas.onkeydown;
      const controls=new Set([...controlContract.movement.keys,controlContract.primary.key,"ShiftLeft","ShiftRight",...Object.values(controlContract.commands).map(c=>c.key)]);
      gameCanvas.onkeydown=e=>{if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&controls.has(e.code))keyDown(e);};
      be2.keyboardBound=true;
    }
    Object.assign(be2,JSON.parse(decode(ptr,len)));
    if(be2.ready&&!be2.playable_at_ms)be2.playable_at_ms=performance.now();
    be2.memory_bytes=wasm_memory.buffer.byteLength;
    const music=document.querySelector('[data-control="music"]');if(music)music.hidden=!(be2.music?.loaded>0);
    const play=document.querySelector('[data-control="play"]');
    if(play){const label=!be2.started?'Play':be2.paused?'Resume':'Pause';play.setAttribute('aria-label',label);}
    document.getElementById('status').textContent=be2.notice||(!be2.started?'Start to play. Your progress stays on this device.':'Progress saves automatically on this device.');
  };
  imports.env.be2_error=(ptr,len)=>{const message=decode(ptr,len);be2.errors.push(message);document.getElementById("status").textContent=message;};
  imports.env.be2_verify=()=>new URLSearchParams(location.search).get("verify")==="1"?1:0;
  imports.env.be2_focused=()=>!document.hidden&&windowFocused&&document.activeElement===gameCanvas?1:0;
  imports.env.be2_audio_active=()=>be2Audio.contexts.some(c=>c.state==="running")?1:0;
  imports.env.be2_keyboard=axis=>{
    if(axis===3){const edge=keyboardPrimary;keyboardPrimary=false;return Number(edge);}
    if(axis===2)return Number(controlContract.sprint.keys.some(k=>heldKeys.has(k)));
    let value=0;for(const [key,direction] of Object.entries(controlContract.movement.directions))if(heldKeys.has(key))value+=direction[axis];
    return Math.max(-1,Math.min(1,value));
  };
  imports.env.be2_pad=axis=>{
    if(axis===0)samplePad();
    if(axis<2)return padFrame.axes[axis]||0;
    if(axis===2)return Number(padFrame.action);
    if(axis===3||axis===4)return padFrame.axes[axis-1]||0;
    if(axis===5)return padFrame.commands;
    return 0;
  };
  imports.env.be2_touch=field=>{
    if(field===3){const c=keyboardCommands;keyboardCommands=0;const t=window.be2Touch;const touch=t?.visible?t.commands:0;if(t)t.commands=0;return c|touch;}
    const t=window.be2Touch;if(!t?.visible)return field===5||field===6?-1:0;
    if(field===0)return t.x;if(field===1)return t.y;
    if(field===2){const action=t.action;t.action=false;return Number(action);}
    if(field===3){const commands=t.commands;t.commands=0;return commands;}
    if(field===5)return t.pointer?.x??-1;if(field===6)return t.pointer?.y??-1;
    return 0;
  };
}});

// Macroquad's permissive missing-import stubs hide ABI mistakes. Supported packages fail explicitly.
add_missing_functions_stabs=function(module){
  for(const entry of WebAssembly.Module.imports(module)){
    if(entry.module!=="env" || !(entry.name in importObject.env))throw new Error(`Unsupported WASM import ${entry.module}.${entry.name}; rebuild with the matching loader/platform adapter`);
  }
};
}
