import {invoke} from '@tauri-apps/api/core';
import {listen} from '@tauri-apps/api/event';
const canvas=document.querySelector<HTMLCanvasElement>('#canvas')!;
const ctx=canvas.getContext('2d')!;
const error=document.querySelector<HTMLElement>('#error')!;
let picture:HTMLImageElement;
let box={x:0,y:0,w:0,h:0};
let anchor:{x:number;y:number}|null=null;
function draw(){
  if(!picture)return;
  ctx.clearRect(0,0,canvas.width,canvas.height);ctx.drawImage(picture,0,0,canvas.width,canvas.height);
  if(box.w&&box.h){ctx.fillStyle='rgba(0,0,0,.48)';ctx.fillRect(0,0,canvas.width,canvas.height);ctx.save();ctx.beginPath();ctx.rect(box.x,box.y,box.w,box.h);ctx.clip();ctx.drawImage(picture,0,0,canvas.width,canvas.height);ctx.restore();ctx.strokeStyle='#fff';ctx.lineWidth=2;ctx.strokeRect(box.x,box.y,box.w,box.h)}
}
function point(event:PointerEvent){const rect=canvas.getBoundingClientRect();return{x:Math.max(0,Math.min(canvas.width,(event.clientX-rect.left)*canvas.width/rect.width)),y:Math.max(0,Math.min(canvas.height,(event.clientY-rect.top)*canvas.height/rect.height))}}
canvas.addEventListener('pointerdown',event=>{anchor=point(event);canvas.setPointerCapture(event.pointerId)});
canvas.addEventListener('pointermove',event=>{if(!anchor)return;const end=point(event);box={x:Math.min(anchor.x,end.x),y:Math.min(anchor.y,end.y),w:Math.abs(end.x-anchor.x),h:Math.abs(end.y-anchor.y)};draw()});
canvas.addEventListener('pointerup',()=>{anchor=null;draw()});
async function load(){
  error.textContent='';const result=await invoke<[string,number[]]|null>('load_background_source');
  if(!result){error.textContent='请先从托盘选择背景图片';return}
  const url=URL.createObjectURL(new Blob([new Uint8Array(result[1])],{type:result[0]}));
  const image=new Image();image.onload=()=>{picture=image;const scale=Math.min(1,700/image.width,450/image.height);canvas.width=Math.round(image.width*scale);canvas.height=Math.round(image.height*scale);box={x:0,y:0,w:canvas.width,h:canvas.height};draw();URL.revokeObjectURL(url)};
  image.onerror=()=>{error.textContent='图片加载失败';URL.revokeObjectURL(url)};image.src=url;
}
void listen('popup-opened',()=>void load()).then(()=>void load());
document.querySelector('#cancel')!.addEventListener('click',()=>void invoke('hide_popup',{label:'crop-background'}).catch(reason=>error.textContent=String(reason)));
document.querySelector('#save')!.addEventListener('click',()=>{
  if(!picture||box.w<2||box.h<2){error.textContent='请选取有效范围';return}
  const scaleX=picture.width/canvas.width,scaleY=picture.height/canvas.height;
  const output=document.createElement('canvas');output.width=Math.round(box.w*scaleX);output.height=Math.round(box.h*scaleY);
  output.getContext('2d')!.drawImage(picture,box.x*scaleX,box.y*scaleY,box.w*scaleX,box.h*scaleY,0,0,output.width,output.height);
  output.toBlob(blob=>{if(!blob){error.textContent='无法生成裁切图片';return}void blob.arrayBuffer().then(buffer=>invoke('save_cropped_background',{bytes:Array.from(new Uint8Array(buffer))})).then(()=>invoke('hide_popup',{label:'crop-background'})).catch(reason=>error.textContent=String(reason))},'image/png');
});
