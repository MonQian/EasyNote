import {invoke} from '@tauri-apps/api/core';
const input=document.querySelector<HTMLInputElement>('#shortcut')!;
const error=document.querySelector<HTMLElement>('#error')!;
void invoke<{click_through_shortcut:string}>('load_note').then(note=>input.value=note.click_through_shortcut);
input.addEventListener('keydown',event=>{
  event.preventDefault();
  if(['Control','Alt','Shift'].includes(event.key))return;
  const key=event.key.length===1?event.key.toUpperCase():event.key.toUpperCase();
  if(!/^(?:[A-Z0-9]|F(?:[1-9]|1[0-2]))$/.test(key)){error.textContent='请使用字母、数字或 F1-F12';return}
  const mods=[event.ctrlKey&&'Ctrl',event.altKey&&'Alt',event.shiftKey&&'Shift'].filter(Boolean);
  if(mods.length<2){error.textContent='请至少按住两个修饰键';return}
  input.value=[...mods,key].join('+');error.textContent='';
});
document.querySelector('#save')!.addEventListener('click',()=>void invoke('set_click_through_shortcut',{value:input.value}).then(()=>invoke('hide_popup',{label:'shortcut-settings'})).catch(reason=>error.textContent=String(reason)));
