import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, currentMonitor, availableMonitors, PhysicalPosition, LogicalSize } from '@tauri-apps/api/window';
import './style.css';

type Theme = 'mist' | 'night' | 'warm';
type Priority = 'red' | 'yellow' | 'green' | 'gray';
type Todo = {id:string;title:string;memo:string;due_at:number|null;time_specified:boolean;repeat:string;priority:string;kind:string;completed_at:number|null;duration_minutes:number};
type NotePage = {id:string;text:string;priority:Priority;edited_at:number};
type Note = { text:string; pages:NotePage[];active_page:number; theme:Theme; opacity:number; priority:Priority; edited_at:number; position?:{x:number;y:number;monitor?:string};window_width?:number|null;note_height:number;todo_height:number|null;todos:Todo[];cleanup_days:number;show_todos:boolean;show_note:boolean;reminder_sound:boolean;reminder_mode:string;mute_app:boolean;notification_volume:number;notification_tone:string;sort_order:string };
const stack = document.querySelector<HTMLElement>('#stack')!;
const card = document.querySelector<HTMLElement>('#card')!;
const todoCard = document.querySelector<HTMLElement>('.todo-card')!;
const editor = document.querySelector<HTMLTextAreaElement>('#note')!;
const line = document.querySelector<HTMLElement>('#line')!;
const edited = document.querySelector<HTMLElement>('#edited')!;
const todoList = document.querySelector<HTMLElement>('#todo-list')!;
const toast = document.querySelector<HTMLElement>('#reminder-toast')!;
const win = getCurrentWindow();
let note:Note = {text:'',pages:[{id:crypto.randomUUID(),text:'',priority:'gray',edited_at:0}],active_page:0,theme:'mist',opacity:72,priority:'gray',edited_at:Date.now(),note_height:251,todo_height:null,todos:[],cleanup_days:15,show_todos:true,show_note:true,reminder_sound:false,reminder_mode:'popup',mute_app:false,notification_volume:60,notification_tone:'chime',sort_order:'tpk'};
let saveTimer:number|undefined;
let moving = false;
let windowReady = false;
let resizeJob = Promise.resolve();

async function loadNoteReady():Promise<Note>{
  for(let attempt=0;attempt<30;attempt++){
    try{return await invoke<Note>('load_note')}
    catch(error){
      if(!String(error).includes('state not managed')||attempt===29)throw error;
      await new Promise(resolve=>setTimeout(resolve,150));
    }
  }
  throw new Error('便签数据未就绪');
}

function adjustWindowHeight(){
  if(!windowReady)return;
  const wanted=18+(note.show_note?note.note_height:0)+(note.show_todos?todoHeight():0)+(note.show_note&&note.show_todos?5:0);
  resizeJob=resizeJob.then(async()=>{
    const scale=await win.scaleFactor();
    const monitor=await currentMonitor();
    const screenHeight=monitor?Math.floor(monitor.workArea.size.height/scale):wanted;
    const height=Math.min(wanted,screenHeight);
    const width=(await win.innerSize()).width/scale;
    await win.setSizeConstraints({minWidth:200,minHeight:height,maxHeight:height});
    await win.setSize(new LogicalSize(width,height));
    const pos=await win.outerPosition();
    if(monitor){const bottom=monitor.workArea.position.y+monitor.workArea.size.height;const outer=await win.outerSize();if(pos.y+outer.height>bottom)await win.setPosition(new PhysicalPosition(pos.x,bottom-outer.height))}
  }).catch(console.error);
}
function todoHeight(){return note.todo_height??32+Math.min(7,Math.max(2,note.todos.length))*58+7}
function applyHeights(){card.style.height=`${note.note_height}px`;todoCard.style.height=`${todoHeight()}px`;adjustWindowHeight()}

// 100% remains subtly translucent; 0% removes only the card background.
function showOpacity(value:number){const level=Math.min(100,Math.max(0,value))/100;stack.style.setProperty('--opacity',String(level*.84));stack.style.setProperty('--opacity-level',String(level));stack.classList.toggle('clear',level===0)}
function showSections(){stack.classList.toggle('hide-todos',!note.show_todos);stack.classList.toggle('hide-note',!note.show_note);applyHeights()}
function currentPage(){return note.pages[note.active_page]}
function renderPage(){const page=currentPage();editor.value=page.text;card.classList.toggle('empty',!page.text.trim());document.querySelector('#page-count')!.textContent=`${note.active_page+1}/${note.pages.length}`;document.querySelector<HTMLButtonElement>('#prev-page')!.disabled=note.active_page===0;document.querySelector<HTMLButtonElement>('#next-page')!.disabled=note.active_page===note.pages.length-1;document.querySelector<HTMLButtonElement>('#delete-page')!.disabled=note.pages.length===1;updateTime();updateLine()}
function render(){stack.dataset.theme=note.theme;showOpacity(note.opacity);showSections();renderPage();renderTodos()}
function renderTodos(){
  const heading=document.querySelector<HTMLElement>('.todo-header span');
  if(heading){const unfinished=note.todos.filter(todo=>!todo.completed_at).length;const overdue=note.todos.filter(todo=>!todo.completed_at&&todo.due_at!==null&&todo.due_at+(todo.time_specified?0:86400000)<Date.now()).length;heading.replaceChildren();heading.className='todo-summary';const total=document.createElement('span');total.textContent=`待办  ${note.todos.length}`;const status=document.createElement('span');status.className='todo-summary-status';const active=document.createElement('span');active.className='summary-unfinished';active.textContent=`未完成 ${unfinished}`;const expired=document.createElement('span');expired.className='summary-overdue';expired.textContent=`已过期 ${overdue}`;status.append(active,expired);heading.append(total,status)}
  todoList.replaceChildren();
  todoList.style.overflowY='auto';
  applyHeights();
  if(!note.todos.length){const empty=document.createElement('div');empty.className='todo-empty';empty.textContent='暂无代办';todoList.append(empty);return}
  const priorityRank:Record<string,number>={high:0,normal:1,low:2};
  const kindRank:Record<string,number>={work:0,life:1,study:2,other:3};
  const order=['tpk','tkp','ptk','pkt','ktp','kpt'].includes(note.sort_order)?note.sort_order:'tpk';
  const sorted=[...note.todos].sort((a,b)=>{
    const completed=Number(Boolean(a.completed_at))-Number(Boolean(b.completed_at));
    if(completed)return completed;
    for(const key of order){
      const difference=key==='t'?(a.due_at??Infinity)-(b.due_at??Infinity):key==='p'?(priorityRank[a.priority]??1)-(priorityRank[b.priority]??1):(kindRank[a.kind]??3)-(kindRank[b.kind]??3);
      if(difference)return difference;
    }
    return 0;
  });
  for(const todo of sorted){
    const row=document.createElement('div');row.className=`todo-row${todo.completed_at?' done':''}`;
    row.dataset.kind=todo.kind;
    const check=document.createElement('button');check.className='todo-check';check.type='button';check.setAttribute('aria-label',todo.completed_at?'取消完成':'完成待办');check.setAttribute('aria-pressed',String(Boolean(todo.completed_at)));
    check.onclick=()=>void invoke('toggle_todo',{id:todo.id}).catch(console.error);
    const body=document.createElement('div');body.className='todo-body';body.setAttribute('role','button');body.tabIndex=0;body.title='点击编辑待办';body.setAttribute('aria-label',`编辑 ${todo.title}`);
    const openEditor=()=>void invoke('open_edit_todo',{id:todo.id}).catch(console.error);
    body.onclick=openEditor;
    body.onkeydown=event=>{if(event.key==='Enter'||event.key===' '){event.preventDefault();openEditor()}};
    const title=document.createElement('div');title.className='todo-title';title.textContent=todo.title;body.append(title);
    if(todo.memo){const memo=document.createElement('div');memo.className='todo-memo';memo.textContent=todo.memo;body.append(memo)}
    const metaParts:string[]=[];
    if(todo.repeat!=='none')metaParts.push(({daily:'每天',weekly:'每周',monthly:'每月'} as Record<string,string>)[todo.repeat]||'');
    metaParts.push(todo.duration_minutes===60?'1 小时':todo.duration_minutes===120?'2 小时':`${todo.duration_minutes||15} 分钟`);
    if(metaParts.length||todo.due_at){const meta=document.createElement('div');meta.className='todo-meta';if(todo.due_at){const date=document.createElement('span');date.textContent=new Date(todo.due_at).toLocaleString('zh-CN',todo.time_specified?{year:'numeric',month:'2-digit',day:'2-digit',hour:'2-digit',minute:'2-digit'}:{year:'numeric',month:'2-digit',day:'2-digit'});if(!todo.completed_at&&todo.due_at+(todo.time_specified?0:86400000)<Date.now())date.className='overdue-date';meta.append(date)}if(metaParts.length)meta.append(document.createTextNode(`${todo.due_at?' · ':''}${metaParts.join(' · ')}`));body.append(meta)}
    const remove=document.createElement('button');remove.className='todo-delete';remove.type='button';remove.textContent='×';remove.title='删除待办';remove.setAttribute('aria-label',`删除 ${todo.title}`);remove.onclick=()=>void invoke('delete_todo',{id:todo.id}).catch(console.error);
    row.append(check,body,remove);todoList.append(row);
  }
}
function updateTime(){const page=currentPage();const at=page.edited_at;if(!page.text.trim()||!at){edited.textContent='';return}const minutes=Math.max(0,Math.floor((Date.now()-at)/60000));edited.textContent=minutes<1?'刚刚':minutes<60?`${minutes} 分钟前`:`${Math.floor(minutes/60)} 小时前`}
function saveSoon(){clearTimeout(saveTimer);saveTimer=window.setTimeout(()=>void save(),500)}
async function save(){clearTimeout(saveTimer);await invoke('save_note',{note})}
function updateLine(){const before=editor.value.slice(0,editor.selectionStart);const row=before.split('\n').length-1;const style=getComputedStyle(editor);const lineHeight=parseFloat(style.lineHeight);line.style.top=`${row*lineHeight-editor.scrollTop+lineHeight-1}px`;line.style.opacity=card.classList.contains('active')?'1':'0'}
function setEditing(active:boolean){card.classList.toggle('active',active);updateLine()}
function focusChanged(){setEditing(document.hasFocus()&&document.activeElement===editor)}
editor.addEventListener('input',()=>{const page=currentPage();page.text=editor.value;page.edited_at=Date.now();note.text=page.text;note.edited_at=page.edited_at;card.classList.toggle('empty',!page.text.trim());updateTime();updateLine();saveSoon()});
editor.addEventListener('focus',focusChanged);editor.addEventListener('blur',focusChanged);
// The textarea can remain the active DOM element after the native window loses focus.
window.addEventListener('blur',()=>setEditing(false));
window.addEventListener('focus',focusChanged);
editor.addEventListener('click',updateLine);editor.addEventListener('keyup',updateLine);editor.addEventListener('scroll',updateLine);
function switchPage(index:number){if(index<0||index>=note.pages.length)return;note.active_page=index;const page=currentPage();note.text=page.text;note.priority=page.priority;note.edited_at=page.edited_at;card.classList.remove('page-turn');void card.offsetWidth;card.classList.add('page-turn');renderPage();saveSoon()}
document.querySelector('#prev-page')!.addEventListener('click',()=>switchPage(note.active_page-1));
document.querySelector('#next-page')!.addEventListener('click',()=>switchPage(note.active_page+1));
document.querySelector('#add-page')!.addEventListener('click',()=>{note.pages.push({id:crypto.randomUUID(),text:'',priority:'gray',edited_at:0});switchPage(note.pages.length-1);editor.focus()});
document.querySelector('#delete-page')!.addEventListener('click',()=>{if(note.pages.length<2)return;note.pages.splice(note.active_page,1);switchPage(Math.min(note.active_page,note.pages.length-1))});
document.querySelector('#add-todo')!.addEventListener('click',()=>void invoke('open_add_todo').catch(error=>{toast.textContent=`无法打开添加窗口：${error}`;toast.hidden=false;window.setTimeout(()=>toast.hidden=true,6000)}));
let listDrag:{id:number;y:number;top:number}|null=null;
todoList.addEventListener('pointerdown',event=>{
  if(event.pointerType!=='mouse'||event.button!==0||(event.target as Element).closest('button,.todo-body')||todoList.scrollHeight<=todoList.clientHeight)return;
  listDrag={id:event.pointerId,y:event.clientY,top:todoList.scrollTop};
  todoList.setPointerCapture(event.pointerId);
  todoList.style.userSelect='none';
});
todoList.addEventListener('pointermove',event=>{if(listDrag?.id===event.pointerId)todoList.scrollTop=listDrag.top-(event.clientY-listDrag.y)});
function endListDrag(){listDrag=null;todoList.style.userSelect=''}
todoList.addEventListener('pointerup',endListDrag);
todoList.addEventListener('pointercancel',endListDrag);
type ResizePart='note'|'todo';
function installHeightHandle(id:string,part:ResizePart){
  const handle=document.querySelector<HTMLElement>(id)!;
  let drag:{pointer:number;y:number;noteHeight:number;todoHeight:number}|null=null;
  handle.addEventListener('pointerdown',event=>{
    if(event.button!==0)return;
    event.preventDefault();event.stopPropagation();
    drag={pointer:event.pointerId,y:event.clientY,noteHeight:note.note_height,todoHeight:todoHeight()};
    handle.setPointerCapture(event.pointerId);
  });
  handle.addEventListener('pointermove',event=>{
    if(!drag||drag.pointer!==event.pointerId)return;
    const change=Math.round(event.clientY-drag.y);
    if(part==='note')note.note_height=Math.max(100,Math.min(1200,drag.noteHeight+change));
    else note.todo_height=Math.max(58,Math.min(1200,drag.todoHeight+change));
    applyHeights();
  });
  const finish=(event:PointerEvent)=>{if(!drag||drag.pointer!==event.pointerId)return;drag=null;saveSoon()};
  handle.addEventListener('pointerup',finish);
  handle.addEventListener('pointercancel',finish);
}
installHeightHandle('#note-resize','note');
installHeightHandle('#todo-resize','todo');
window.addEventListener('beforeunload',()=>{if(saveTimer)void save()});
setInterval(updateTime,30000);
setInterval(renderTodos,60000);

async function restoreWidth(){if(note.window_width){const current=await win.innerSize();const scale=await win.scaleFactor();await win.setSize(new LogicalSize(Math.min(2000,Math.max(200,note.window_width)),current.height/scale))}}
async function restorePosition(){if(!note.position)return;const monitors=await availableMonitors();const size=await win.outerSize();const target=monitors.find(m=>m.name===note.position?.monitor)||monitors.find(m=>{const p=note.position!;return p.x>=m.position.x&&p.x<m.position.x+m.size.width&&p.y>=m.position.y&&p.y<m.position.y+m.size.height})||await currentMonitor();if(!target)return;const x=Math.min(Math.max(note.position.x,target.position.x),target.position.x+target.size.width-size.width);const y=Math.min(Math.max(note.position.y,target.position.y),target.position.y+target.size.height-size.height);await win.setPosition(new PhysicalPosition(x,y))}
async function snap(){if(moving)return;moving=true;try{const pos=await win.outerPosition();const size=await win.outerSize();const monitors=await availableMonitors();const monitor=monitors.find(m=>pos.x>=m.position.x&&pos.x<m.position.x+m.size.width&&pos.y>=m.position.y&&pos.y<m.position.y+m.size.height)||await currentMonitor();if(!monitor)return;const left=monitor.position.x,top=monitor.position.y,right=left+monitor.size.width-size.width,bottom=top+monitor.size.height-size.height;const x=Math.abs(pos.x-left)<20?left:Math.abs(pos.x-right)<20?right:pos.x;const y=Math.abs(pos.y-top)<20?top:Math.abs(pos.y-bottom)<20?bottom:pos.y;if(x!==pos.x||y!==pos.y)await win.setPosition(new PhysicalPosition(x,y));note.position={x,y,monitor:monitor.name||undefined};saveSoon()}finally{moving=false}}

function showToast(message:string){toast.textContent=message;toast.hidden=false;window.setTimeout(()=>toast.hidden=true,8000)}
async function beep(){if(note.mute_app||note.reminder_mode==='popup'||note.notification_volume===0)return;if(note.notification_tone==='custom'){try{const bytes=await invoke<number[]>('load_custom_sound');const url=URL.createObjectURL(new Blob([new Uint8Array(bytes)]));const player=new Audio(url);player.volume=note.notification_volume/100;player.onended=()=>URL.revokeObjectURL(url);player.onerror=()=>URL.revokeObjectURL(url);await player.play()}catch(error){console.error('提示音播放失败',error)}return}const audio=new AudioContext();const oscillator=audio.createOscillator();const gain=audio.createGain();const tones:Record<string,{frequency:number;wave:OscillatorType;duration:number}>={chime:{frequency:880,wave:'sine',duration:.3},bell:{frequency:660,wave:'triangle',duration:.55},soft:{frequency:520,wave:'sine',duration:.4}};const tone=tones[note.notification_tone]||tones.chime;oscillator.type=tone.wave;oscillator.frequency.value=tone.frequency;gain.gain.setValueAtTime(.15*note.notification_volume/100,audio.currentTime);gain.gain.exponentialRampToValueAtTime(.001,audio.currentTime+tone.duration);oscillator.connect(gain).connect(audio.destination);oscillator.start();oscillator.stop(audio.currentTime+tone.duration);oscillator.onended=()=>void audio.close();void audio.resume().catch(console.error)}
async function init(){note=await loadNoteReady();await restoreWidth();render();await restorePosition();windowReady=true;adjustWindowHeight();await win.onMoved(()=>{window.setTimeout(()=>void snap(),120)});await win.onResized(({payload:size})=>{void win.scaleFactor().then(scale=>{const width=Math.round(size.width/scale);if(width>=200&&width<=2000&&width!==note.window_width){note.window_width=width;saveSoon()}})});await win.onFocusChanged(({payload:focused})=>{if(!focused){setEditing(false);return}focusChanged();void invoke<Note>('load_note').then(latest=>{note.opacity=latest.opacity;showOpacity(note.opacity);note.theme=latest.theme;stack.dataset.theme=note.theme;note.todos=latest.todos;note.show_todos=latest.show_todos;note.show_note=latest.show_note;note.reminder_sound=latest.reminder_sound;note.reminder_mode=latest.reminder_mode;note.mute_app=latest.mute_app;note.notification_volume=latest.notification_volume;note.notification_tone=latest.notification_tone;note.sort_order=latest.sort_order;showSections();renderTodos()})});await listen<Theme>('theme-changed',event=>{note.theme=event.payload;stack.dataset.theme=note.theme});await listen<number>('opacity-changed',event=>{note.opacity=event.payload;showOpacity(note.opacity)});await listen<[boolean,boolean]>('visibility-changed',event=>{[note.show_todos,note.show_note]=event.payload;showSections()});await listen<[string,boolean]>('reminder-settings-changed',event=>{[note.reminder_mode,note.mute_app]=event.payload});await listen<[number,string]>('notification-sound-changed',event=>{[note.notification_volume,note.notification_tone]=event.payload});await listen<string>('sort-changed',event=>{note.sort_order=event.payload;renderTodos()});await listen('todos-changed',()=>void invoke<Note>('load_note').then(latest=>{note.todos=latest.todos;renderTodos()}));await listen<string>('todo-reminder',event=>{beep()});await listen('show-tips',()=>showToast('提示：拖动待办标题可移动窗口；便签可用箭头翻页。'));await listen('check-update',()=>showToast('检查更新暂未实现。'))}
void init().catch(error=>{console.error(error);edited.textContent=`加载失败：${String(error).slice(0,80)}`});



