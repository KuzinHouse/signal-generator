console.log('BROKERS loaded');
const API='/api';
let brokers=[];

/* ========== утилиты ========== */
function esc(s){const d=document.createElement('div');d.textContent=s;return d.innerHTML}
function statusChipId(bid){return 'cb-'+bid}

async function api(p,o={},timeout=6000){
  try{
    const c=new AbortController();const t=setTimeout(()=>c.abort(),timeout);
    const headers={'Content-Type':'application/json'};
    const tok=localStorage.getItem('signalApiToken');
    if(tok)headers['Authorization']='Bearer '+tok;
    const r=await fetch(API+p,{headers,signal:c.signal,...o});
    clearTimeout(t);
    if(!r.ok)throw Error(r.status);
    return await r.json();
  }catch(e){return null}
}

/* ========== рендер карточек ========== */
function meta(label,val,cls){
  return `<span class="meta ${cls||''}">${label}: <b>${esc(val)}</b></span>`;
}
function cardHTML(b){
  const on=b.enabled?'':'disabled';
  const dotClass=(b.connected?'ok':'bad');
  const statusText=b.connected?'online':'offline';
  const topicP=(b.topic_prefix&&b.topic_prefix!=='')?`${b.topic_prefix}/&lt;id&gt;`:'/&lt;id&gt;';
  return `
  <div class="card ${on}" id="card-${esc(b.id)}">
    <div class="card-header">
      <div class="card-id">
        <div class="card-avatar">${esc((b.name||b.id).slice(0,2).toUpperCase())}</div>
        <div class="card-name">${esc(b.name||b.id)}<small>${esc(b.id)}</small></div>
      </div>
      <label class="toggle"><input type="checkbox" ${b.enabled?'checked':''} data-toggle="${esc(b.id)}"><span class="toggle-slider"></span></label>
    </div>
    <div class="card-body">
      <div class="addr-row"><span class="proto">${b.use_tls?'TLS':'TCP'}</span><span class="val">${esc(b.host)}:${esc(b.port)}</span></div>
      <div class="meta-row">
        ${meta('СОСТОЯНИЕ',statusText,dotClass)}
        ${meta('VER',b.version||'—')}
        ${meta('КЛИЕНТЫ',b.clients??0)}
        ${meta('ТОПИКИ',b.topics_count ?? b.topics ?? 0)}
      </div>
      <div class="topic-preview">${esc(topicP)}</div>
    </div>
    <div class="card-foot">
      <button class="btn" data-action="test" data-id="${esc(b.id)}">ПРОВЕРИТЬ</button>
      <button class="btn" data-action="edit" data-id="${esc(b.id)}">✎ РЕДАКТИРОВАТЬ</button>
      <button class="btn danger" data-action="delete" data-id="${esc(b.id)}">✕ УДАЛИТЬ</button>
    </div>
  </div>`;
}
function renderCards(){
  const cards=document.getElementById('cards');
  if(!brokers.length){cards.innerHTML='<div class="empty">· нет брокеров — добавьте первый ·</div>';return}
  cards.innerHTML=brokers.map(cardHTML).join('');
}

/* ========== загрузка ========== */
async function loadBrokers(){
  const d=await api('/brokers');
  if(!d||!Array.isArray(d.brokers))return;
  brokers=d.brokers;
  renderCards();
  await refreshStatus();
}
async function refreshStatus(){
  const st=await api('/brokers/status');
  if(!st||!Array.isArray(st))return;
  const connected=st.filter(s=>s.connected);
  const live={};st.forEach(s=>{live[s.id]=s});
  brokers.forEach(b=>{if(live[b.id])Object.assign(b,live[b.id])});
  // обновляем dot и значения в карточках без полной перерисовки
  brokers.forEach(b=>{
    const card=document.getElementById('card-'+b.id);
    if(!card)return;
    const conn=live[b.id]&&live[b.id].connected?true:false;
    const dotCls=conn?'ok':'bad';
    const metaVals=card.querySelectorAll('.meta');
    st.forEach(s=>{});
  });
  renderCards();
  const top=document.getElementById('topStatus');
  const topText=document.getElementById('topStatusText');
  const total=st.length;
  if(top){
    top.className='status-chip '+(connected.length?'ok':'bad');
    topText.textContent=`${connected.length}/${total} online`;
  }
}

/* ========== действия ========== */
async function openModal(id){
  const overlay=document.getElementById('modalOverlay');
  document.getElementById('editId').value=id||'';
  document.getElementById('modalTitle').textContent=id?'РЕДАКТИРОВАТЬ БРОКЕР':'НОВЫЙ БРОКЕР';
  document.getElementById('modalSubmit').textContent=id?'СОХРАНИТЬ':'СОЗДАТЬ';
  document.getElementById('testResult').innerHTML='&nbsp;';
  // дефолты или текущие
  const b=id?(brokers.find(x=>x.id===id)||{}):{};
  document.getElementById('fName').value=b.name||'';
  document.getElementById('fId').value=b.id||'';
  document.getElementById('fHost').value=b.host||'127.0.0.1';
  document.getElementById('fPort').value=b.port||1883;
  document.getElementById('fPrefix').value=b.topic_prefix||'USEPI';
  document.getElementById('fDiag').value=b.diagnostics_topic||'USEPI/diagnostics';
  document.getElementById('fEnabled').checked=b.enabled!==undefined?b.enabled:true;
  document.getElementById('fTls').checked=!!b.use_tls;
  document.getElementById('fUser').value=b.username||'';
  document.getElementById('fPass').value='';
  overlay.classList.add('open');
}
function closeModal(){document.getElementById('modalOverlay').classList.remove('open')}

function collectForm(){
  return {
    id:(document.getElementById('fId').value||'').trim(),
    name:(document.getElementById('fName').value||'').trim(),
    host:(document.getElementById('fHost').value||'').trim(),
    port:parseInt(document.getElementById('fPort').value)||1883,
    topic_prefix:(document.getElementById('fPrefix').value||'').trim(),
    diagnostics_topic:(document.getElementById('fDiag').value||'').trim(),
    enabled:document.getElementById('fEnabled').checked,
    use_tls:document.getElementById('fTls').checked,
    username:(document.getElementById('fUser').value||'').trim(),
    password:(document.getElementById('fPass').value||''),
  };
}

async function saveBroker(e){
  e.preventDefault();
  const body=collectForm();
  const id=document.getElementById('editId').value;
  const payload=JSON.stringify(body);
  const btn=document.getElementById('modalSubmit');
  btn.classList.add('spin');btn.disabled=true;
  let r;
  if(id){r=await api('/brokers/'+encodeURIComponent(id),{method:'PUT',body:payload})}
  else{r=await api('/brokers',{method:'POST',body:payload})}
  btn.classList.remove('spin');btn.disabled=false;
  if(r){loadBrokers();closeModal()}
  else{alert('Ошибка сохранения. Проверьте логи.')}
}

async function deleteBroker(id){
  const b=brokers.find(x=>x.id===id);
  if(!confirm('Удалить брокер '+(b?b.name:id)+'?'))return;
  await api('/brokers/'+encodeURIComponent(id),{method:'DELETE'});
  loadBrokers();
}

async function testBroker(id){
  const btn=document.querySelector(`[data-action="test"][data-id="${id}"]`);
  if(btn){btn.classList.add('spin');btn.disabled=true}
  const b=brokers.find(x=>x.id===id)||{};
  const r=await api('/mqtt/test',{method:'POST',body:JSON.stringify(b)});
  if(btn){btn.classList.remove('spin');btn.disabled=false}
  if(r&&r.ok){alert('Соединение OK: '+b.host+':'+b.port+' ('+r.latency_ms+'ms)')}
  else{alert('Ошибка: '+(r&&r.error?'('+r.error+')':'нет ответа'))}
}

// Тест из формы
async function testForm(){
  const body=collectForm();
  const el=document.getElementById('testResult');
  const btn=document.getElementById('testBtn');
  btn.classList.add('spin');btn.disabled=true;
  const r=await api('/mqtt/test',{method:'POST',body:JSON.stringify(body)});
  btn.classList.remove('spin');btn.disabled=false;
  if(r&&r.ok){el.className='test-result good';el.textContent='✓ соединение OK ('+r.latency_ms+'ms)'}
  else{el.className='test-result bad';el.textContent='✕ '+(r&&r.error?r.error:'нет ответа')}
}

async function toggleBroker(id,checked){
  await api('/brokers/'+encodeURIComponent(id)+'/toggle',{method:'PUT'});
  loadBrokers();
}

/* ========== события ========== */
document.addEventListener('click',async (e)=>{
  const a=e.target.closest('[data-action]');
  if(!a)return;
  const act=a.getAttribute('data-action');
  const id=a.getAttribute('data-id');
  if(act==='open-modal')openModal();
  else if(act==='close-modal')closeModal();
  else if(act==='edit')openModal(id);
  else if(act==='delete')deleteBroker(id);
  else if(act==='test')testBroker(id);
  else if(act==='test-broker')testForm();
});
document.getElementById('brokerForm').addEventListener('submit',saveBroker);
// клик по тумблеру (перехват до checkbox default)
document.getElementById('cards').addEventListener('change',async (e)=>{
  const t=e.target.closest('[data-toggle]');
  if(t){e.preventDefault();await toggleBroker(t.getAttribute('data-toggle'),t.checked)}
});

loadBrokers();
// живое обновление каждые 5с
setInterval(refreshStatus,5000);
setInterval(loadBrokers,15000);
