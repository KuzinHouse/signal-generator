console.log('SGCORE loaded');
const API='/api';let gens=[],signals={},chart=null,chartSel=[],le=[],bs={},brokerChart=null,bhTimer=0;
try{initChart()}catch(e){}loadAll();setInterval(loadAll,1000);
function gv(s,id){if(!s||!Array.isArray(s))return null;const e=s.find(x=>x['@id']===id||x['@id']===('sensor/'+id));return e?e.value:null}
function gvNum(s,id){const v=gv(s,id);return typeof v==='number'?v:parseFloat(v)}
function gvStr(s,id){const v=gv(s,id);return v!==null&&v!==undefined?String(v):'—'}
async function api(p,o={},timeout=5000){try{const c=new AbortController();const t=setTimeout(()=>c.abort(),timeout);const r=await fetch(API+p,{headers:{'Content-Type':'application/json'},signal:c.signal,...o});clearTimeout(t);if(!r.ok)throw Error(r.status);return await r.json()}catch(e){return null}}
async function loadAll(){const d=await api('/generators');if(!d)return;if(JSON.stringify(gens)!==JSON.stringify(d)){gens=d;upCore();upChartUI();render()}upBroker();upCurrent()}
async function upCurrent(){const ids=gens.map(g=>g.id);const res=[];for(let i=0;i<ids.length;i+=10){const chunk=ids.slice(i,i+10);res.push(...await Promise.all(chunk.map(id=>api('/generators/'+id+'/current'))))}res.forEach((s,i)=>{if(s&&Array.isArray(s))signals[ids[i]]=s});upPanels();upChart();upEv()}
async function upBroker(){const b=await api('/broker');if(!b)return;bs=b;
document.getElementById('brokerVer').textContent=b.version||'—';
document.getElementById('brokerClients').textContent=b.clientsConnected;
document.getElementById('brokerClientsDetail').textContent='(max '+b.clientsMax+', истекло '+b.clientsExpired+')';
document.getElementById('brokerSubs').textContent=b.subscriptions;
document.getElementById('brokerTopics').textContent=b.topics;
document.getElementById('brokerHeap').textContent=fmtBytes(b.heapUsed);
document.getElementById('brokerHeapPct').textContent='('+(b.heapMax?((b.heapUsed/b.heapMax)*100).toFixed(0):'?')+'%)';
document.getElementById('brokerRetained').textContent=b.retainedCount;
document.getElementById('brokerStore').textContent=b.storeCount;
document.getElementById('brokerDot').className='mqtt-dot online';
try{const h=await api('/broker/history');if(h&&h.length){const l=h[h.length-1];document.getElementById('brokerMsgRate').textContent=l.msgRate.toFixed(0);document.getElementById('brokerByteRate').textContent=fmtBytes(l.byteRate)+'/с';upBrokerChart(h)}}catch(e){}}
function fmtBytes(b){if(!b||b<1)return'0';if(b<1024)return b+'B';if(b<1048576)return(b/1024).toFixed(0)+'KB';return(b/1048576).toFixed(0)+'MB'}

async function saveGenerator(e){e.preventDefault();const ei=document.getElementById('editId').value;const body={id:document.getElementById('fId').value.trim(),name:document.getElementById('fName').value.trim(),waveType:document.getElementById('fWaveType').value,intervalMs:+document.getElementById('fInterval').value,amplitude:+document.getElementById('fAmplitude').value,offset:+document.getElementById('fOffset').value,frequency:+document.getElementById('fFrequency').value,unit:document.getElementById('fUnit').value,min:+document.getElementById('fMin').value,max:+document.getElementById('fMax').value,quality:+document.getElementById('fQuality').value,enabled:document.getElementById('fEnabled').checked,drift:+document.getElementById('fDrift').value,trend:+document.getElementById('fTrend').value,noiseAmp:+document.getElementById('fNoiseAmp').value,spikeProb:+document.getElementById('fSpikeProb').value,spikeAmp:+document.getElementById('fSpikeAmp').value,deadband:+document.getElementById('fDeadband').value,hysteresis:+document.getElementById('fHysteresis').value,stuckProb:+document.getElementById('fStuckProb').value,stuckDurationMs:+document.getElementById('fStuckDur').value,jitter:+document.getElementById('fJitter').value,dropProb:+document.getElementById('fDropProb').value,degradationRate:+document.getElementById('fDegradation').value,modbusAddr:+document.getElementById('fMbAddr').value,modbusFn:+document.getElementById('fMbFn').value,modbusType:+document.getElementById('fMbType').value,modbusScale:+document.getElementById('fMbScale').value,modbusSlave:+document.getElementById('fMbSlave').value};if(ei){await api('/generators/'+ei,{method:'PUT',body:JSON.stringify({...body,id:ei})})}else{await api('/generators',{method:'POST',body:JSON.stringify(body)})}closeModal();loadAll()}
async function delGen(id){const g=gens.find(x=>x.id===id);if(!g)return;document.getElementById('confirmName').textContent=g.name+' ('+g.id+')';document.getElementById('confirmOverlay').classList.add('open');window._delId=id}
function confirmYes(){const id=window._delId;window._delId=null;document.getElementById('confirmOverlay').classList.remove('open');api('/generators/'+id,{method:'DELETE'});delete signals[id];loadAll()}
function confirmNo(){window._delId=null;document.getElementById('confirmOverlay').classList.remove('open')}
async function togGen(id){await api('/generators/'+id+'/toggle',{method:'PUT'});loadAll()}

const WI={Sine:'∿',Sawtooth:'↗',Square:'⊓',Noise:'≈',Random:'?',Constant:'—'};
function render(){const g=document.getElementById('panelsGrid');if(!gens.length){g.innerHTML='<div style="grid-column:1/-1;text-align:center;padding:30px;color:var(--t3);font-family:JetBrains Mono,monospace;font-size:10px;text-transform:uppercase">Нет генераторов</div>';return}
g.innerHTML=gens.map(x=>`<div class="panel ${x.enabled?'':'disabled'} ${panelCls(x)}" id="pn-${x.id}" onclick="editGen('${x.id}')"><div class="panel-header"><span>${esc(x.name)}</span><span>${esc(x.id)}</span></div><div class="panel-value gradient-text ${valCol(x)}" id="pv-${x.id}">${signals[x.id]&&Array.isArray(signals[x.id])?gvNum(signals[x.id],x.id).toFixed(1):'—'}<small> ${esc(x.unit)}</small></div><div class="panel-footer">${x.waveType} ${x.intervalMs}ms${x.modbusAddr?' · MB:'+x.modbusAddr:''} · <span id="lt-${x.id}">—</span></div><div class="wave-icon">${WI[x.waveType]||''}</div><div class="panel-actions" onclick="event.stopPropagation()"><label class="toggle"><input type="checkbox" ${x.enabled?'checked':''} onchange="togGen('${x.id}')"><span class="toggle-slider"></span></label><button class="pn-action" onclick="delGen('${x.id}')">✕</button></div><span style="display:none">${signals[x.id]&&Array.isArray(signals[x.id])?gvNum(signals[x.id],x.id).toFixed(1)+' '+esc(x.unit):'—'}</span></div>`).join('')}
function upPanels(){for(const g of gens){const el=document.getElementById('pv-'+g.id);const pn=document.getElementById('pn-'+g.id);if(!el)continue;const lt=document.getElementById('lt-'+g.id);const s=signals[g.id];if(s&&Array.isArray(s)){el.innerHTML=gvNum(s,g.id).toFixed(1)+'<small> '+esc(g.unit)+'</small>',el.className='panel-value gradient-text '+valCol(g);const ts=gvStr(s,'timestamp');if(ts&&ts!=='—'){const lat=Date.now()-new Date(ts).getTime();lt.textContent=lat<1000?lat+'ms':(lat/1000).toFixed(1)+'s'}else lt.textContent='—'}}if(pn){const cls=['panel'];if(!g.enabled)cls.push('disabled');cls.push(panelCls(g));pn.className=cls.join(' ')}}
function valCol(g){const s=signals[g.id];if(!s||!Array.isArray(s))return'value-ok';const v=gvNum(s,g.id),q=gvNum(s,g.id+'_quality');const r=Math.abs(v-g.offset)/(g.amplitude||1);if(r>0.9||q<50)return'value-bad';if(r>0.7||q<80)return'value-warn';return'value-ok'}
function panelCls(g){const s=signals[g.id];if(!s||!Array.isArray(s))return'';const v=gvNum(s,g.id),q=gvNum(s,g.id+'_quality');const r=Math.abs(v-g.offset)/(g.amplitude||1);if(r>0.9||q<50)return'alarm';if(r>0.7||q<80)return'warning';return''}
function upStat(){document.getElementById('mqttDot').className='mqtt-dot online'}
function upCore(){const t=gens.length,a=gens.filter(g=>g.enabled).length,rate=gens.filter(g=>g.enabled).reduce((s,g)=>s+(g.intervalMs>0?1000/g.intervalMs:0),0);document.getElementById('sGen').textContent=t;document.getElementById('sAct').textContent=a;document.getElementById('sSig').textContent=rate.toFixed(0);document.getElementById('genCnt').textContent=t;document.getElementById('actCnt').textContent=a;document.getElementById('coreTitle').textContent=a?'Активно '+a+' генераторов.':(t?'Все генераторы остановлены':'Система в режиме ожидания.')}

function upEv(){const n=new Date();for(const g of gens){const s=signals[g.id];if(!s||!Array.isArray(s)||!s.length)continue;const v=gvNum(s,g.id);if(isNaN(v))continue;const k=g.id+'_'+v.toFixed(2);if(le.some(e=>e.k===k))continue;le.unshift({k,time:n.toTimeString().slice(0,8),tag:g.waveType.toUpperCase(),text:g.name+': '+v.toFixed(2)+' '+g.unit,alert:Math.abs(v)>g.amplitude*0.9})}if(le.length>20)le.length=20;document.getElementById('eventLog').innerHTML=le.slice(0,5).map(e=>'<div class="event-item'+(e.alert?' alert':'')+'"><span class="ev-time">'+e.time+'</span><span class="ev-tag">'+e.tag+'</span><span class="ev-text">'+esc(e.text)+'</span></div>').join('')}

function openModal(){document.getElementById('editId').value='';document.getElementById('modalTitle').textContent='НОВЫЙ ГЕНЕРАТОР';document.getElementById('modalSubmit').textContent='СОЗДАТЬ';document.getElementById('generatorForm').reset();document.getElementById('fEnabled').checked=true;document.getElementById('modalOverlay').classList.add('open')}
function closeModal(){document.getElementById('modalOverlay').classList.remove('open')}
function rnd(v,d){return typeof v==='number'?parseFloat(v.toFixed(d||4)):v}
function editGen(id){const g=gens.find(x=>x.id===id);if(!g)return;const s=id=>document.getElementById(id);s('editId').value=g.id;s('modalTitle').textContent='РЕДАКТИРОВАТЬ';s('modalSubmit').textContent='СОХРАНИТЬ';s('fId').value=g.id;s('fName').value=g.name;s('fWaveType').value=g.waveType;
s('fInterval').value=g.intervalMs;s('fAmplitude').value=rnd(g.amplitude);s('fOffset').value=rnd(g.offset);s('fFrequency').value=rnd(g.frequency,6);
s('fUnit').value=g.unit;s('fMin').value=rnd(g.min);s('fMax').value=rnd(g.max);s('fQuality').value=g.quality;
s('fEnabled').checked=g.enabled;
s('fDrift').value=rnd(g.drift||0);s('fTrend').value=rnd(g.trend||0);s('fNoiseAmp').value=rnd(g.noiseAmp||0);
s('fSpikeProb').value=rnd(g.spikeProb||0);s('fSpikeAmp').value=rnd(g.spikeAmp||0);
s('fDeadband').value=rnd(g.deadband||0);s('fHysteresis').value=rnd(g.hysteresis||0);
s('fStuckProb').value=rnd(g.stuckProb||0);s('fStuckDur').value=g.stuckDurationMs||3000;
s('fJitter').value=rnd(g.jitter||0);s('fDropProb').value=rnd(g.dropProb||0);
s('fDegradation').value=rnd(g.degradationRate||0);
s('fMbAddr').value=g.modbusAddr||0;s('fMbFn').value=g.modbusFn||0;s('fMbType').value=g.modbusType||0;
s('fMbScale').value=rnd(g.modbusScale||1.0);s('fMbSlave').value=g.modbusSlave||1;
s('modalOverlay').classList.add('open')}

function initChart(){const ctx=document.getElementById('liveChart').getContext('2d');chart=new Chart(ctx,{type:'line',data:{datasets:[]},options:{responsive:true,maintainAspectRatio:false,animation:{duration:200},interaction:{intersect:false,mode:'nearest'},scales:{x:{type:'linear',grid:{color:'rgba(255,255,255,0.04)'},ticks:{color:'rgba(255,255,255,0.25)',font:{size:8},maxTicksLimit:8,callback:v=>new Date(v).toLocaleTimeString()}},y:{grid:{color:'rgba(255,255,255,0.04)'},ticks:{color:'rgba(255,255,255,0.25)',font:{size:8}}}},plugins:{legend:{labels:{color:'rgba(255,255,255,0.35)',font:{size:9},usePointStyle:true,padding:10}}}}})}
function upChartUI(){const t=document.getElementById('chartToggles');const cols=['#00f0ff','#34d399','#fbbf24','#a855f7','#ef4444','#3b82f6','#fff','#f472b6'];t.innerHTML=gens.map((g,i)=>'<label style="cursor:pointer;display:inline-flex;align-items:center;gap:3px;font-size:clamp(7px,0.7vw,9px);font-family:JetBrains Mono,monospace;text-transform:uppercase;letter-spacing:0.5px;color:'+(chartSel.includes(g.id)?cols[i%cols.length]:'var(--t3)')+'" onclick="toggleChart(\''+g.id+'\')"><span style="display:inline-block;width:6px;height:6px;border-radius:50%;background:'+cols[i%cols.length]+';opacity:'+(chartSel.includes(g.id)?'1':'0.3')+'"></span>'+g.name+'</label>').join('')}
function toggleChart(id){const i=chartSel.indexOf(id);if(i>-1)chartSel.splice(i,1);else chartSel.push(id);if(chart){chart.data.datasets=[];chart.update()}upChartUI()}
function upChart(){try{const n=Date.now();if(!chart)return;for(const id of chartSel){const s=signals[id];if(!s||!Array.isArray(s)||!s.length)continue;const v=gvNum(s,id);if(isNaN(v))continue;let ds=chart.data.datasets.find(d=>d.label===id);if(!ds){const g=gens.find(x=>x.id===id);const cols=['#00f0ff','#34d399','#fbbf24','#a855f7','#ef4444','#3b82f6','#fff','#f472b6'];const c=cols[chart.data.datasets.length%cols.length];ds={label:id,data:[],borderColor:c,backgroundColor:c+'20',pointRadius:0,pointHoverRadius:3,borderWidth:1.5,tension:0.3,fill:false};chart.data.datasets.push(ds)}ds.data.push({x:n,y:v});const co=n-30000;ds.data=ds.data.filter(p=>p.x>co);if(ds.data.length>60)ds.data=ds.data.slice(-60)}chart.data.datasets=chart.data.datasets.filter(ds=>chartSel.includes(ds.label));chart.update('none')}catch(e){console.error('CHART:',e)}}
function esc(s){const d=document.createElement('div');d.textContent=s;return d.innerHTML}
function initBrokerChart(){const c=document.getElementById('brokerChart');if(!c)return;brokerChart=c;brokerChart.width=c.clientWidth||600;brokerChart.height=c.clientHeight||80;brokerChart.buffer=[]}

function upBrokerChart(h){if(!brokerChart||!h||!h.length)return;
const buf=brokerChart.buffer;const now=Date.now();
for(const x of h){buf.push({x:now,y1:x.msgRate,y2:x.heapPct})}
const co=now-600000;while(buf.length&&buf[0].x<co)buf.shift();
if(buf.length>120)buf.splice(0,buf.length-120);
const ctx=brokerChart.getContext('2d');const w=brokerChart.width;const hgt=brokerChart.height;
ctx.clearRect(0,0,w,hgt);
if(buf.length<2)return;
const maxRate=Math.max(...buf.map(p=>p.y1),1);const maxHeap=Math.max(...buf.map(p=>p.y2),1);
ctx.strokeStyle='rgba(255,255,255,0.5)';ctx.lineWidth=1;ctx.beginPath();
buf.forEach((p,i)=>{const x=i/(buf.length-1)*w;const y=hgt-(p.y1/maxRate)*(hgt-20)-10;i===0?ctx.moveTo(x,y):ctx.lineTo(x,y)});ctx.stroke();
ctx.strokeStyle='rgba(255,255,255,0.2)';ctx.setLineDash([2,2]);ctx.beginPath();
buf.forEach((p,i)=>{const x=i/(buf.length-1)*w;const y=hgt-(p.y2/maxHeap)*(hgt-20)-10;i===0?ctx.moveTo(x,y):ctx.lineTo(x,y)});ctx.stroke();ctx.setLineDash([]);
ctx.fillStyle='rgba(255,255,255,0.15)';ctx.font='8px monospace';ctx.fillText(Math.round(maxRate)+' msg/s',4,10);
ctx.fillStyle='rgba(255,255,255,0.08)';ctx.fillText(Math.round(maxHeap)+'%',4,hgt-4);}
initBrokerChart();
