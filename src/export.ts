import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
import './export.css';
const panel = document.querySelector<HTMLElement>('main')!;
const win = getCurrentWindow();
let resizing = Promise.resolve();
new ResizeObserver(() => {
  resizing = resizing.then(async () => {
    const size = await win.innerSize();
    const scale = await win.scaleFactor();
    const height = Math.ceil(panel.getBoundingClientRect().height);
    if (Math.abs(size.width / scale - 420) > 2 || Math.abs(size.height / scale - height) > 2) {
      await win.setSize(new LogicalSize(420, height));
      await invoke('center_popup',{label:'export'});
    }
  }).catch(console.error);
}).observe(panel);
const format = document.querySelector<HTMLSelectElement>('#format')!;
const days = document.querySelector<HTMLSelectElement>('#days')!;
const kinds = document.querySelector<HTMLElement>('#kinds')!;
const extras = document.querySelector<HTMLElement>('#extras')!;
const status = document.querySelector<HTMLElement>('#status')!;
const checked = (root: HTMLElement) => Array.from(root.querySelectorAll<HTMLInputElement>('input:checked')).map(v => v.value);
kinds.addEventListener('change', event => {
  const changed = event.target as HTMLInputElement;
  const all = kinds.querySelector<HTMLInputElement>('input[value="all"]')!;
  const items = Array.from(kinds.querySelectorAll<HTMLInputElement>('input:not([value="all"])'));
  if (changed === all) items.forEach(item => item.checked = all.checked);
  else all.checked = items.every(item => item.checked);
});
document.querySelector<HTMLButtonElement>('#cancel')!.onclick = () => void invoke('close_export');
document.querySelector<HTMLButtonElement>('#export')!.onclick = async () => {
  const selected = checked(kinds).filter(v => v !== 'all');
  if (!selected.length) { status.textContent = '请至少选择一种类型。'; return; }
  const button = document.querySelector<HTMLButtonElement>('#export')!;
  button.disabled = true; status.textContent = '正在准备导出…';
  try {
    status.textContent = await invoke<string>('export_todos', { options: { format: format.value, days: Number(days.value), kinds: selected, extras: checked(extras) } });
  } catch (error) { status.textContent = `导出失败：${String(error)}`; }
  finally { button.disabled = false; }
};
