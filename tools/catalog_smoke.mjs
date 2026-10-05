// Exercise the shipped unified feed, favorites and filters in actual Chromium.
import {createRequire} from 'node:module';
import {spawn,spawnSync} from 'node:child_process';
import {mkdtemp,writeFile,rm} from 'node:fs/promises';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
const WebSocket=createRequire(import.meta.url)('ws');
const [url,reportPath,screenshotPath]=process.argv.slice(2);
const profile=await mkdtemp(join(tmpdir(),'be2-feed-'));
const exe=process.env.BE2_CHROMIUM||['chromium','google-chrome'].find(n=>spawnSync(n,['--version']).status===0);
const browser=spawn(exe,['--headless','--no-sandbox','--remote-debugging-port=0',`--user-data-dir=${profile}`,'about:blank']);
let socket,log='',errors=[];const deadline=setTimeout(()=>browser.kill('SIGKILL'),90000);
try{
 const endpoint=await new Promise((ok,bad)=>{browser.stderr.on('data',b=>{log+=b;const m=log.match(/DevTools listening on (ws:\/\/\S+)/);if(m)ok(m[1]);});browser.on('error',bad);browser.on('exit',c=>bad(new Error('Chrome exited '+c)));});
 socket=new WebSocket(endpoint);await new Promise(ok=>socket.on('open',ok));let seq=0,session;const pending=new Map();
 socket.on('message',b=>{const m=JSON.parse(b);if(m.id){const p=pending.get(m.id);pending.delete(m.id);m.error?p.bad(new Error(JSON.stringify(m.error))):p.ok(m.result);}if(m.method==='Runtime.exceptionThrown')errors.push(m.params.exceptionDetails);});
 const send=(method,params={},attach=true)=>new Promise((ok,bad)=>{const id=++seq;pending.set(id,{ok,bad});socket.send(JSON.stringify({id,method,params,...(attach&&session?{sessionId:session}:{})}));});
 const target=await send('Target.createTarget',{url:'about:blank'},false);session=(await send('Target.attachToTarget',{targetId:target.targetId,flatten:true},false)).sessionId;
 for(const method of ['Runtime.enable','Page.enable'])await send(method);
 const evaluate=async expression=>{const r=await send('Runtime.evaluate',{expression,returnByValue:true,awaitPromise:true});if(r.exceptionDetails)throw new Error(JSON.stringify(r.exceptionDetails));return r.result.value;};
 const wait=async expression=>{for(let i=0;i<200;i++){const r=await evaluate(expression);if(r)return r;await new Promise(ok=>setTimeout(ok,100));}throw new Error('Timeout '+expression);};
 await send('Page.navigate',{url});await wait('document.querySelectorAll("[data-star]").length>0');
 const initial=await evaluate('Array.from(document.querySelectorAll("#grid .card")).map(c=>({id:c.dataset.id,browser:c.dataset.browser,native:c.dataset.native}))');
 if(new Set(initial.map(g=>g.id)).size!==initial.length)throw new Error('Duplicate game IDs in unified feed');
 if(!initial.some(g=>g.browser==='true')||!initial.some(g=>g.native==='true'))throw new Error('Feed must include browser and native games together');
 const chosen=initial.at(-1).id;
 await evaluate(`document.querySelector('[data-star="${chosen}"]').click()`);
 if(await evaluate('document.querySelector("#grid .card").dataset.id')!==chosen)throw new Error('Star did not pin game first');
 await send('Page.reload');await wait('document.querySelectorAll("[data-star]").length>0');
 if(await evaluate(`document.querySelector('[data-star="${chosen}"]').getAttribute('aria-pressed')`)!=='true'||await evaluate('document.querySelector("#grid .card").dataset.id')!==chosen)throw new Error('Favorite did not persist/pin after reload');
 await evaluate('document.getElementById("distribution").value="browser";document.getElementById("distribution").dispatchEvent(new Event("change"))');
 if(!await evaluate('Array.from(document.querySelectorAll("#grid .card")).filter(c=>getComputedStyle(c).display!=="none").every(c=>c.dataset.browser==="true")'))throw new Error('Browser filter did not hide native-only cards');
 await evaluate('document.getElementById("distribution").value="";document.getElementById("presentation").value="hybrid";document.getElementById("presentation").dispatchEvent(new Event("change"))');
 if(!await evaluate('Array.from(document.querySelectorAll("#grid .card")).filter(c=>getComputedStyle(c).display!=="none").every(c=>c.dataset.presentation==="hybrid")'))throw new Error('Presentation filter failed');
 await evaluate('document.getElementById("presentation").value="";document.getElementById("presentation").dispatchEvent(new Event("change"))');
 await evaluate(`Storage.prototype.setItem=()=>{throw new Error('Denied')};document.querySelector('[data-star="${chosen}"]').click()`);
 if(!await evaluate('document.getElementById("favorites-status").textContent.includes("could not")'))throw new Error('Storage failure was hidden');
 await send('Page.reload');await wait('document.querySelectorAll("[data-star]").length>0');
 if(await evaluate(`document.querySelector('[data-star="${chosen}"]').getAttribute('aria-pressed')`)!=='true')throw new Error('Failed favorite write destroyed previous value');
 await send('Emulation.setDeviceMetricsOverride',{width:390,height:844,deviceScaleFactor:2,mobile:true});await send('Emulation.setTouchEmulationEnabled',{enabled:true});
 if(await evaluate('document.documentElement.scrollWidth>innerWidth'))throw new Error('Mobile feed overflows horizontally');
 const screen=await send('Page.captureScreenshot',{format:'png',captureBeyondViewport:true});await writeFile(screenshotPath,Buffer.from(screen.data,'base64'));
 if(errors.length)throw new Error(JSON.stringify(errors));
 const report={ok:true,games:initial.length,browser_games:initial.filter(g=>g.browser==='true').length,native_games:initial.filter(g=>g.native==='true').length,unique_ids:true,favorite_pins:true,favorite_reload:true,failed_write_preserves_value:true,filters:true,mobile_layout:true,errors};
 await writeFile(reportPath,JSON.stringify(report,null,2));console.log(JSON.stringify(report));
}catch(e){await writeFile(reportPath,JSON.stringify({ok:false,error:String(e),errors}));process.exitCode=1;console.error(e);}
finally{clearTimeout(deadline);socket?.close();browser.kill();await new Promise(ok=>setTimeout(ok,300));await rm(profile,{recursive:true,force:true});}
