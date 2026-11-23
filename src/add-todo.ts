import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWindow, LogicalSize, PhysicalPosition, currentMonitor } from '@tauri-apps/api/window';
import './style.css';

const card=document.querySelector<HTMLElement>('#add-card')!;
const form=document.querySelector<HTMLFormElement>('#todo-form')!;
const error=document.querySelector<HTMLElement>('#form-error')!;
const win=getCurrentWindow();
let resizeJob=Promise.resolve();
function fitWindow(){resizeJob=resizeJob.then(async()=>{
  const scale=await win.scaleFactor();
  const current=await win.innerSize();
  const monitor=await currentMonitor();
  const needed=Math.ceil(form.getBoundingClientRect().height+card.querySelector<HTMLElement>('.add-drag')!.offsetHeight+parseFloat(getComputedStyle(card).paddingBottom));
  const max=monitor?Math.floor(monitor.workArea.size.height/scale)-16:needed;
  const height=Math.min(max,Math.max(needed,180));
  if(Math.abs(current.height/scale-height)>1)await win.setSize(new LogicalSize(current.width/scale,height));
  if(monitor){const position=await win.outerPosition();const size=await win.outerSize();const top=monitor.workArea.position.y;const bottom=top+monitor.workArea.size.height;const y=Math.min(Math.max(position.y,top),Math.max(top,bottom-size.height));if(y!==position.y)await win.setPosition(new PhysicalPosition(position.x,y))}
}).catch(console.error);return resizeJob}
let editingId:string|null=null;
const duration=form.elements.namedItem('duration') as HTMLSelectElement;
function syncDuration(){const custom=document.querySelector<HTMLInputElement>('#custom-duration')!;custom.hidden=duration.value!=='custom';custom.disabled=custom.hidden;void fitWindow();if(!custom.hidden)custom.focus()}
duration.addEventListener('change',syncDuration);
document.querySelector<HTMLDetailsElement>('.reminder-details')!.addEventListener('toggle',()=>void fitWindow());
new ResizeObserver(()=>void fitWindow()).observe(form);

async function syncAppearance(){
  const note=await invoke<{theme:string;opacity:number}>('load_note');
  card.dataset.theme=note.theme;
  card.style.setProperty('--opacity','0.8');
  card.style.setProperty('--opacity-level','0.8');
}
function parseDate(value:string):string|null{const match=/^(\d{4})[年/\-.](\d{1,2})[月/\-.](\d{1,2})日?$/.exec(value.trim());if(!match)return null;const year=Number(match[1]),month=Number(match[2]),day=Number(match[3]);const date=new Date(year,month-1,day);if(date.getFullYear()!==year||date.getMonth()!==month-1||date.getDate()!==day)return null;return `${year}-${String(month).padStart(2,'0')}-${String(day).padStart(2,'0')}`}
function syncDatePlaceholders(){for(const input of Array.from(form.querySelectorAll<HTMLInputElement>('input[type=date],input[type=datetime-local]')))input.closest('.date-field')?.classList.toggle('has-value',Boolean(input.value))}
form.addEventListener('change',syncDatePlaceholders);
form.addEventListener('input',syncDatePlaceholders);
for(const input of Array.from(form.querySelectorAll<HTMLInputElement>('input[type=date],input[type=time],input[type=datetime-local]')))input.addEventListener('click',()=>{try{input.showPicker()}catch{}});
syncDatePlaceholders();

document.querySelector('#cancel')!.addEventListener('click',()=>void invoke('close_add_todo'));
form.addEventListener('submit',event=>{
  event.preventDefault();
  error.textContent='';
  const data=new FormData(form);
  const get=(key:string)=>String(data.get(key)||'').trim();
  const dateText=get('date'),untilText=get('repeat_until'),date=dateText?parseDate(dateText):null,until=untilText?parseDate(untilText):null,time=get('time'),repeat=get('repeat');
  if((dateText&&!date)||(untilText&&!until)){error.textContent='日期请填写为 年/月/日';return}
  if(time&&!date){error.textContent='填写时间时也请选择日期';return}
  const due_at=date?new Date(`${date}T${time||'00:00'}`).getTime():null;
  const repeat_until=until?new Date(`${until}T23:59:59`).getTime():null;
  const remind_offsets=data.getAll('remind_offsets').map(Number);
  const remind_at=get('remind_at')?new Date(get('remind_at')).getTime():null;
  if(remind_at!==null&&(!Number.isFinite(remind_at)||(due_at!==null&&remind_at>due_at))){error.textContent='自定义提醒日期不能晚于待办时间';return}
  if((repeat!=='none'||remind_offsets.length>0)&&!date){error.textContent='重复或提醒需要日期';return}
  if(remind_offsets.length>0&&!time){error.textContent='设置提醒时请选择时间';return}
  const duration_minutes=duration.value==='custom'?Number(get('custom_duration')):Number(duration.value);
  if(!Number.isInteger(duration_minutes)||duration_minutes<1||duration_minutes>10080){error.textContent='时长需为 1–10080 分钟';return}
  const input={title:get('title'),memo:get('memo'),due_at,time_specified:Boolean(time),repeat,repeat_until,remind_before:-1,remind_offsets,remind_at,remind_method:'app',duration_minutes,priority:get('priority'),kind:get('kind')};
  const command=editingId?'update_todo':'add_todo';
  void invoke(command,{input,...(editingId?{id:editingId}:{})}).then(()=>{form.reset();syncDuration();syncDatePlaceholders()}).catch(reason=>{error.textContent=String(reason)});
});

void syncAppearance();
void listen('theme-changed',()=>void syncAppearance());
void listen('todo-add-opened',()=>{editingId=null;form.reset();document.querySelector<HTMLDetailsElement>('.reminder-details')!.open=false;syncDuration();syncDatePlaceholders();error.textContent='';document.querySelector('#submit-todo')!.textContent='添加';void syncAppearance();void fitWindow();form.querySelector<HTMLInputElement>('[name=title]')?.focus()});
type ExistingTodo={id:string;title:string;memo:string;due_at:number|null;time_specified:boolean;repeat:string;repeat_until:number|null;remind_offsets:number[];remind_at:number|null;remind_before:number;duration_minutes:number;priority:string;kind:string};
async function openEdit(id:string){editingId=id;form.reset();const note=await invoke<{todos:ExistingTodo[]}>('load_note');const todo=note.todos.find(item=>item.id===editingId);if(!todo)return;const set=(name:string,value:string)=>{(form.elements.namedItem(name) as HTMLInputElement|HTMLSelectElement).value=value};set('title',todo.title);set('memo',todo.memo);if(todo.due_at){const date=new Date(todo.due_at);set('date',`${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`);if(todo.time_specified)set('time',`${String(date.getHours()).padStart(2,'0')}:${String(date.getMinutes()).padStart(2,'0')}`)}set('repeat',todo.repeat);if(todo.repeat_until){const date=new Date(todo.repeat_until);set('repeat_until',`${date.getFullYear()}-${String(date.getMonth()+1).padStart(2,'0')}-${String(date.getDate()).padStart(2,'0')}`)}for(const input of Array.from(form.querySelectorAll<HTMLInputElement>('[name=remind_offsets]')))input.checked=(todo.remind_offsets||[]).includes(Number(input.value));if(todo.remind_at){const date=new Date(todo.remind_at);set('remind_at',date.getFullYear()+'-'+String(date.getMonth()+1).padStart(2,'0')+'-'+String(date.getDate()).padStart(2,'0')+'T'+String(date.getHours()).padStart(2,'0')+':'+String(date.getMinutes()).padStart(2,'0'))}document.querySelector<HTMLDetailsElement>('.reminder-details')!.open=Boolean(todo.remind_offsets?.length||todo.remind_at);const minutes=todo.duration_minutes||15;set('duration',[15,30,60,120].includes(minutes)?String(minutes):'custom');if(duration.value==='custom')set('custom_duration',String(minutes));set('priority',todo.priority);set('kind',todo.kind);syncDuration();syncDatePlaceholders();error.textContent='';document.querySelector('#submit-todo')!.textContent='保存';void syncAppearance();form.querySelector<HTMLInputElement>('[name=title]')?.focus()}
void listen<string>('todo-edit-opened',event=>void openEdit(event.payload));
void invoke<ExistingTodo|null>('form_todo').then(todo=>{if(todo)void openEdit(todo.id)});


