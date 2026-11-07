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
  const [mqtt, setMqtt] = useState(null);
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
            setMqtt((m) => m || { logs_to_mqtt: r.logs_to_mqtt, node_log_interval_s: r.node_log_interval_s });
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
  const saveMqtt = async (changes) => {
    const r = await run(() => api.logs.update(changes), 'Saved');
    if (r) setMqtt({ logs_to_mqtt: r.logs_to_mqtt, node_log_interval_s: r.node_log_interval_s });
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
    <${Section} title="Log lines to MQTT">
      <p class="muted">The gateway's lines go to <code>canbus/gateway/log</code> (every 10 s, what came since), the nodes' to <code>${'canbus/device_log/<device>'}</code>: the gateway downloads their logs over CAN one after the other, and a node drops what was downloaded. One line per message, not retained.</p>
      ${mqtt && html`<div class="row">
        <label class="switch"><input type="checkbox" checked=${mqtt.logs_to_mqtt} onChange=${(e) => saveMqtt({ logs_to_mqtt: e.target.checked })} /><span></span>${mqtt.logs_to_mqtt ? 'On' : 'Off'}</label>
        <label>Node logs every <select value=${mqtt.node_log_interval_s} onChange=${(e) => saveMqtt({ node_log_interval_s: +e.target.value })}>
          ${[[60, '1 minute'], [300, '5 minutes'], [900, '15 minutes'], [3600, 'hour'], [0, 'never']].map(([s, label]) => html`<option value=${s}>${label}</option>`)}
          ${![60, 300, 900, 3600, 0].includes(mqtt.node_log_interval_s) && html`<option value=${mqtt.node_log_interval_s}>${mqtt.node_log_interval_s} s</option>`}
        </select></label>
      </div>`}
    <//>
    <${Section} title="CAN traffic to MQTT">
      <p class="muted">Every CAN frame sent and received also goes to <code>${'canbus/log/0x<id>'}</code> (<code>0x</code> and the data bytes, <code>R…</code> for a request), as the old gateway did. For debugging: it is a lot of messages. Off again after a restart. Frames can be sent to the bus with <code>${'canbus/debug/<id>'}</code>.</p>
      <label class="switch"><input type="checkbox" checked=${canToMqtt} onChange=${(e) => toggleCan(e.target.checked)} /><span></span>${canToMqtt ? 'On' : 'Off'}</label>
    <//>`;
}
