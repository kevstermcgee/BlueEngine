/* Physical and digital devices feed the same Intent. This panel exists only on coarse-pointer devices. */
(() => {
  const commands=controlContract.commands;
  const config = JSON.parse(document.getElementById('be2-mobile-config').textContent);
  const panel = document.getElementById('mobile-controls');
  const canvas = document.getElementById('glcanvas');
  const mobile = matchMedia('(pointer: coarse)');
  const state = window.be2Touch = {x:0,y:0,action:false,commands:0,pointer:null,layout:config.layout,visible:false};
  const held = new Map();
  const clear = () => {held.clear();state.x=state.y=0;state.action=false;state.commands=0;state.pointer=null;};
  function visibility() {clear();panel.hidden=!mobile.matches;state.visible=mobile.matches;}
  mobile.addEventListener('change',visibility);
  window.addEventListener('blur',clear);
  document.addEventListener('visibilitychange',()=>{if(document.hidden)clear();});
  function button(label, name, handler, hold=false) {
    const b=document.createElement('button');b.type='button';b.textContent=label;b.dataset.control=name;b.setAttribute('aria-label',hold?'Move '+name:label);
    const release=e=>{held.delete(e.pointerId);update();};
    b.addEventListener('pointerdown',e=>{e.preventDefault();activate();b.setPointerCapture(e.pointerId);if(hold){held.set(e.pointerId,name);update();}if(name==='action')handler();});
    b.addEventListener('pointerup',()=>{activate();if(!hold&&name!=='action')handler();});
    for(const event of ['pointerup','pointercancel','lostpointercapture'])b.addEventListener(event,release);
    // Keyboard accessibility for the panel; pointer activation already happened on down.
    b.addEventListener('click',e=>{if(e.detail===0){activate();handler();}});
    return b;
  }
  function update(){const values=[...held.values()];state.x=Number(values.includes('right'))-Number(values.includes('left'));state.y=Number(values.includes('down'))-Number(values.includes('up'));}
  const play=document.createElement('div');play.className='digital-play';
  if(config.layout==='dpad') {
    const pad=document.createElement('div');pad.className='dpad';
    for(const [label,name] of [['↑','up'],['←','left'],['↓','down'],['→','right']])pad.append(button(label,name,()=>{},true));
    play.append(pad);
  } else if(config.layout==='paddle') {
    const slider=document.createElement('input');slider.type='range';slider.min='0';slider.max='799';slider.value='400';slider.dataset.control='paddle';slider.setAttribute('aria-label','Paddle position');
    const move=()=>{activate();state.pointer={x:Number(slider.value),y:365};};
    slider.addEventListener('pointerdown',move);slider.addEventListener('input',move);play.append(slider);
  } else {
    const hint=document.createElement('p');hint.textContent='Tap the game to select or place.';play.append(hint);
  }
  const face=document.createElement('div');face.className='face-buttons';
  const action=config.action_label!==null;
  const a=button('A',action?'action':'restart',()=>{if(action)state.action=true;else state.commands|=commands.restart.bit;});
  a.setAttribute('aria-label',action?(config.action_label||'Action'):'Restart');
  const b=button('B','pause',()=>{state.commands|=commands.pause.bit;});b.setAttribute('aria-label','Pause or resume');
  for(const [key,label] of [[b,'Pause'],[a,action?(config.action_label||'Action'):'Restart']]){
    const group=document.createElement('div');group.append(key);const caption=document.createElement('small');caption.textContent=label;group.append(caption);face.append(group);
  }
  play.append(face);panel.append(play);
  const actions=document.createElement('div');actions.className='digital-actions';
  actions.append(button('START','play',()=>{state.commands|=window.be2?.started?commands.pause.bit:commands.start.bit;}));
  const more=document.createElement('details');more.className='digital-more';
  const summary=document.createElement('summary');summary.textContent='SELECT';summary.setAttribute('aria-label','Sound and saved game controls');more.append(summary);
  const menu=document.createElement('div');menu.className='digital-menu';
  for(const [label,name,bit] of [...(action?[['Restart','restart',commands.restart.bit]]:[]),['Sound','sound',commands.sound.bit],['Music','music',commands.music.bit],['Save','save',commands.save.bit],['Load','load',commands.load.bit]])menu.append(button(label,name,()=>{state.commands|=bit;more.open=false;}));
  more.append(menu);actions.append(more);
  panel.append(actions);
  const point=e=>{
    const r=canvas.getBoundingClientRect(),scale=Math.min(r.width/800,r.height/450);
    const x=(e.clientX-r.x-(r.width-800*scale)/2)/scale,y=(e.clientY-r.y-(r.height-450*scale)/2)/scale;
    return x>=0&&y>=0&&x<800&&y<450?{x:Math.floor(x),y:Math.floor(y)}:null;
  };
  canvas.addEventListener('pointerdown',e=>{if(!state.visible||e.pointerType!=='touch')return;e.preventDefault();activate();canvas.setPointerCapture(e.pointerId);state.pointer=point(e);if(state.pointer)state.action=true;state.commands|=commands.start.bit;});
  canvas.addEventListener('pointermove',e=>{if(state.visible&&e.pointerType==='touch'&&e.buttons)state.pointer=point(e);});
  canvas.addEventListener('pointerup',e=>{if(state.visible&&e.pointerType==='touch'){activate();state.commands|=commands.start.bit;}});
  canvas.addEventListener('pointercancel',()=>{state.pointer=null;state.action=false;});
  visibility();
})();
