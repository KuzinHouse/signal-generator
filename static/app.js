console.log('SGCORE loaded');
const API='/api';
let gens=[],signals={},chartSel=[],le=[],bs={},brokerChart=null,liveChart=null,liveBuf={},bhTimer=0;

/* ========== утилиты ========== */
function gv(s,id){if(!s||!Array.isArray(s))return null;const e=s.find(x=>x['@id']===id||x['@id']===('sensor/'+id));return e?e.value:null}
function gvNum(s,id){const v=gv(s,id);return typeof v==='number'?v:parseFloat(v)}
function gvStr(s,id){const v=gv(s,id);return v!==null&&v!==undefined?String(v):'—'}
function esc(s){const d=document.createElement('div');d.textContent=s;return d.innerHTML}
function rnd(v,d){return typeof v==='number'?parseFloat(v.toFixed(d||4)):v}
function fmtBytes(b){if(!b||b<1)return'0';if(b<1024)return b+'B';if(b<1048576)return(b/1024).toFixed(0)+'KB';return(b/1048576).toFixed(0)+'MB'}

async function api(p,o={},timeout=5000){try{const c=new AbortController();const t=setTimeout(()=>c.abort(),timeout);const r=await fetch(API+p,{headers:{'Content-Type':'application/json'},signal:c.signal,...o});clearTimeout(t);if(!r.ok)throw Error(r.status);return await r.json()}catch(e){return null}}

/* ========== данные ========== */
async function loadAll(){
  const d=await api('/generators');
  if(!d)return;
  if(JSON.stringify(gens)!==JSON.stringify(d)){gens=d;upCore();upChartUI();render()}
  upBroker();upCurrent();
}
async function upCurrent(){
  const ids=gens.map(g=>g.id);const res=[];
  for(let i=0;i<ids.length;i+=10){const chunk=ids.slice(i,i+10);res.push(...await Promise.all(chunk.map(id=>api('/generators/'+id+'/current'))))}
  res.forEach((s,i)=>{if(s&&Array.isArray(s))signals[ids[i]]=s});
  upPanels();upChart();upEv();
}
async function upBroker(){
  const b=await api('/broker');if(!b)return;bs=b;
  const $=id=>document.getElementById(id);
  $('brokerVer').textContent=b.version||'—';
  $('brokerClients').textContent=b.clients_connected??b.clientsConnected??0;
  $('brokerClientsDetail').textContent='(max '+(b.clients_max??b.clientsMax??0)+', истекло '+(b.clients_expired??b.clientsExpired??0)+')';
  $('brokerSubs').textContent=b.subscriptions??0; $('brokerTopics').textContent=b.topics??0;
  $('brokerHeap').textContent=fmtBytes(b.heap_used??b.heapUsed??0);
  $('brokerHeapPct').textContent='('+((b.heap_max??b.heapMax)?(((b.heap_used??b.heapUsed??0)/(b.heap_max??b.heapMax))*100).toFixed(0):'?')+'%)';
  $('brokerRetained').textContent=b.retained_count??b.retainedCount??0;
  $('brokerStore').textContent=b.store_count??b.storeCount??0;
  $('brokerDot').className='mqtt-dot online';
  const h=await api('/broker/history');
  if(h&&h.length){const l=h[h.length-1];
    $('brokerMsgRate').textContent=((l.msg_rate??l.msgRate)||0).toFixed(0);
    $('brokerByteRate').textContent=fmtBytes(l.byte_rate??l.byteRate)+'/с';
    upBrokerChart(h);
  }
}

/* ========== панели ========== */
const WI={Sine:'∿',Sawtooth:'↗',Square:'⊓',Noise:'≈',Random:'?',Constant:'—'};
let searchQuery='';
function filteredGens(){const q=searchQuery.trim().toLowerCase();if(!q)return gens;return gens.filter(g=>(g.name+' '+g.id+' '+g.topic+' '+g.waveType).toLowerCase().includes(q))}
function render(){
  const g=document.getElementById('panelsGrid');
  const list=filteredGens();
  const cnt=document.getElementById('panelsCount');
  if(cnt)cnt.textContent=list.length+' / '+gens.length;
  if(!list.length){g.innerHTML='<div style="grid-column:1/-1;text-align:center;padding:30px;color:var(--t3);font-family:JetBrains Mono,monospace;font-size:10px;text-transform:uppercase">'+(gens.length?'Ничего не найдено':'Нет генераторов')+'</div>';return}
  g.innerHTML=list.map(x=>'<div class="panel '+(x.enabled?'':'disabled')+' '+panelCls(x)+'" id="pn-'+esc(x.id)+'" data-edit="'+esc(x.id)+'">'
    +'<div class="panel-header"><span>'+esc(x.name)+'</span><span>'+esc(x.id)+'</span></div>'
    +'<div class="panel-value gradient-text '+valCol(x)+'" id="pv-'+esc(x.id)+'">'+(signals[x.id]&&Array.isArray(signals[x.id])?gvNum(signals[x.id],x.id).toFixed(1):'—')+'<small> '+esc(x.unit)+'</small></div>'
    +'<div class="panel-footer">'+esc(x.topic)+' · '+esc(x.waveType)+' '+x.intervalMs+'ms'+(x.modbusAddr?' · MB:'+x.modbusAddr:'')+' · <span id="lt-'+esc(x.id)+'">—</span></div>'
    +'<div class="wave-icon">'+(WI[x.waveType]||'')+'</div>'
    +'<div class="panel-actions" data-stop="1">'
    +'<label class="toggle"><input type="checkbox"'+(x.enabled?' checked':'')+' data-toggle="'+esc(x.id)+'"><span class="toggle-slider"></span></label>'
    +'<button class="pn-action" data-del="'+esc(x.id)+'">✕</button></div>'
    +'<span style="display:none">'+(signals[x.id]&&Array.isArray(signals[x.id])?gvNum(signals[x.id],x.id).toFixed(1)+' '+esc(x.unit):'—')+'</span></div>').join('');
}
function upPanels(){
  for(const g of gens){
    const el=document.getElementById('pv-'+g.id);const pn=document.getElementById('pn-'+g.id);
    if(!el)continue;
    const lt=document.getElementById('lt-'+g.id);const s=signals[g.id];
    if(s&&Array.isArray(s)){
      el.innerHTML=gvNum(s,g.id).toFixed(1)+'<small> '+esc(g.unit)+'</small>';
      el.className='panel-value gradient-text '+valCol(g);
      const ts=gvStr(s,'timestamp');
      if(ts&&ts!=='—'){const lat=Date.now()-new Date(ts).getTime();lt.textContent=lat<1000?lat+'ms':(lat/1000).toFixed(1)+'s'}
      else lt.textContent='—';
    }
    if(pn){const cls=['panel'];if(!g.enabled)cls.push('disabled');cls.push(panelCls(g));pn.className=cls.join(' ')}
  }
}
function valCol(g){const s=signals[g.id];if(!s||!Array.isArray(s))return'value-ok';const v=gvNum(s,g.id),q=gvNum(s,g.id+'_quality');const r=Math.abs(v-g.offset)/(g.amplitude||1);if(r>0.9||q<50)return'value-bad';if(r>0.7||q<80)return'value-warn';return'value-ok'}
function panelCls(g){const s=signals[g.id];if(!s||!Array.isArray(s))return'';const v=gvNum(s,g.id),q=gvNum(s,g.id+'_quality');const r=Math.abs(v-g.offset)/(g.amplitude||1);if(r>0.9||q<50)return'alarm';if(r>0.7||q<80)return'warning';return''}
function upStat(){document.getElementById('mqttDot').className='mqtt-dot online'}
function upCore(){
  const t=gens.length,a=gens.filter(g=>g.enabled).length,
    rate=gens.filter(g=>g.enabled).reduce((s,g)=>s+(g.intervalMs>0?1000/g.intervalMs:0),0);
  document.getElementById('sGen').textContent=t;document.getElementById('sAct').textContent=a;
  document.getElementById('sSig').textContent=rate.toFixed(0);
  document.getElementById('genCnt').textContent=t;document.getElementById('actCnt').textContent=a;
  document.getElementById('coreTitle').textContent=a?'Активно '+a+' генераторов.':(t?'Все генераторы остановлены':'Система в режиме ожидания.');
}
function upEv(){
  const n=new Date();
  for(const g of gens){
    const s=signals[g.id];if(!s||!Array.isArray(s)||!s.length)continue;
    const v=gvNum(s,g.id);if(isNaN(v))continue;
    const k=g.id+'_'+v.toFixed(2);
    if(le.some(e=>e.k===k))continue;
    le.unshift({k,time:n.toTimeString().slice(0,8),tag:g.waveType.toUpperCase(),text:g.name+': '+v.toFixed(2)+' '+g.unit,alert:Math.abs(v)>g.amplitude*0.9});
  }
  if(le.length>20)le.length=20;
  document.getElementById('eventLog').innerHTML=le.slice(0,5).map(e=>'<div class="event-item'+(e.alert?' alert':'')+'"><span class="ev-time">'+e.time+'</span><span class="ev-tag">'+e.tag+'</span><span class="ev-text">'+esc(e.text)+'</span></div>').join('');
}

/* ========== модалка ========== */
function openModal(){
  document.getElementById('editId').value='';
  document.getElementById('modalTitle').textContent='НОВЫЙ ГЕНЕРАТОР';
  document.getElementById('modalSubmit').textContent='СОЗДАТЬ';
  document.getElementById('generatorForm').reset();
  document.getElementById('fEnabled').checked=true;
  document.getElementById('modalOverlay').classList.add('open');
}
function closeModal(){document.getElementById('modalOverlay').classList.remove('open')}
function editGen(id){
  const g=gens.find(x=>x.id===id);if(!g)return;
  const s=id=>document.getElementById(id);
  s('editId').value=g.id;s('modalTitle').textContent='РЕДАКТИРОВАТЬ';s('modalSubmit').textContent='СОХРАНИТЬ';
  s('fId').value=g.id;s('fName').value=g.name;s('fWaveType').value=g.waveType;
  s('fInterval').value=g.intervalMs;s('fAmplitude').value=rnd(g.amplitude);s('fOffset').value=rnd(g.offset);
  s('fFrequency').value=rnd(g.frequency,6);s('fUnit').value=g.unit;s('fMin').value=rnd(g.min);s('fMax').value=rnd(g.max);
  s('fQuality').value=g.quality;s('fEnabled').checked=g.enabled;
  s('fDrift').value=rnd(g.drift||0);s('fTrend').value=rnd(g.trend||0);
  s('fNoiseAmp').value=rnd(g.noiseAmp||0);s('fSpikeProb').value=rnd(g.spikeProb||0);s('fSpikeAmp').value=rnd(g.spikeAmp||0);
  s('fDeadband').value=rnd(g.deadband||0);s('fHysteresis').value=rnd(g.hysteresis||0);
  s('fStuckProb').value=rnd(g.stuckProb||0);s('fStuckDur').value=g.stuckDurationMs||3000;
  s('fJitter').value=rnd(g.jitter||0);s('fDropProb').value=rnd(g.dropProb||0);
  s('fDegradation').value=rnd(g.degradationRate||0);
  s('fMbAddr').value=g.modbusAddr||0;s('fMbFn').value=g.modbusFn||0;s('fMbType').value=g.modbusType||0;
  s('fMbScale').value=rnd(g.modbusScale||1.0);s('fMbSlave').value=g.modbusSlave||1;
  s('modalOverlay').classList.add('open');
}
async function saveGenerator(e){
  e.preventDefault();
  const ei=document.getElementById('editId').value;
  const body={id:document.getElementById('fId').value.trim(),name:document.getElementById('fName').value.trim(),
    waveType:document.getElementById('fWaveType').value,intervalMs:+document.getElementById('fInterval').value,
    amplitude:+document.getElementById('fAmplitude').value,offset:+document.getElementById('fOffset').value,
    frequency:+document.getElementById('fFrequency').value,unit:document.getElementById('fUnit').value,
    min:+document.getElementById('fMin').value,max:+document.getElementById('fMax').value,
    quality:+document.getElementById('fQuality').value,enabled:document.getElementById('fEnabled').checked,
    drift:+document.getElementById('fDrift').value,trend:+document.getElementById('fTrend').value,
    noiseAmp:+document.getElementById('fNoiseAmp').value,spikeProb:+document.getElementById('fSpikeProb').value,
    spikeAmp:+document.getElementById('fSpikeAmp').value,deadband:+document.getElementById('fDeadband').value,
    hysteresis:+document.getElementById('fHysteresis').value,stuckProb:+document.getElementById('fStuckProb').value,
    stuckDurationMs:+document.getElementById('fStuckDur').value,jitter:+document.getElementById('fJitter').value,
    dropProb:+document.getElementById('fDropProb').value,degradationRate:+document.getElementById('fDegradation').value,
    modbusAddr:+document.getElementById('fMbAddr').value,modbusFn:+document.getElementById('fMbFn').value,
    modbusType:+document.getElementById('fMbType').value,modbusScale:+document.getElementById('fMbScale').value,
    modbusSlave:+document.getElementById('fMbSlave').value};
  const req = ei
    ? api('/generators/'+ei,{method:'PUT',body:JSON.stringify({...body,id:ei})})
    : api('/generators',{method:'POST',body:JSON.stringify(body)});
  closeModal();          // закрываем сразу, не ждём ответ
  await req;
  loadAll();
}
function delGen(id){
  const g=gens.find(x=>x.id===id);if(!g)return;
  document.getElementById('confirmName').textContent=g.name+' ('+g.id+')';
  document.getElementById('confirmOverlay').classList.add('open');
  window._delId=id;
}
function confirmYes(){
  const id=window._delId;window._delId=null;
  document.getElementById('confirmOverlay').classList.remove('open');
  api('/generators/'+id,{method:'DELETE'});
  delete signals[id];loadAll();
}
function confirmNo(){window._delId=null;document.getElementById('confirmOverlay').classList.remove('open')}
async function togGen(id){await api('/generators/'+id+'/toggle',{method:'PUT'});loadAll()}

/* ========== настройки MQTT ========== */
async function openMqttSettings(){
  const cfg=await api('/mqtt/config');
  if(!cfg)return;
  document.getElementById('mqHost').value=cfg.host||'';
  document.getElementById('mqPort').value=cfg.port||1883;
  document.getElementById('mqUser').value=cfg.username||'';
  document.getElementById('mqPass').value=cfg.password&&cfg.password!=='••••••'?cfg.password:'';
  if(document.getElementById('mqTls'))document.getElementById('mqTls').checked=!!cfg.use_tls;
  document.getElementById('mqPrefix').value=cfg.topic_prefix||'USEPI';
  document.getElementById('mqDiag').value=cfg.diagnostics_topic||'USEPI/diagnostics';
  document.getElementById('mqttOverlay').classList.add('open');
}
function closeMqtt(){document.getElementById('mqttOverlay').classList.remove('open')}
async function testMqtt(){
  const btn=document.getElementById('mqttTestBtn');
  const res=document.getElementById('mqTestResult');
  if(btn)btn.disabled=true;
  if(res)res.textContent='ПРОВЕРКА...';
  const body={
    host:document.getElementById('mqHost').value.trim(),
    port:+document.getElementById('mqPort').value||1883,
    username:document.getElementById('mqUser').value.trim(),
    password:document.getElementById('mqPass').value,
    topic_prefix:document.getElementById('mqPrefix').value.trim()||'USEPI',
    diagnostics_topic:document.getElementById('mqDiag').value.trim()||'USEPI/diagnostics',
    use_tls:document.getElementById('mqTls')?document.getElementById('mqTls').checked:false,
  };
  const r=await api('/mqtt/test',{method:'POST',body:JSON.stringify(body)},6000);
  if(btn)btn.disabled=false;
  if(!r){if(res)res.textContent='ОШИБКА: нет ответа';return}
  if(res){
    if(r.ok)res.textContent='✓ ДОСТУПЕН ('+r.latency_ms+'ms)'+(r.tls?' TLS':'');
    else res.textContent='✗ НЕДОСТУПЕН: '+(r.error||'?');
    res.style.color=r.ok?'#34d399':'rgba(255,80,80,0.8)';
  }
}
async function saveMqttSettings(e){
  e.preventDefault();
  const body={
    host:document.getElementById('mqHost').value.trim(),
    port:+document.getElementById('mqPort').value||1883,
    username:document.getElementById('mqUser').value.trim(),
    password:document.getElementById('mqPass').value,
    topic_prefix:document.getElementById('mqPrefix').value.trim()||'USEPI',
    diagnostics_topic:document.getElementById('mqDiag').value.trim()||'USEPI/diagnostics',
    use_tls:document.getElementById('mqTls')?document.getElementById('mqTls').checked:false,
  };
  const req=api('/mqtt/config',{method:'PUT',body:JSON.stringify(body)});
  closeMqtt();
  const r=await req;
  if(r)loadAll();
}

/* ========== live chart (canvas, без Chart.js) ========== */
const COLORS=['#00f0ff','#34d399','#fbbf24','#a855f7','#ef4444','#3b82f6','#ffffff','#f472b6'];
function initLiveChart(){
  const c=document.getElementById('liveChart');if(!c)return;
  liveChart=c;
  const dpr=window.devicePixelRatio||1;
  liveChart.width=(c.clientWidth||600)*dpr;liveChart.height=(c.clientHeight||160)*dpr;
  liveChart._dpr=dpr;liveChart.buffer={};
}
function resizeCharts(){
  const dpr=window.devicePixelRatio||1;
  if(liveChart){liveChart.width=(liveChart.clientWidth||600)*dpr;liveChart.height=(liveChart.clientHeight||160)*dpr;liveChart._dpr=dpr;drawLiveChart()}
  if(brokerChart){brokerChart.width=brokerChart.clientWidth||600;brokerChart.height=brokerChart.clientHeight||80;drawBrokerChart()}
}
window.addEventListener('resize',resizeCharts);
function upChartUI(){
  const t=document.getElementById('chartToggles');
  t.innerHTML=gens.map((g,i)=>'<label style="cursor:pointer;display:inline-flex;align-items:center;gap:3px;font-size:clamp(7px,0.7vw,9px);font-family:JetBrains Mono,monospace;text-transform:uppercase;letter-spacing:0.5px;color:'+(chartSel.includes(g.id)?COLORS[i%COLORS.length]:'var(--t3)')+'" data-chart="'+esc(g.id)+'"><span style="display:inline-block;width:6px;height:6px;border-radius:50%;background:'+COLORS[i%COLORS.length]+';opacity:'+(chartSel.includes(g.id)?'1':'0.3')+'"></span>'+esc(g.name)+'</label>').join('');
}
function toggleChart(id){
  const i=chartSel.indexOf(id);
  if(i>-1)chartSel.splice(i,1);else chartSel.push(id);
  upChartUI();
}
function upChart(){
  try{
    const n=Date.now();if(!liveChart)return;
    if(!liveChart.buffer)liveChart.buffer={};
    for(const id of chartSel){
      const s=signals[id];if(!s||!Array.isArray(s)||!s.length)continue;
      const v=gvNum(s,id);if(isNaN(v))continue;
      if(!liveChart.buffer[id])liveChart.buffer[id]=[];
      const buf=liveChart.buffer[id];
      buf.push({x:n,y:v});
      const co=n-30000;while(buf.length&&buf[0].x<co)buf.shift();
      if(buf.length>60)buf.splice(0,buf.length-60);
    }
    drawLiveChart();
  }catch(e){console.error('CHART:',e)}
}
function drawLiveChart(){
  const c=liveChart;if(!c)return;
  const dpr=c._dpr||1;const w=c.width/dpr,h=c.height/dpr;
  const ctx=c.getContext('2d');
  ctx.setTransform(dpr,0,0,dpr,0,0);
  ctx.clearRect(0,0,w,h);
  // сетка
  ctx.strokeStyle='rgba(255,255,255,0.04)';ctx.lineWidth=1;
  for(let i=1;i<5;i++){const y=h*i/5;ctx.beginPath();ctx.moveTo(0,y);ctx.lineTo(w,y);ctx.stroke()}
  for(let i=1;i<8;i++){const x=w*i/8;ctx.beginPath();ctx.moveTo(x,0);ctx.lineTo(x,h);ctx.stroke()}
  if(!chartSel.length){ctx.fillStyle='rgba(255,255,255,0.15)';ctx.font='8px monospace';ctx.fillText('выберите сигнал выше',8,h-8);return}
  const now=Date.now();const t0=now-30000;
  let gi=0;
  for(const id of chartSel){
    const buf=liveChart.buffer[id]||[];if(buf.length<2){gi++;continue}
    const col=COLORS[gi%COLORS.length];gi++;
    const min=Math.min(...buf.map(p=>p.y)),max=Math.max(...buf.map(p=>p.y));
    const span=(max-min)||1,pad=span*0.1;
    ctx.strokeStyle=col;ctx.lineWidth=1.5;ctx.beginPath();
    buf.forEach((p,i)=>{
      const x=(p.x-t0)/30000*w;
      const y=h-((p.y-(min-pad))/(span+2*pad))*h;
      i===0?ctx.moveTo(x,y):ctx.lineTo(x,y);
    });
    ctx.stroke();
    const g=gens.find(x=>x.id===id);
    ctx.fillStyle=col;ctx.font='8px monospace';
    ctx.fillText((g?g.name:id)+' '+buf[buf.length-1].y.toFixed(1),8,10+gi*10);
  }
}

/* ========== broker chart (canvas) ========== */
function initBrokerChart(){
  const c=document.getElementById('brokerChart');if(!c)return;
  brokerChart=c;
  brokerChart.width=c.clientWidth||600;brokerChart.height=c.clientHeight||80;
  brokerChart.buffer=[];
}
function upBrokerChart(h){
  if(!brokerChart||!h||!h.length)return;
  const buf=brokerChart.buffer;const now=Date.now();
  for(const x of h){buf.push({x:now,y1:x.msg_rate??x.msgRate??0,y2:x.heap_pct??x.heapPct??0})}
  const co=now-600000;while(buf.length&&buf[0].x<co)buf.shift();
  if(buf.length>120)buf.splice(0,buf.length-120);
  drawBrokerChart();
}
function drawBrokerChart(){
  if(!brokerChart)return;
  const buf=brokerChart.buffer;
  const ctx=brokerChart.getContext('2d');const w=brokerChart.width;const hgt=brokerChart.height;
  ctx.clearRect(0,0,w,hgt);
  if(!buf||buf.length<2)return;
  const maxRate=Math.max(...buf.map(p=>p.y1),1);const maxHeap=Math.max(...buf.map(p=>p.y2),1);
  ctx.strokeStyle='rgba(255,255,255,0.5)';ctx.lineWidth=1;ctx.beginPath();
  buf.forEach((p,i)=>{const x=i/(buf.length-1)*w;const y=hgt-(p.y1/maxRate)*(hgt-20)-10;i===0?ctx.moveTo(x,y):ctx.lineTo(x,y)});ctx.stroke();
  ctx.strokeStyle='rgba(255,255,255,0.2)';ctx.setLineDash([2,2]);ctx.beginPath();
  buf.forEach((p,i)=>{const x=i/(buf.length-1)*w;const y=hgt-(p.y2/maxHeap)*(hgt-20)-10;i===0?ctx.moveTo(x,y):ctx.lineTo(x,y)});ctx.stroke();ctx.setLineDash([]);
  ctx.fillStyle='rgba(255,255,255,0.15)';ctx.font='8px monospace';ctx.fillText(Math.round(maxRate)+' msg/s',4,10);
  ctx.fillStyle='rgba(255,255,255,0.08)';ctx.fillText(Math.round(maxHeap)+'%',4,hgt-4);
}

/* ========== делегирование событий (работает без inline JS) ========== */
document.addEventListener('click',e=>{
  let el=e.target;
  while(el&&el!==document.body&&el!==document.documentElement){
    if(el.hasAttribute&&el.hasAttribute('data-stop'))return; // внутри модалки/действий — не всплываем
    const a=el.dataset&&el.dataset.action;
    if(a){a==='open-modal'?openModal():a==='close-modal'?closeModal():a==='confirm-yes'?confirmYes():a==='confirm-no'?confirmNo():a==='open-mqtt'?openMqttSettings():a==='close-mqtt'?closeMqtt():a==='test-mqtt'?testMqtt():0;return}
    if(el.hasAttribute&&el.hasAttribute('data-chart')){toggleChart(el.dataset.chart);return}
    if(el.hasAttribute&&el.hasAttribute('data-del')){delGen(el.dataset.del);return}
    if(el.hasAttribute&&el.hasAttribute('data-edit')){editGen(el.dataset.edit);return}
    el=el.parentElement;
  }
});
document.addEventListener('change',e=>{
  const tg=e.target.closest('[data-toggle]');
  if(tg&&e.target.matches('input[type=checkbox]')){togGen(tg.dataset.toggle)}
});
document.addEventListener('input',e=>{
  const s=e.target.closest('[data-search]');
  if(s){searchQuery=s.value;render()}
});
document.getElementById('generatorForm').addEventListener('submit',saveGenerator);
document.getElementById('mqttForm').addEventListener('submit',saveMqttSettings);

/* ========== init ========== */
initLiveChart();initBrokerChart();
let ws=null,wsOk=false;
function wsUrl(){const p=location.protocol==='https:'?'wss://':'ws://';return p+location.host+'/ws'}
function applySnapshot(d){
  if(!d)return;
  if(Array.isArray(d.generators)){
    if(JSON.stringify(gens)!==JSON.stringify(d.generators)){gens=d.generators;upCore();upChartUI();render()}
  }
  if(d.type==='update'&&d.signals&&typeof d.signals==='object'){
    // дифф: смержить в существующий signals (только изменившиеся)
    for(const k in d.signals){signals[k]=d.signals[k]}
    upPanels();upChart();upEv();
  } else if(d.signals&&typeof d.signals==='object'){
    // полный снапшот
    signals=d.signals;upPanels();upChart();upEv()
  }
  if(d.broker){const b=d.broker;const $=id=>document.getElementById(id);
    $('brokerVer').textContent=b.version||'—';
    $('brokerClients').textContent=b.clients_connected??b.clientsConnected??0;
    $('brokerClientsDetail').textContent='(max '+(b.clients_max??b.clientsMax??0)+', истекло '+(b.clients_expired??b.clientsExpired??0)+')';
    $('brokerSubs').textContent=b.subscriptions??0;$('brokerTopics').textContent=b.topics??0;
    $('brokerHeap').textContent=fmtBytes(b.heap_used??b.heapUsed??0);
    $('brokerHeapPct').textContent='('+((b.heap_max??b.heapMax)?(((b.heap_used??b.heapUsed??0)/(b.heap_max??b.heapMax))*100).toFixed(0):'?')+'%)';
    $('brokerRetained').textContent=b.retained_count??b.retainedCount??0;
    $('brokerStore').textContent=b.store_count??b.storeCount??0;
    $('brokerDot').className='mqtt-dot online';
  }
  if(Array.isArray(d.history)&&d.history.length){const l=d.history[d.history.length-1];
    document.getElementById('brokerMsgRate').textContent=((l.msg_rate??l.msgRate)||0).toFixed(0);
    document.getElementById('brokerByteRate').textContent=fmtBytes(l.byte_rate??l.byteRate)+'/с';
    upBrokerChart(d.history);
  }
}
function connectWs(){
  try{ws=new WebSocket(wsUrl())}catch(e){return}
  ws.onopen=()=>{wsOk=true;document.getElementById('sysStat').textContent='ОНЛАЙН (WS)'};
  ws.onmessage=ev=>{try{applySnapshot(JSON.parse(ev.data))}catch(e){}};
  ws.onclose=()=>{wsOk=false;document.getElementById('sysStat').textContent='ОНЛАЙН';setTimeout(connectWs,3000)};
  ws.onerror=()=>{wsOk=false};
}
connectWs();
// Fallback: polling, если WS недоступен
setInterval(()=>{if(!wsOk)loadAll()},1000);
loadAll();
