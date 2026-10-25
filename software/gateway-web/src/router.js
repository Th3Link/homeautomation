// Path-based routing on top of the History API. The gateway answers each of
// the page paths below with the same index.html.
import { h } from 'preact';
import { useEffect, useState } from 'preact/hooks';

const listeners = new Set();

export function navigate(path) {
  if (path !== location.pathname) history.pushState(null, '', path);
  listeners.forEach((l) => l());
}

addEventListener('popstate', () => listeners.forEach((l) => l()));

// The path of the page being shown, kept up to date.
export function usePath() {
  const [path, setPath] = useState(location.pathname);
  useEffect(() => {
    const update = () => setPath(location.pathname);
    listeners.add(update);
    return () => listeners.delete(update);
  }, []);
  return path;
}

// Finds the route whose pattern (`/devices/:device`) matches `path`.
export function match(routes, path) {
  const parts = path.split('/').filter(Boolean);
  for (const route of routes) {
    const pattern = route.path.split('/').filter(Boolean);
    if (pattern.length !== parts.length) continue;
    const params = {};
    const ok = pattern.every((p, i) => {
      if (p.startsWith(':')) {
        params[p.slice(1)] = decodeURIComponent(parts[i]);
        return true;
      }
      return p === parts[i];
    });
    if (ok) return { route, params };
  }
  return null;
}

export function Link({ href, children, ...rest }) {
  const click = (e) => {
    if (e.metaKey || e.ctrlKey || e.shiftKey || e.button !== 0) return;
    e.preventDefault();
    navigate(href);
  };
  return h('a', { href, onClick: click, ...rest }, children);
}
