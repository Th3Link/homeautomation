(() => {
  const collator = new Intl.Collator("en", { numeric: true, sensitivity: "base" });

  for (const table of document.querySelectorAll("table[data-sortable]")) {
    const body = table.tBodies[0];
    const rows = Array.from(body.rows);
    let activeHeader;
    let activeMark;

    for (const [column, header] of Array.from(table.tHead.rows[0].cells).entries()) {
      const numeric = header.classList.contains("number");
      const key = (row) => {
        const cell = row.cells[column];
        const value = cell.dataset.sort ?? cell.textContent.trim();
        if (value === "" || value === "—") return null;
        return numeric ? Number(value) : value;
      };

      const button = document.createElement("button");
      button.type = "button";
      button.textContent = header.textContent;
      const mark = document.createElement("span");
      mark.className = "sort-mark";
      mark.setAttribute("aria-hidden", "true");
      mark.textContent = "↕";
      button.append(mark);
      header.replaceChildren(button);

      button.addEventListener("click", () => {
        const descending = header.getAttribute("aria-sort") === "ascending";
        if (activeHeader) {
          activeHeader.removeAttribute("aria-sort");
          activeMark.textContent = "↕";
        }
        header.setAttribute("aria-sort", descending ? "descending" : "ascending");
        mark.textContent = descending ? "↓" : "↑";
        activeHeader = header;
        activeMark = mark;

        const sorted = rows.map((row) => ({ row, value: key(row) }));
        sorted.sort((a, b) => {
          if (a.value === null) return b.value === null ? 0 : 1;
          if (b.value === null) return -1;
          const order = numeric ? a.value - b.value : collator.compare(a.value, b.value);
          return descending ? -order : order;
        });
        body.append(...sorted.map(({ row }) => row));
      });
    }
  }
})();

for (const panel of document.querySelectorAll(".placement[data-banks]")) {
  const banks = JSON.parse(panel.dataset.banks);
  const stack = banks.SRAM2.at(-1);
  const budget = stack.spans[0].size;
  const select = panel.querySelector("select");
  const totals = panel.querySelector(".placement-totals");
  const bars = panel.querySelector(".placement-bars");
  const rows = panel.querySelector("tbody");
  const bytes = value => value.toLocaleString("en-US") + " B";
  const hex = addr => "0x" + addr.toString(16).toUpperCase();
  const element = (tag, text, className) => {
    const node = document.createElement(tag);
    if (text !== undefined) node.textContent = text;
    if (className) node.className = className;
    return node;
  };
  const names = { ".rom_boot_pro_uninit": "PRO boot window", ".rom_boot_app_uninit": "APP boot window",
    ".dram2_uninit": "Upper SRAM1" };
  const node = panel.dataset.firmware === "cancomponents";
  const colors = { rom: "ROM", linked: "Linked data", object: "Named static",
    gap: "Unallocated", stack: "Main stack", moved: "Previewed static" };
  if (node) colors.reserved = "Reserved boot area";
  const windows = banks.SRAM1.filter(region => region.spans.at(-1).kind === "gap");
  const gap = region => region.spans.at(-1).size;
  const largest = Math.max(...windows.map(gap));
  const free = windows.reduce((sum, region) => sum + gap(region), 0);
  const candidates = windows.flatMap(region => region.spans.filter(span => span.kind === "object"));
  const upper = windows.find(region => region.label === ".dram2_uninit");
  const staticBytes = banks.SRAM2.flatMap(region => region.spans)
    .filter(span => span.kind === "linked" || span.kind === "object")
    .reduce((sum, span) => sum + span.size, 0);
  const bss = banks.SRAM2.find(region => region.label === ".bss");
  const toUpper = node && bss ? bss.spans.filter(span => span.kind === "object") : [];
  const shortName = span => span.label.replace(/^(gateway_hardware|cancomponents)::/, "");
  select.add(new Option("As linked", ""));
  // Look for one swap which improves the largest usable linker window without
  // changing the stack budget. Compare byte counts before linker alignment.
  let swap;
  let best = largest;
  for (const [i, left] of windows.entries()) {
    for (const right of windows.slice(i + 1)) {
      for (const a of left.spans.filter(span => span.kind === "object")) {
        for (const b of right.spans.filter(span => span.kind === "object")) {
          const delta = a.size - b.size;
          const gaps = [gap(left) + delta, gap(right) - delta];
          if (Math.min(...gaps) >= 0 && Math.max(...gaps) > best) {
            best = Math.max(...gaps);
            swap = { left, right, a, b };
          }
        }
      }
    }
  }
  if (swap) select.add(new Option("Swap two SRAM1 statics · preserve stack budget", "swap"));
  function addChoices(label, objects, prefix) {
    if (!objects.length) return;
    const group = element("optgroup");
    group.label = label;
    objects.forEach((span, i) => group.append(new Option(shortName(span) + " · " + bytes(span.size), prefix + i)));
    select.append(group);
  }
  addChoices("Move one static to SRAM2", candidates, "to-stack-");
  addChoices("Move one SRAM2 static to upper SRAM1", toUpper, "to-upper-");
  select.disabled = !candidates.length && !toUpper.length;
  for (const [kind, label] of Object.entries(colors)) {
    const key = element("span", label);
    key.prepend(element("i", undefined, "segment " + kind));
    panel.querySelector(".placement-key").append(key);
  }

  function metric(label, current, linked) {
    const card = element("div");
    card.append(element("span", label), element("strong", bytes(current)));
    if (current !== linked) {
      const delta = current - linked;
      card.append(element("small", (delta > 0 ? "+" : "−") + bytes(Math.abs(delta)) + " vs linked"));
    }
    totals.append(card);
  }

  function render() {
    const swapping = select.value === "swap";
    const toSRAM1 = select.value.startsWith("to-upper-");
    const index = Number(select.value.split("-").at(-1));
    const selected = select.value && !swapping ? (toSRAM1 ? toUpper : candidates)[index] : null;
    const movedSize = selected?.size ?? 0;
    const available = toSRAM1 ? gap(upper) : budget;
    const fits = movedSize <= available;
    let description = "";
    if (select.disabled) description = "Individual placement candidates are unavailable for this build.";
    if (swapping) description = "Preview · swap " + shortName(swap.a) + " and " + shortName(swap.b) +
      " between " + names[swap.left.label] + " and " + names[swap.right.label] + ".";
    if (selected) description = fits ? "Estimated addresses after repacking." :
      "This object exceeds the " + (toSRAM1 ? "upper SRAM1 gap" : "stack budget") + " by " +
      bytes(movedSize - available) + ". Showing the linked layout.";
    const note = panel.querySelector(".placement-note");
    note.textContent = description;
    note.hidden = !description;
    const affectedWindow = selected && fits && (toSRAM1 ? upper : windows.find(r => r.spans.includes(selected)));
    const gapDelta = affectedWindow ? (toSRAM1 ? -movedSize : movedSize) : 0;
    const largestAfter = Math.max(...windows.map(r => gap(r) + (r === affectedWindow ? gapDelta : 0)));
    totals.replaceChildren();
    metric("Largest SRAM1 linker gap", swapping ? best : largestAfter, largest);
    if (node) metric("SRAM2 statics", staticBytes + gapDelta, staticBytes);
    else metric("Total SRAM1 gaps", free + gapDelta, free);
    metric("SRAM2 main stack budget", budget - gapDelta, budget);

    bars.replaceChildren();
    rows.replaceChildren();
    for (const [bank, regions] of Object.entries(banks)) {
      const title = element("h3", bank);
      const first = regions[0].start;
      const last = regions.at(-1);
      const end = last.start + last.spans.reduce((sum, span) => sum + span.size, 0);
      title.append(element("small", (end - first) / 1024 + " KiB"));
      const strip = element("div", undefined, "placement-strip");
      strip.setAttribute("aria-hidden", "true");
      const labels = element("div", undefined, "placement-windows");
      let addr = first;
      for (const region of regions) {
        let spans = region.spans;
        if (swapping && (region === swap.left || region === swap.right)) {
          const [outgoing, incoming] = region === swap.left ? [swap.a, swap.b] : [swap.b, swap.a];
          spans = spans.map(span => {
            if (span === outgoing) return { ...incoming, kind: "moved" };
            if (span.kind === "gap") return { ...span, size: span.size + outgoing.size - incoming.size };
            return span;
          });
        } else if (region === affectedWindow) {
          spans = spans.filter(span => span !== selected && span.kind !== "gap");
          if (toSRAM1) spans.push({ ...selected, kind: "moved" });
          spans.push({ ...region.spans.at(-1), size: gap(region) + gapDelta });
        } else if (affectedWindow && toSRAM1 && region === bss) {
          spans = spans.filter(span => span !== selected);
        } else if (affectedWindow && region === stack) {
          spans = [{ ...stack.spans[0], size: budget - gapDelta }];
          if (!toSRAM1) spans.unshift({ ...selected, kind: "moved" });
        }
        for (const span of spans) {
          if (span.kind === "gap" && bank === "SRAM1") {
            labels.append(element("span", names[region.label] + " · " + bytes(span.size) + " free"));
          }
          if (!span.size) continue;
          const part = element("i", undefined, "segment " + span.kind);
          part.style.flexGrow = String(span.size);
          part.title = span.label + " · " + hex(addr) + " · " + bytes(span.size);
          if (span.size >= (end - first) * 0.07) {
            const symbol = span.label.split("::");
            const label = symbol.at(-1) === "POOL" ? symbol.slice(-2).join("::") : symbol.at(-1);
            part.textContent = span.kind === "stack" ? "Main stack" : label;
          }
          strip.append(part);
          const row = element("tr");
          for (const value of [bank + " / " + (span.kind === "moved" && region === stack ? "projected static" : region.label),
            hex(addr), span.size.toLocaleString("en-US"), span.label]) row.append(element("td", value));
          rows.append(row);
          addr += span.size;
        }
      }
      const axis = element("div", undefined, "placement-axis");
      axis.append(element("code", hex(first)), element("code", hex(end - 1)));
      bars.append(title, strip, axis, labels);
    }
  }
  select.addEventListener("change", render);
  render();
  panel.querySelector(".placement-ui").hidden = false;
}
