# Automation UI: requirements for a redesign

This is the brief for designing the *Automation* part of the gateway's web
UI from scratch. It says what the UI has to do, what it works with and
what it must not do. It doesn't prescribe a layout; the current
implementation (`src/pages/automation/`) is one possible answer, not the
target.

## 1. Context

- **The device**: a CAN-to-MQTT gateway (ESP32 at 160 MHz, a few hundred KB of RAM shared by everything)
  in a house's switch cabinet. On the CAN bus are wall switches with
  buttons, a motion detector (PIR) and a brightness sensor; lamp
  controllers (up to 24 lamps each, fading by themselves); relay nodes
  (relays and roller shutters).
- **The automation** runs on the gateway itself, without network, MQTT or
  Home Assistant: button bindings, motion lights, schedules. It replaces
  rules that ran in openHAB.
- **The web UI** is served by the gateway itself: HTTPS with Basic Auth,
  in the home network, often **without internet access**. It has other
  pages (devices, sensors, MQTT, gateway, HTTPS) that share one shell
  (top navigation, `style.css`, toasts, form helpers in `src/ui.js`).
- **The user**: the installer/owner, one technical person, at a
  **desktop browser** (a laptop next to a switch when learning buttons).
  The gateway UI is for setting up, testing and troubleshooting, **not
  for daily use**: the household switches with the wall switches and
  Home Assistant. No phone layout for configuring, no separate view for
  other people.
- **The size of the job**: one house today has 30 wall switches (5 of them
  with an extension board, so 4 or 8 buttons each), 6 lamp controllers,
  3 relay nodes with shutters and relays. The UI must stay usable at that
  size and be generic for any other installation.

## 2. The building blocks (data model)

Exact JSON shapes and routes: [`../../gateway-core/docs/api-rest.md`](../../gateway-core/docs/api-rest.md#automation).
Every item is validated on the gateway; a refused one answers `400` with
`{error: "invalid", message, field}` (`field` like `levels`,
`windows.end`, `actions.name`).

**Names** (outputs, motion lights, schedules): 1–20 characters
`A–Z a–z 0–9 - _`. They are the identity: references between items and
the MQTT topics use them. Motion lights and schedules share one namespace;
outputs have their own. There is no rename on the gateway: renaming is
"save under the new name, delete the old one", and every reference to the
old name breaks (see §5).

**Device references** are `{type, id}` (type 0–63, id 1–255). `/devices`
lists the known devices with a user-given `name`, `type_name`
(`LegacyLamps`, `Relais`, `Button`, …), `device_type`, `device_id`. A
reference to a device that isn't listed must still be possible (entered
by type and id).

### 2.1 Outputs (max 48)

What gets switched. Five kinds; the kind of an existing output is fixed.

| Kind | Fields | State shown |
|---|---|---|
| **Light** (levels) | `groups`: 1–8 lamp groups, each `device` (a lamp controller, type 3), optional `bank` (0–255), `mask` (24-bit, ≥ 1 bit: which lamps); `levels`: 1–15 rows, each one value 0–255 **per group**; `default_level` (1..number of levels: what *on* uses the first time) | `level` 0..`top` (0 = off) |
| **Dimmer** | `lamps` (one lamp group as above), `default_value` 1–255 | `level` 0–255 |
| **Relay** | `device`, `channel` 0–255, `bank` 0–255 | on/off |
| **Roller shutter** | `device`, `channel` 0–255, `bank` 0–255, `travel_ms` (full travel, 1 ms–4.6 h), `up` (1 or 2: which direction is up, depends on wiring) | `movement`: up / down / stopped. No position is known |
| **Group** | `members`: 1–16 output names, no groups | none of its own |

A light is the important case: a room with e.g. white and red lamps on
one or more controllers, each level a combination of values. The level
table today comes from a generator (a ramp `2 → vmax` with exponent 3,
fine steps at the dark end), then single values are tuned by hand. The
controllers take **one change per second**; level changes are queued.

### 2.2 Button bindings (max 128)

`trigger` → up to 4 `actions`, run in order. The same trigger can have
several bindings; all run.

- **Trigger**: `device` (the switch), `button` 0–31 (today 0–7: two
  rockers × up/down on the base board, 4–7 on an extension board),
  `event`: `single`, `double`, `triple`, `hold` (repeats every 0.5 s while
  held), `released` (let go after holding).
- **Actions**:
  - `output`: a name and an operation; which operations fit depends on
    the output's kind:
    - light: `toggle`, `on` (last level), `off`, `{set: n}`, `max`,
      `inc`, `dec`
    - dimmer: same, with a value 0–255
    - relay: `toggle`, `on`, `off`, `{on_for: ms}`
    - shutter: `up` (up, or stop if moving), `down`, `up_hold`,
      `down_hold` (while held), `stop`
    - group: `toggle`, `on`, `off`, `up`, `down`, `stop`
  - `automation`: a motion light or schedule name and `enable` /
    `disable` / `toggle`;
  - `nightlight`: a switch device and a value 0–255 (the switch's own
    LED).

Typical: a switch has 2–4 buttons, each with 3–6 bindings (press once:
on/off; hold: dim; let go; three times: disable/enable the motion light).
A house has ~10–20 switches, so 60–128 bindings. **Bindings are mostly
thought of per switch and button**, not as a flat list.

Bindings have no name; they are identified by a `slot` number (0–127).

### 2.3 Motion lights (max 12)

`detectors` (1–4 devices with a PIR), `output` (light, dimmer or relay;
not shutter or group), optional `level` (what to switch on to; default:
the last level), `timeout_s` 1–86400 (off after no motion for…),
`lockout_s` 0–3600 (ignore motion after off), `conditions` (0–3, all must
hold; one that can't be evaluated counts as holding):

- `dark` / `bright`: `sensor` (a device with a brightness sensor), `lux`
- `between`: `from`, `to` (each a time, see below; may span midnight)
- `sun_below` / `sun_above`: `degrees` −90..90

State: `idle` (waiting), `auto` (on by motion, `remaining_s` until off),
`manual` (switched on some other way: no timer until switched off),
`lockout` (`remaining_s`). `enabled` on/off is persisted and can also be
switched by a button, MQTT and (later) Home Assistant.

### 2.4 Schedules (max 8)

`output` (light, dimmer or relay) and 1–4 `windows`. A window:

- `start`: a **time**, either `{clock: {hour, minute}}` or
  `{sun: {event, offset_min −720..720}}` with `event` one of
  astronomical/nautical/civil dawn, sunrise, noon, sunset,
  civil/nautical/astronomical dusk;
- `days`: weekday bit mask Mo..Su, at least one;
- optional `only_if`: one condition (as above), checked once at the start;
- `end`: 1–3 ends, the first reached switches off: `{at: time}` or
  `{bright: {sensor, lux, for_min}}`. **At least one must be `at`**.

Only the edges switch (a manual off inside a window sticks). After a
restart or enabling inside a window, it switches on. Nothing happens while
the clock is unknown. State: `running` / `idle`, `enabled`.

### 2.5 Global

- **Master switch** `enabled`: off = nothing switches by itself; manual
  commands still work. Persisted. Used during migration.
- **Last button**: the last button event seen on the bus (`trigger`,
  `age_s`): for learning a binding and for "is this switch heard at all".
- **Location and time** (latitude, longitude, POSIX TZ, today's sun
  times, local time): today on the *Gateway* page; schedules and sun
  conditions depend on it.
- **Loop guard**: > 60 commands in one second pause the automation for
  10 s (logged on the console only, not visible in the API today).

## 3. Tasks the UI must support

Ordered by importance. Each must be doable without reading docs.

1. **Configure a switch** (the most frequent and most tedious job, see
   W5): which actions each button of a switch does on each click type,
   for one switch or several switches that do the same.
   - Find the switch by name or by *learning* it (press a button on the
     wall; the UI fills in switch, button and click type; waits ~30 s).
   - See at once everything the switch does: all buttons × all click
     types, empty ones too.
   - Common patterns (a rocker for a light: up on, down off, hold
     brighter/darker; a rocker for a shutter) in a few steps.
2. **Test and watch**: every output's state, switch it by hand (light:
   level up/down and a level; dimmer: value; relay: on/off; shutter:
   up/stop/down; group: on/off); every motion light and schedule with
   its state (incl. time left) and enable/disable; the master switch.
   This is a test panel for the installer, not a dashboard.
3. **Set up a light**: lamp groups across controllers (select lamps of a
   24-lamp controller; the device page already shows lamps as toggles),
   the level table (generate a ramp, tune single values, see the result
   per level), the default level, and **try a level live** on the real
   lamps while tuning.
4. **Set up a motion light** and **watch it** (state, time left,
   conditions currently met or not, e.g. current lux vs threshold).
5. **Set up a schedule** and **check it**: what will it do today / this
   week (resolved times for sun events), is it running now, why not.
6. **Migrate**: import a JSON file (generated from the openHAB config),
   see per item what worked and what failed and why; then enable the
   master switch.
7. **Back up / restore**: export everything as one JSON file; import
   with or without wiping first.
8. **Troubleshoot** "the light didn't come on": last button seen, master
   switch, motion light state and conditions, schedule's next times,
   references to missing outputs.
9. **Change and clean up**: rename an output without silently breaking
   bindings; delete with a clear warning of what uses it.

## 4. The owner's wishes

From the owner, after using the first version. The first version is "not
intuitive" and the subject is complex; the redesign has to make it
approachable without hiding what it can do. Everything should be tidier.

- **W1 – Explain as you go.** Every block says in one or two sentences
  what it does and how it behaves (motion light: the states, the
  lockout, manual override; schedule: only the edges switch). Defaults
  that work, advanced fields folded away. The owner "had no idea how to
  set up a motion light".
- **W2 – Find a lamp (explore mode).** When choosing the lamps of a lamp
  group (the 24-bit mask of a controller), switch single lamps on the
  real controller to see which one is which: e.g. hover/select a bit →
  that lamp lights up (others as they were), or step through the lamps
  one by one. Uses the existing lamp command
  (`PUT /devices/:device/lamps/:bank` with `value` and `bitmask`); the
  controller takes one change per second.
- **W3 – Lamp names.** Each lamp of a controller gets a name ("WZ white
  2"), set once (ideally while exploring, W2) and shown wherever lamps
  are chosen. Needs a model change (§6).
- **W4 – Edit as text.** Every item can also be edited as text, next to
  the form, both in sync; and the whole configuration too (that's the
  backup file, which can then also be YAML). Some people prefer to write
  it down. **YAML in the browser** (more practical to write); the
  browser converts, the gateway only ever gets JSON (or something more
  compact). Validation errors from the gateway point at the line or
  field.
- **W5 – One entity per switch, not per button and click.** Today each
  button × click type is a binding of its own: with 30 switches that is
  hundreds of clicks and entities. Wanted: **one item per switch
  configuration** that covers all buttons and click types of a switch,
  and that **several switches can share** (switches doing the same, e.g.
  both bedroom switches, the hall switches). In the owner's words: for a
  switch, one button entity in which everything for that switch can be
  configured. The configuration carries the list of switches it applies
  to. Needs a model change (§6).
- **W6 – Light levels as levels.** Controls for a light work in levels:
  up/down and a direct choice of the level. Controls must not move while
  being clicked (today the +/− buttons shift as the label next to them
  changes width).
- **W7 – Groups that are easy to find and big enough**, e.g. "all lights
  in the house off" from one button. Groups exist (kind *Group*) but
  weren't found; at most 16 members today, too few for "all lights". A
  flat group with more members is enough; no groups of groups (§6).
- **W8 – Motion light: the level depends on the time of day.** Better
  guidance (W1), and the level it switches on to set per time of day:
  e.g. at night always level 1; in the daytime the level it had last,
  but at least level 4. The times are clock times or sun events, as in
  schedules. Needs a model change (§6).
- **W9 – Location help.** How to get latitude and longitude: e.g. "use
  this browser's location" (the page is HTTPS, so the Geolocation API is
  available if the certificate is trusted), or a hint where to look them
  up; and show the result as today's sun times right away.
- **W10 – Device features (outside the automation page, but needed by
  it).** On the device page, declare what a switch has on board or on
  its extension: VEML7700 (brightness), night light, BME680, DHT22,
  BME280, PIR, extension board (buttons 4–7), … The automation UI then
  offers only fitting devices: PIRs as motion detectors, brightness
  sensors in lux conditions, night lights as targets, buttons 4–7 only
  with an extension. Needs a model change (§6).

## 5. Gaps found in the first version

The redesign should solve these or argue why not.

- Bindings as a flat table: no view **per switch → button → event**.
- No **"where is this used"** for outputs, motion lights and schedules;
  renaming or deleting breaks references silently. Wanted: show the
  users, offer to update them on rename (the UI does the PUTs), refuse or
  warn on delete.
- **Dangling references** (a binding naming a deleted output) are not
  shown.
- Schedules show sun events but **not the resolved times** (today's
  sunset − 30 min = 17:12); no preview of the week.
- Motion lights don't show **why** they didn't switch (which condition
  failed, current lux).
- Light level table is a grid of numbers: no visual of the levels, no
  "try this level" button.
- Lamp mask as 24 toggles without knowing which lamp is which (W2, W3).
- Devices shown as `type_name / 0x..` when not named; device names
  should be used everywhere.
- Validation only on save (the gateway's `field`); inline hints before
  saving where the rules are known (§2).
- Learn mode only inside a binding's form; it should be reachable from
  "which switch is this?" too.
- Location and time live on another page (they stay there), but the
  automation page doesn't link to them where they matter.
- No sign when the gateway's clock is not set or the location is missing
  (schedules then do nothing).

## 6. Model and firmware changes the wishes need

These change the gateway, not only the UI. The design should assume them
and say what exactly it needs; they are built before or with the UI.

| Wish | Change | Notes |
|---|---|---|
| W5 | A **switch configuration** block: name, 1–n switch devices, and per button (0–7) and click type up to 4 actions. Replaces or sits on top of today's bindings | Fewer, bigger items: one item must still fit a request (~1.5 KB JSON), so a full 8-button table may need a compact form or one request per button. RAM: the trigger index stays per switch device |
| W3 | **Lamp names** per controller and bank: 24 names | Stored with the device (not the automation); up to ~24 × 16 B per controller |
| W7 | **Bigger flat groups**: up to all outputs (48) | Today 16 members. 48 names are ~1.1 KB of JSON, still one request; in RAM the members can be output indices (1 B each) |
| W10 | **Device features**: a set of flags per device | Stored on the gateway with the device name; later maybe reported by the node itself |
| W8 | **Motion light level by time of day**: a few periods (start as a clock time or sun event), each either a fixed level or "the last level, at least *n*" | Replaces today's single optional `level`; small (a few bytes per period) |

## 7. The API and its limits



All under `/api/v1`, JSON. See `api-rest.md` for the full list. Relevant
for the design:

- **One request at a time.** The gateway serves one TLS session at a
  time; parallel requests queue. A new TLS handshake takes noticeable
  time (key exchange in software on the ESP32); keep-alive connections are dropped after 1 s idle. So: few
  requests, sequential, no bursts of parallel fetches, no polling faster
  than ~2 s, and pages must show something useful while data arrives.
- **Small items.** A request body must fit ~1.5 KB (2048 B incl.
  headers); a response ≤ 3 KB. Hence one item per request; lists return
  summaries (`/automation/outputs` → name, kind, state), full items come
  from `GET …/:name`. Bindings are paged by 8 (`?offset=`).
- **No bulk or search endpoints.** "Where is X used" or "all bindings of
  switch Y" must be computed in the browser from the full lists.
- **Asynchronous effects.** Commands and enable/disable answer `202`; the
  state changes a moment later (light controllers: ≤ 1 s per change).
  The UI should reflect the intent at once and confirm from the next
  state read.
- **Changes reload the engine.** After a PUT/DELETE the gateway reloads
  its configuration (states are kept).
- **The API may grow**, if the design needs it and it fits the memory:
  e.g. condition state of a motion light, next switching times of a
  schedule, per-switch binding lookup, the loop guard state, lamp labels.
  Mark such needs explicitly; they cost firmware work and RAM.

## 8. Technical constraints

- **Stack**: Preact 10 + htm (tagged templates, no JSX, no build step
  besides esbuild bundling). No other runtime dependencies without a good
  reason; no CSS framework, no icon fonts, **no external resources**
  (CDNs, web fonts): the page must work without internet.
- **Size**: the whole UI is embedded in the firmware's flash,
  gzip-compressed (today `app.js.gz` 32.6 KB, `style.css.gz` 2.8 KB for
  all pages). **Flash is plentiful** (the app partition is ~40 % full):
  a library like a YAML parser is fine. **RAM is the scarce resource**:
  the gateway streams the files from flash, but anything the API sends
  or receives goes through small buffers (§7). Keep the bundle
  reasonable for load time over one TLS connection; no hard limit.
- **Routing**: History API (`src/router.js`). Every path the UI uses must
  also be listed in `gateway-hardware/src/web/assets.rs` (the gateway
  answers unknown paths with 404). Sub-paths like `/automation/:tab` are
  possible; deeper ones need a firmware change.
- **Shell**: the page lives in the existing shell and uses its CSS
  variables (`--bg`, `--card`, `--text`, `--muted`, `--line`, `--accent`,
  `--ok`, `--warn`, `--bad`, `--radius`); a dark mode exists via
  `prefers-color-scheme`. New components should fit the other pages, or
  the design should cover the shell too.
- **Language**: UI text in English (as all pages).
- **Browsers**: current Firefox, Chrome, Safari on the desktop. Narrow
  windows must not break, but there's no phone layout for configuring.
- **Accessibility**: keyboard usable, labels on all inputs, contrast AA,
  state not by color alone.

## 9. Out of scope

- The automation engine's behaviour beyond §6: §2 is what it does;
  other changes are a separate decision.
- Home Assistant discovery for outputs and automations (planned; the UI
  may show "visible in Home Assistant" later).
- Other pages, unless the design deliberately touches the shell, and
  except the device features on the device page (W10).

## 10. Decisions and open questions

Decided by the owner:

- No separate "use" view: the gateway UI isn't used for daily switching.
- Configuring on a phone: no. Desktop only.
- Rooms as a concept: no.
- Lamp names: yes (W3).
- Location and time: stay where they are, but need help to fill in (W9).
- Visual style: stays largely as the other pages; device features (W10)
  are new.

Answered later:

- W5: one entity per switch that configures everything for it (and
  shareable by several switches).
- W8: the level by time of day (night 1; daytime the last level, at
  least 4).
- W4: YAML in the browser, JSON to the gateway; flash is no problem,
  RAM is.
- W7: a flat group is enough.
- W5: the switches that share a configuration are a list on the
  configuration (not "same as switch X" on each switch).

Nothing is open.
