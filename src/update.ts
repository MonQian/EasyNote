import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';
import { getCurrentWindow } from '@tauri-apps/api/window';
import './popup.css';
import './update.css';

type Release = { tag_name: string };
const releasesApi = 'https://api.github.com/repos/MonQian/EasyNote/releases/latest';

function versionParts(value: string): number[] {
  const match = /^v?(\d+)\.(\d+)\.(\d+)$/.exec(value);
  if (!match) throw new Error(`无法识别版本号：${value}`);
  return match.slice(1).map(Number);
}

function isNewer(latest: string, current: string): boolean {
  const a = versionParts(latest);
  const b = versionParts(current);
  for (let i = 0; i < 3; i++) {
    if (a[i] !== b[i]) return a[i] > b[i];
  }
  return false;
}

const status = document.querySelector<HTMLElement>('#status')!;
const versions = document.querySelector<HTMLElement>('#versions')!;
const retry = document.querySelector<HTMLButtonElement>('#retry')!;
const release = document.querySelector<HTMLButtonElement>('#release')!;
let checking = false;

async function check(): Promise<void> {
  if (checking) return;
  checking = true;
  retry.disabled = true;
  release.hidden = true;
  status.textContent = '正在连接 GitHub…';
  versions.textContent = '';
  try {
    const controller = new AbortController();
    const timer = window.setTimeout(() => controller.abort(), 10000);
    let latestVersion: string;
    try {
      const response = await fetch(releasesApi, {
        headers: { Accept: 'application/vnd.github+json' },
        signal: controller.signal,
        cache: 'no-store',
      });
      if (!response.ok) throw new Error(`GitHub 返回 HTTP ${response.status}`);
      const result = await response.json() as Release;
      latestVersion = result.tag_name;
    } finally {
      window.clearTimeout(timer);
    }
    const currentVersion = await getVersion();
    const updateAvailable = isNewer(latestVersion, currentVersion);
    status.textContent = updateAvailable ? '发现新版本' : '当前已是最新版本';
    versions.textContent = `当前版本 ${currentVersion} · 最新版本 ${latestVersion}`;
    release.hidden = !updateAvailable;
  } catch (error) {
    status.textContent = `检查失败：${String(error)}`;
  } finally {
    checking = false;
    retry.disabled = false;
  }
}

retry.addEventListener('click', () => void check());
release.addEventListener('click', async () => {
  try { await invoke('open_update_page'); }
  catch (error) { status.textContent = String(error); }
});
document.querySelector('#close')!.addEventListener('click', async () => {
  try { await getCurrentWindow().hide(); }
  catch (error) { status.textContent = `无法关闭窗口：${String(error)}`; }
});
void listen('check-update', () => void check()).then(async () => {
  if (await getCurrentWindow().isVisible()) void check();
});
