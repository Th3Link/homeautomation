// The gateway's log: the latest lines it printed, as they come.
import { useEffect, useRef, useState } from 'preact/hooks';
import * as api from '../api.js';
import { html, run, Section } from '../ui.js';

// Lines kept in the page; older ones scroll away.
const KEEP = 2000;

export function Log() {
  const [lines, setLines] = useState([]);
  const [error, setError] = useState(null);
  const [paused, setPaused] = useState(false);
  const [filter, setFilter] = useState('');
  const [canToMqtt, setCanToMqtt] = useState(false);
  const from = useRef(0);
  const box = useRef(null);
  const follow = useRef(true);
  const pausedRef = useRef(false);
  pausedRef.current = paused;

  useEffect(() => {
    let stopped = false;
    let timer;
    const poll = async () => {
      if (!pausedRef.current) {
        try {
          // Catch up in chunks, then wait.
          for (let i = 0; i < 10; i++) {
            const r = await api.logs.read(from.current);
            setCanToMqtt(r.can_to_mqtt);
            setError(null);
            if (r.from > from.current && from.current > 0) setLines((l) => [...l, '… (older lines were overwritten)']);
            from.current = r.next;
            if (r.text) setLines((l) => [...l, ...r.text.replace(/\n$/, '').split('\n')].slice(-KEEP));
            if (!r.text) break;
          }
        } catch (e) {
          setError(e.message);
        }
      }
      if (!stopped) timer = setTimeout(poll, 2000);
    };
    poll();
    return () => {
      stopped = true;
      clearTimeout(timer);
    };
  }, []);

  // Stays at the bottom unless scrolled up.
  useEffect(() => {
    const el = box.current;
    if (el && follow.current) el.scrollTop = el.scrollHeight;
  }, [lines, filter]);
  const onScroll = () => {
    const el = box.current;
    follow.current = el.scrollHeight - el.scrollTop - el.clientHeight < 30;
  };

  const shown = filter ? lines.filter((l) => l.toLowerCase().includes(filter.toLowerCase())) : lines;
  const toggleCan = async (on) => {
    const r = await run(() => api.logs.setCanToMqtt(on), on ? 'CAN frames go to MQTT' : 'CAN frames no longer go to MQTT');
    if (r) setCanToMqtt(r.can_to_mqtt);
  };
  const save = () => {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(new Blob([lines.join('\n') + '\n'], { type: 'text/plain' }));
    a.download = `gateway-log-${new Date().toISOString().slice(0, 19).replace(/:/g, '')}.txt`;
    a.click();
    URL.revokeObjectURL(a.href);
  };

  return html`
    <${Section} title="Log" aside=${html`<div class="buttons">
      <button onClick=${() => setPaused(!paused)}>${paused ? 'Resume' : 'Pause'}</button>
      <button onClick=${() => setLines([])}>Clear view</button>
      <button onClick=${save} disabled=${!lines.length}>Save as file</button>
    </div>`}>
      <p class="muted">What the gateway printed on its console, newest at the bottom; the gateway keeps the last few kilobytes. Times are seconds since the start.</p>
      <div class="row tight">
        <input type="search" placeholder="Filter, e.g. mqtt or automation" aria-label="Filter" value=${filter} onInput=${(e) => setFilter(e.target.value)} />
      </div>
      <pre class="log console" ref=${box} onScroll=${onScroll}>${shown.join('\n') || (error ? '' : 'Waiting for lines…')}</pre>
      ${error && html`<p class="error">${error}</p>`}
    <//>
    <${Section} title="CAN traffic to MQTT">
      <p class="muted">Every CAN frame sent and received also goes to <code>${'canbus/log/0x<id>'}</code> (<code>0x</code> and the data bytes, <code>R…</code> for a request), as the old gateway did. For debugging: it is a lot of messages. Off again after a restart. Frames can be sent to the bus with <code>${'canbus/debug/<id>'}</code>.</p>
      <label class="switch"><input type="checkbox" checked=${canToMqtt} onChange=${(e) => toggleCan(e.target.checked)} /><span></span>${canToMqtt ? 'On' : 'Off'}</label>
    <//>`;
}
