import { convertFileSrc } from '@tauri-apps/api/core';
import { AppEntry } from '../bridge';

export function appInitials(appName: string) {
  return appName
    .split(/\s+/)
    .filter(Boolean)
    .slice(0, 2)
    .map((part) => part[0]?.toUpperCase() ?? '')
    .join('') || '?';
}

export function boxArtSrc(app: AppEntry) {
  if (!app.boxArtUrl || app.boxArtUrl.startsWith('qrc:')) {
    return '';
  }
  if (app.boxArtUrl.startsWith('data:image/')) {
    return app.boxArtUrl;
  }

  try {
    const url = new URL(app.boxArtUrl);
    if (url.protocol === 'file:') {
      let path = decodeURIComponent(url.pathname);
      if (url.hostname) {
        path = `//${url.hostname}${path}`;
      }
      if (/^\/[A-Za-z]:\//.test(path)) {
        path = path.slice(1);
      }
      return convertFileSrc(path);
    }
    if (url.protocol === 'http:' || url.protocol === 'https:') {
      return app.boxArtUrl;
    }
  }
  catch {
    return '';
  }

  return '';
}
