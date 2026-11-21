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
