// Small shared pieces: the `html` template tag, data loading, notifications.
import { h } from 'preact';
import { useCallback, useEffect, useRef, useState } from 'preact/hooks';
import htm from 'htm';

export const html = htm.bind(h);

// ---- notifications -------------------------------------------------------

let pushToast = () => {};

export function notify(message, kind = 'ok') {
  pushToast(message, kind);
}

export function Toasts() {
  const [toasts, setToasts] = useState([]);
  useEffect(() => {
    let id = 0;
    pushToast = (message, kind) => {
      const toast = { id: ++id, message, kind };
      setToasts((t) => [...t, toast]);
      setTimeout(() => setToasts((t) => t.filter((x) => x !== toast)), kind === 'error' ? 8000 : 3000);
    };
    return () => (pushToast = () => {});
  }, []);
  return html`<div class="toasts">
    ${toasts.map((t) => html`<div key=${t.id} class=${'toast ' + t.kind}>${t.message}</div>`)}
  </div>`;
}

// Runs an API call and reports the outcome. Returns the result, or undefined
// if it failed.
export async function run(call, success) {
  try {
    const result = await call();
    if (success) notify(success);
    return result;
  } catch (e) {
    notify(e.message, 'error');
    return undefined;
  }
}

// ---- loading -------------------------------------------------------------

// Loads `load()` when the component appears and every `every` ms after that
// (the requests never overlap). `reload()` loads right away.
export function useLoad(load, every) {
  const [state, setState] = useState({ data: undefined, error: undefined });
  const loadRef = useRef(load);
  loadRef.current = load;
  const reload = useCallback(async () => {
    try {
      setState({ data: await loadRef.current(), error: undefined });
    } catch (e) {
      setState((s) => ({ data: s.data, error: e }));
    }
  }, []);
  useEffect(() => {
    let stopped = false;
    let timer;
    const loop = async () => {
      await reload();
      if (every && !stopped) timer = setTimeout(loop, every);
    };
    loop();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, [every, reload]);
  return { ...state, reload };
}

// ---- layout --------------------------------------------------------------

export const Section = ({ title, children, aside }) => html`
  <section class="card">
    <header><h2>${title}</h2>${aside}</header>
    ${children}
  </section>`;

export const Loading = ({ error }) =>
  error
    ? html`<p class="error">${error.message}</p>`
    : html`<p class="muted">Loading…</p>`;

// A form field with a label.
export const Field = ({ label, hint, children }) => html`
  <label class="field">
    <span>${label}</span>
    ${children}
    ${hint && html`<small>${hint}</small>`}
  </label>`;

export const Button = ({ children, kind, onClick, disabled }) => {
  const [busy, setBusy] = useState(false);
  const click = async (e) => {
    setBusy(true);
    try {
      await onClick(e);
    } finally {
      setBusy(false);
    }
  };
  return html`<button class=${kind || ''} disabled=${disabled || busy} onClick=${click}>${children}</button>`;
};

// A byte as `0x0a`.
export const hex = (n) => '0x' + n.toString(16).padStart(2, '0');

// Minutes as "3 min", "2 h", "1 d".
export function ago(minutes) {
  if (minutes < 1) return 'now';
  if (minutes < 60) return `${minutes} min`;
  if (minutes < 60 * 24) return `${Math.floor(minutes / 60)} h`;
  return `${Math.floor(minutes / 1440)} d`;
}

export function duration(seconds) {
  const d = Math.floor(seconds / 86400);
  const hrs = Math.floor((seconds % 86400) / 3600);
  const m = Math.floor((seconds % 3600) / 60);
  return d ? `${d} d ${hrs} h` : hrs ? `${hrs} h ${m} min` : `${m} min`;
}
