// Real Chromium/CDP verification, with ws as the only Node dependency. No gameplay implementation here.
import {createRequire} from 'node:module';
import {spawn,spawnSync} from 'node:child_process';
import {mkdtemp,readFile,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const require=createRequire(import.meta.url);
const WebSocket=require('ws');
const [url,reportPath,screenshotPath]=process.argv.slice(2);
const profile=await mkdtemp(join(tmpdir(),'be2-browser-'));
const executable=process.env.BE2_CHROMIUM||['chromium','chromium-browser','google-chrome','google-chrome-stable'].find(name=>spawnSync(name,['--version']).status===0);
if(!executable)throw new Error('Browser verification needs Chromium/Chrome on PATH or BE2_CHROMIUM=/path/to/chrome');
const browser=spawn(executable,['--headless','--no-sandbox','--enable-unsafe-swiftshader','--use-gl=angle','--use-angle=swiftshader','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank'],{stdio:['ignore','ignore','pipe']});
let socket;
let errors=[];let requests=[];let browserLog='';
const timeout=setTimeout(()=>{browser.kill('SIGKILL');console.error('Browser smoke timed out');process.exit(1);},180000);
try {
  const endpoint=await new Promise((resolve,reject)=>{
    browser.stderr.on('data',chunk=>{browserLog+=chunk;const match=browserLog.match(/DevTools listening on (ws:\/\/\S+)/);if(match)resolve(match[1]);});
    browser.on('error',reject);browser.on('exit',code=>reject(new Error(`Chromium exited ${code}: ${browserLog.slice(-1000)}`)));
  });
  socket=new WebSocket(endpoint);await new Promise((resolve,reject)=>{socket.on('open',resolve);socket.on('error',reject);});
  let sequence=0,session;const pending=new Map();
  socket.on('message',data=>{
    const message=JSON.parse(data);
    if(message.id){const entry=pending.get(message.id);pending.delete(message.id);message.error?entry.reject(new Error(JSON.stringify(message.error))):entry.resolve(message.result);}
    if(message.method==='Runtime.exceptionThrown')errors.push(message.params.exceptionDetails.text+JSON.stringify(message.params.exceptionDetails.exception));
    if(message.method==='Runtime.consoleAPICalled'&&message.params.type==='error')errors.push(message.params.args.map(a=>a.value||a.description).join(' '));
    if(message.method==='Network.responseReceived'){const r=message.params.response;requests.push({url:r.url,status:r.status});if(r.status>=400)errors.push(`HTTP ${r.status} ${r.url}`);}
    if(message.method==='Network.loadingFailed')errors.push(JSON.stringify(message.params));
  });
  const send=(method,params={},useSession=true)=>new Promise((resolve,reject)=>{const id=++sequence;pending.set(id,{resolve,reject});socket.send(JSON.stringify({id,method,params,...(useSession&&session?{sessionId:session}:{})}));});
  const target=await send('Target.createTarget',{url:'about:blank'},false);
  session=(await send('Target.attachToTarget',{targetId:target.targetId,flatten:true},false)).sessionId;
  for(const method of ['Runtime.enable','Page.enable','Network.enable'])await send(method);
  const evaluate=async expression=>{const result=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(result.exceptionDetails)throw new Error(JSON.stringify(result.exceptionDetails));return result.result.value;};
  const wait=async expression=>{for(let i=0;i<900;i++){const result=await evaluate(expression);if(errors.length)throw new Error(errors.join('\n'));if(result)return result;await new Promise(r=>setTimeout(r,100));}throw new Error(`Timeout waiting for ${expression}`);};
  const key=async (key,code,vk)=>{await send('Input.dispatchKeyEvent',{type:'keyDown',key,code,windowsVirtualKeyCode:vk});await new Promise(r=>setTimeout(r,100));await send('Input.dispatchKeyEvent',{type:'keyUp',key,code,windowsVirtualKeyCode:vk});};
  await send('Page.navigate',{url:url+'?verify=1'});
  await writeFile(reportPath,JSON.stringify({ok:false,stage:'waiting for WASM ready',errors}));await wait('window.be2?.ready');
  await key('Enter','Enter',13);
  await wait('be2.started');
  const verified=await wait('be2.verified && JSON.parse(JSON.stringify(be2))');
  const metadata=await evaluate('fetch("manifest.json").then(r=>r.json())');
  if(verified.hash!==metadata.verification.hash||verified.outcome!==metadata.verification.outcome)throw new Error(`Browser/native mismatch: ${JSON.stringify(verified)} expected ${JSON.stringify(metadata.verification)}`);
  if(!verified.audio.activated||verified.audio.loaded!==3||verified.audio.submitted<1)throw new Error('No audio activation/decode/play submission evidence');
  const audio=await evaluate('({states:be2Audio.contexts.map(c=>c.state),starts:be2Audio.starts})');
  if(!audio.states.length||audio.states.some(s=>s!=='running')||audio.starts<1)throw new Error('Audio buffers never reached a running playback context');
  const screen=await send('Page.captureScreenshot',{format:'png'});await writeFile(screenshotPath,Buffer.from(screen.data,'base64'));
  await key('k','KeyK',75);await wait('be2.notice.includes("saved")');
  const saved=await evaluate('be2.hash');
  await key('m','KeyM',77);await wait('!be2.sound');
  await send('Page.reload');await writeFile(reportPath,JSON.stringify({ok:false,stage:'waiting for WASM ready',errors}));await wait('window.be2?.ready');
  if(await evaluate('be2.sound')!==false)throw new Error('Settings did not survive reload');
  await key('l','KeyL',76);await wait('be2.notice.includes("resumed")');
  if(await evaluate('be2.hash')!==saved)throw new Error('Save did not survive reload exactly');
  await evaluate('window.be2BlockStorage=true');await key('k','KeyK',75);
  await wait('be2.notice.includes("failed")');
  await evaluate('window.be2BlockStorage=false');await key('l','KeyL',76);await wait('be2.notice.includes("resumed")');
  if(await evaluate('be2.hash')!==saved)throw new Error('Blocked write destroyed the previous save');
  // Real controls in normal mode, independent of verification_input.
  await send('Page.navigate',{url});await writeFile(reportPath,JSON.stringify({ok:false,stage:'waiting for WASM ready',errors}));await wait('window.be2?.ready');await key('Enter','Enter',13);await key('Escape','Escape',27);await wait('be2.paused');
  await key('r','KeyR',82);await key('Escape','Escape',27);
  const probe=await evaluate('be2.probe');
  let mousePoint;const held=[];
  if(probe.pointer){
    mousePoint=await evaluate(`(()=>{const r=document.querySelector('canvas').getBoundingClientRect();const scale=Math.min(r.width/800,r.height/450);return {x:r.x+(r.width-800*scale)/2+${probe.pointer.x}*scale,y:r.y+(r.height-450*scale)/2+${probe.pointer.y}*scale};})()`);
    await send('Input.dispatchMouseEvent',{type:'mouseMoved',...mousePoint});
  }
  if(probe.x)held.push([probe.x>0?'ArrowRight':'ArrowLeft',probe.x>0?39:37]);
  if(probe.y)held.push([probe.y>0?'ArrowDown':'ArrowUp',probe.y>0?40:38]);
  if(probe.action&&!probe.pointer)held.push(['Space',32]);
  for(const [code,vkey] of held)await send('Input.dispatchKeyEvent',{type:'keyDown',key:code==='Space'?' ':code,code,windowsVirtualKeyCode:vkey});
  if(probe.action&&mousePoint)await send('Input.dispatchMouseEvent',{type:'mousePressed',...mousePoint,button:'left',clickCount:1});
  await new Promise(r=>setTimeout(r,100));
  for(const [code,vkey] of held)await send('Input.dispatchKeyEvent',{type:'keyUp',key:code==='Space'?' ':code,code,windowsVirtualKeyCode:vkey});
  if(probe.action&&mousePoint)await send('Input.dispatchMouseEvent',{type:'mouseReleased',...mousePoint,button:'left',clickCount:1});
  await wait('be2.probe_passed');
  const playScreen=await send('Page.captureScreenshot',{format:'png'});await writeFile(screenshotPath.replace(/\.png$/, '-playing.png'),Buffer.from(playScreen.data,'base64'));
  const keyboard=await evaluate(`(()=>{const e=new KeyboardEvent('keydown',{code:'Tab',key:'Tab',cancelable:true});document.querySelector('canvas').dispatchEvent(e);return !e.defaultPrevented;})()`);
  if(!keyboard)throw new Error('Tab navigation was consumed by the game');
  const canvas=await evaluate('({width:document.querySelector("canvas").width,height:document.querySelector("canvas").height})');
  if(canvas.width<100||canvas.height<100||errors.length)throw new Error(`Browser errors: ${JSON.stringify(errors)}`);
  const packageBase=new URL(url),declared=new Set(['','manifest.json',...Object.keys(metadata.file_sha256)]);
  for(const request of requests){
    const resource=new URL(request.url);
    if(resource.protocol==='http:'||resource.protocol==='https:'){
      if(resource.origin!==packageBase.origin||!resource.pathname.startsWith(packageBase.pathname)||!declared.has(decodeURIComponent(resource.pathname.slice(packageBase.pathname.length))))throw new Error(`Undeclared runtime dependency: ${resource}; embed it or declare its relative path in identity.package`);
    }
  }
  const report={ok:true,verified,native_expected:metadata.verification,audio,persistence:'save and settings survive reload; blocked write fails explicitly and retains save',real_input:{probe,meaningful_result:true,start_pause_restart:true,tab_navigation_preserved:true},canvas,requests,errors,screenshot:screenshotPath,browser:'Chromium CDP/software WebGL; no human listening or physical controller test'};
  await writeFile(reportPath,JSON.stringify(report,null,2));console.log(JSON.stringify({ok:true,report:reportPath,hash:verified.hash}));
} catch(error) {await writeFile(reportPath,JSON.stringify({ok:false,error:String(error),errors,requests,browserLog:browserLog.slice(-4000)},null,2));console.error(error);process.exitCode=1;}
finally {clearTimeout(timeout);socket?.close();browser.kill();await new Promise(r=>setTimeout(r,500));await rm(profile,{recursive:true,force:true});}
