/* Narrow browser ABI. No source paths, host credentials or application rules. */
window.be2 = {ready:false, errors:[], storage:"unknown"};
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
const gameCanvas=document.getElementById("glcanvas");
function activate() {gameCanvas.focus({preventScroll:true});for(const context of be2Audio.contexts) context.resume().catch(e=>be2.errors.push(String(e)));}
gameCanvas.addEventListener("pointerdown",activate);
gameCanvas.addEventListener("pointerup",activate);
gameCanvas.addEventListener("keydown",activate);
// Only focused game controls are consumed. Tab, Ctrl/Cmd shortcuts and ordinary page keys survive.
const keys=new Set(["ArrowUp","ArrowDown","ArrowLeft","ArrowRight","Space"]);
gameCanvas.addEventListener("keydown",e=>{if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&keys.has(e.code))e.preventDefault();});
gameCanvas.addEventListener("contextmenu",e=>e.preventDefault());
const decode=(ptr,len)=>new TextDecoder().decode(new Uint8Array(wasm_memory.buffer,ptr,len));
let lastPad=false;
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
  imports.env.be2_report=(ptr,len)=>{
    if(!be2.keyboardBound && gameCanvas.onkeydown){
      const keyDown=gameCanvas.onkeydown;
      const controls=new Set(["KeyW","KeyA","KeyS","KeyD","KeyR","KeyM","KeyK","KeyL","ArrowUp","ArrowDown","ArrowLeft","ArrowRight","Enter","Space","Escape"]);
      gameCanvas.onkeydown=e=>{if(!e.ctrlKey&&!e.metaKey&&!e.altKey&&controls.has(e.code))keyDown(e);};
      be2.keyboardBound=true;
    }
    Object.assign(be2,JSON.parse(decode(ptr,len)));
    const play=document.querySelector('[data-control="play"]');
    if(play){const label=!be2.started?'Play':be2.paused?'Resume':'Pause';play.setAttribute('aria-label',label);}
    document.getElementById('status').textContent=be2.notice||(!be2.started?'Start to play. Your progress stays on this device.':'Progress saves automatically on this device.');
  };
  imports.env.be2_error=(ptr,len)=>{const message=decode(ptr,len);be2.errors.push(message);document.getElementById("status").textContent=message;};
  imports.env.be2_verify=()=>new URLSearchParams(location.search).get("verify")==="1"?1:0;
  imports.env.be2_focused=()=>!document.hidden&&document.activeElement===gameCanvas?1:0;
  imports.env.be2_audio_active=()=>be2Audio.contexts.some(c=>c.state==="running")?1:0;
  imports.env.be2_pad=axis=>{
    const pad=Array.from(navigator.getGamepads?.()||[]).find(p=>p&&p.mapping==="standard");
    if(!pad)return 0;
    if(axis<2){const v=pad.axes[axis]||0;return Math.abs(v)>0.18?v:0;}
    const down=pad.buttons[0]?.pressed||false,edge=down&&!lastPad;lastPad=down;return edge?1:0;
  };
  imports.env.be2_touch=field=>{
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
