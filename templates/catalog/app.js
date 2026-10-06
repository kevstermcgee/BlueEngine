(function () {
  'use strict';
  const grid=document.getElementById('grid'),empty=document.getElementById('empty'),search=document.getElementById('search'),sort=document.getElementById('sort');
  const cards=Array.from(grid.children),kindButtons=Array.from(document.querySelectorAll('.kinds button'));
  const filters=['presentation','distribution','networking'].map(id=>document.getElementById(id));
  const storageKey='blueengine:library:favorites:v1';let kind='',stars=new Set();
  const status=document.getElementById('favorites-status');
  function notice(message){if(status)status.textContent=message;}
  function load(){try{const values=JSON.parse(localStorage.getItem(storageKey)||'[]');if(!Array.isArray(values)||values.some(v=>typeof v!=='string'))throw new Error('Invalid favorites');stars=new Set(values);}catch(e){notice('Favorites storage unavailable. Stars will remain for this session.');}}
  const sorters={newest:(a,b)=>b.dataset.created.localeCompare(a.dataset.created)||a.dataset.name.localeCompare(b.dataset.name),name:(a,b)=>a.dataset.name.localeCompare(b.dataset.name),size:(a,b)=>Number(a.dataset.size)-Number(b.dataset.size)};
  sorters.oldest=(a,b)=>-sorters.newest(a,b);
  function apply(){
    const q=search.value.trim().toLowerCase();let shown=0;
    cards.sort((a,b)=>Number(stars.has(b.dataset.id))-Number(stars.has(a.dataset.id))+(stars.has(b.dataset.id)===stars.has(a.dataset.id)?(sorters[sort.value]||sorters.newest)(a,b):0));
    for(const card of cards){
      const presentation=filters[0]?.value||'',distribution=filters[1]?.value||'',network=filters[2]?.value||'';
      const show=(!kind||card.dataset.kind===kind)&&(!q||card.dataset.text.includes(q))&&(!presentation||card.dataset.presentation===presentation)&&(!distribution||card.dataset[distribution]==='true')&&(!network||card.dataset.networking===network);
      card.hidden=!show;if(show)shown++;grid.appendChild(card);
      const button=card.querySelector('[data-star]');if(button){const starred=stars.has(card.dataset.id);button.setAttribute('aria-pressed',String(starred));button.setAttribute('aria-label',(starred?'Unstar ':'Star ')+card.dataset.name);}
    }
    empty.hidden=shown>0;
  }
  for(const card of cards)card.querySelector('[data-star]')?.addEventListener('click',()=>{
    const id=card.dataset.id;if(stars.has(id))stars.delete(id);else stars.add(id);
    try{localStorage.setItem(storageKey,JSON.stringify([...stars].sort()));notice('Favorites saved on this device.');}catch(e){notice('Favorites could not be saved. Stars will remain for this session.');}apply();
  });
  search.addEventListener('input',apply);sort.addEventListener('change',apply);filters.forEach(f=>f?.addEventListener('change',apply));
  kindButtons.forEach(btn=>btn.addEventListener('click',()=>{kind=btn.dataset.kind;kindButtons.forEach(b=>b.classList.toggle('active',b===btn));apply();}));
  window.addEventListener('storage',e=>{if(e.key===storageKey){load();apply();}});
  load();apply();
})();
