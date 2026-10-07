/* A complete, hash-checked version activates atomically. Saves are never stored in asset caches. */
const PREFIX='be2:'+self.registration.scope+':';
const ID='{{cache}}',CACHE=PREFIX+ID;
const HASHES={{hashes}};
const FILES=[...Object.keys(HASHES),'service-worker.js','manifest.json'];
const sha=async bytes=>Array.from(new Uint8Array(await crypto.subtle.digest('SHA-256',bytes)),b=>b.toString(16).padStart(2,'0')).join('');
self.addEventListener('install',event=>event.waitUntil((async()=>{
  try {
    const response=await fetch(new URL('manifest.json',self.registration.scope),{cache:'no-store'});
    if(!response.ok)throw new Error('Update manifest unavailable');
    const manifest=await response.clone().json();
    if(manifest.package_id!==ID)throw new Error('Incomplete deployment: manifest/worker version mismatch');
    for(const [name,digest] of Object.entries(HASHES))if(manifest.file_sha256[name]!==digest)throw new Error('Update manifest hash mismatch: '+name);
    const cache=await caches.open(CACHE);
    // No activate/claim/cache replacement until every file has passed its declared hash.
    for(const name of FILES){
      const asset=name==='manifest.json'?response:await fetch(new URL(name,self.registration.scope),{cache:'no-store'});
      if(!asset.ok)throw new Error('Incomplete update: '+name);
      if(name!=='manifest.json'&&await sha(await asset.clone().arrayBuffer())!==manifest.file_sha256[name])throw new Error('Corrupt update: '+name);
      await cache.put(name,asset);
    }
    await self.skipWaiting();
  } catch(error) {
    await caches.delete(CACHE); // failed candidate; the active version survives
    throw error;
  }
})()));
self.addEventListener('activate',event=>event.waitUntil((async()=>{
  const cache=await caches.open(CACHE);
  for(const name of FILES)if(!await cache.match(name))throw new Error('Candidate cache is incomplete');
  await Promise.all((await caches.keys()).filter(k=>k.startsWith(PREFIX)&&k!==CACHE).map(k=>caches.delete(k)));
  await self.clients.claim();
})()));
self.addEventListener('fetch',event=>{
  if(event.request.method!=='GET')return;
  const url=new URL(event.request.url);if(url.origin!==location.origin||!url.href.startsWith(self.registration.scope))return;
  const relative=decodeURIComponent(url.pathname.slice(new URL(self.registration.scope).pathname.length));
  if(relative&&!FILES.includes(relative))return;
  const name=relative||'index.html';
  event.respondWith(caches.open(CACHE).then(async cache=>{
    const cached=await cache.match(name);if(cached)return cached;
    return fetch(event.request); // recover online if the browser evicts cached assets
  }));
});
