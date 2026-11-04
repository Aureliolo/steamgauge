// Inlined into the page. The report is readable with scripting off: every panel is a real
// table row and every long review is fully present in the markup, so this only adds folding.
(() => {
  const root = document.documentElement;
  const Stored = 'steamgauge-theme';

  function applyStored() {
    try {
      const choice = localStorage.getItem(Stored);
      if (choice === 'dark' || choice === 'light') {
        root.setAttribute('data-theme', choice);
      }
    } catch {
      // A browser refusing storage is not a reason to render nothing.
    }
  }

  function dark() {
    const chosen = root.getAttribute('data-theme');
    if (chosen) {
      return chosen === 'dark';
    }
    return globalThis.matchMedia('(prefers-color-scheme: dark)').matches;
  }

  function toggleTheme(button) {
    const next = dark() ? 'light' : 'dark';
    root.setAttribute('data-theme', next);
    button.setAttribute('aria-pressed', String(next === 'dark'));
    try {
      localStorage.setItem(Stored, next);
    } catch {
      // The page still switches. It just will not remember.
    }
  }

  // Each category row is followed by its own panel row, so the two move together or the
  // evidence ends up under somebody else's number.
  function pairs(body) {
    const out = [];
    const rows = body.querySelectorAll('tr.row');
    Array.prototype.forEach.call(rows, (row) => {
      const next = row.nextElementSibling;
      out.push([row, next?.classList.contains('panel') ? next : null]);
    });
    return out;
  }

  function value(row, index) {
    const cell = row.children[index];
    if (!cell) {
      return 0;
    }
    const raw = cell.getAttribute('data-value');
    if (raw !== null) {
      return Number.parseFloat(raw);
    }
    // The name cell also holds a warning mark and a sentence meant only for a screen reader,
    // so the name is taken off the row, which carries it exactly.
    if (index === 0) {
      const name = row.getAttribute('data-name');
      if (name !== null) {
        return name;
      }
    }
    return cell.textContent.trim().toLowerCase();
  }

  function comparing(index, descending) {
    return (left, right) => {
      const a = value(left[0], index);
      const b = value(right[0], index);
      if (a === b) {
        return 0;
      }
      const order = a > b ? 1 : -1;
      return descending ? -order : order;
    };
  }

  function sortable(table) {
    const buttons = table.querySelectorAll('thead [data-sort]');
    Array.prototype.forEach.call(buttons, (button, index) => {
      button.addEventListener('click', () => {
        sortBy(table, button.parentNode, index);
      });
    });
  }

  function sortBy(table, header, index) {
    const [body] = table.tBodies;
    // A first click on a number sorts largest first, which is what anyone reading a
    // rate wants. A first click on a name sorts A to Z.
    const numeric = header.classList.contains('num');
    const current = header.getAttribute('aria-sort');
    const descending = current === 'none' ? numeric : current === 'ascending';

    const sorted = pairs(body).sort(comparing(index, descending));

    Array.prototype.forEach.call(table.querySelectorAll('thead th'), (th) => {
      th.setAttribute('aria-sort', 'none');
    });
    header.setAttribute('aria-sort', descending ? 'descending' : 'ascending');

    for (const [row, panel] of sorted) {
      body.appendChild(row);
      if (panel) {
        body.appendChild(panel);
      }
    }
  }

  // Every row that names a category, in the cross-game matrix and in each game's table, with
  // the evidence panel that has to move with it.
  function named() {
    const out = [];
    const tables = document.querySelectorAll('table.categories, table.matrix');
    Array.prototype.forEach.call(tables, (table) => {
      const [body] = table.tBodies;
      if (!body) {
        return;
      }
      Array.prototype.forEach.call(body.rows, (row) => {
        // The rendered cell carries a warning mark and text meant only for a screen
        // reader, so the name is read off the row rather than out of it.
        const name = row.getAttribute('data-name');
        if (name === null) {
          return;
        }
        const next = row.nextElementSibling;
        out.push({
          name,
          panel: next?.classList.contains('panel') ? next : null,
          row,
        });
      });
    });
    return out;
  }

  function narrow(form) {
    const input = form.querySelector('[data-filter-input]');
    const count = form.querySelector('[data-filter-count]');
    const rows = named();
    const total = {};
    for (const entry of rows) {
      total[entry.name] = true;
    }
    const all = Object.keys(total).length;

    function filterRows(query) {
      const matched = {};
      for (const entry of rows) {
        const hit = query === '' || entry.name.indexOf(query) !== -1;
        // A class rather than the hidden attribute, which already means "folded away" on
        // these rows. Printing opens everything folded and must not undo a filter with it.
        entry.row.classList.toggle('filtered-out', !hit);
        if (entry.panel) {
          entry.panel.classList.toggle('filtered-out', !hit);
        }
        if (hit) {
          matched[entry.name] = true;
        }
      }
      return Object.keys(matched).length;
    }

    function apply() {
      const query = input.value.trim().toLowerCase();
      const shown = filterRows(query);
      if (query === '') {
        count.textContent = count.getAttribute('data-showing-everything') || '';
      } else if (shown === 0) {
        count.textContent = 'no subject matches';
      } else {
        count.textContent = `${shown} of ${all}`;
      }
    }

    input.addEventListener('input', apply);
    // A search input clears itself, and browsers disagree about which event says so.
    input.addEventListener('search', apply);
    form.addEventListener('submit', (event) => {
      event.preventDefault();
    });
    form.hidden = false;
  }

  // Nobody can open a fold on paper. The stylesheet handles the rows and the clipped text,
  // but a closed <details> is hidden by the browser itself and no author rule reaches it.
  function unfoldToPrint() {
    let reopened = [];
    globalThis.addEventListener('beforeprint', () => {
      reopened = [];
      Array.prototype.forEach.call(document.querySelectorAll('details'), (fold) => {
        if (!fold.open) {
          fold.open = true;
          reopened.push(fold);
        }
      });
    });
    globalThis.addEventListener('afterprint', () => {
      for (const fold of reopened) {
        fold.open = false;
      }
      reopened = [];
    });
  }

  // A link to one category's evidence has to arrive with that evidence open, or it lands on
  // a row that looks exactly like every other row.
  function openWhatTheAddressAsksFor() {
    const id = globalThis.location.hash.replace('#', '');
    if (!id) {
      return;
    }
    const panel = document.querySelector(`#${CSS.escape(id)}`);
    if (!panel?.classList.contains('panel')) {
      return;
    }
    // A narrowed page has this row off screen entirely, and scrolling to something with no
    // box is the same as doing nothing. Being asked for a category is a good enough reason
    // to stop hiding it.
    if (panel.classList.contains('filtered-out')) {
      const input = document.querySelector('[data-filter-input]');
      if (input) {
        input.value = '';
        input.dispatchEvent(new Event('input'));
      }
    }

    const row = panel.previousElementSibling;
    const button = row?.querySelector('button.disclose');
    if (button && row.getAttribute('data-expands') === id) {
      button.setAttribute('aria-expanded', 'true');
    }
    panel.hidden = false;
    panel.scrollIntoView();
  }

  function setUp() {
    // Tells the stylesheet that folding is available. Everything folded is fully present in
    // the markup, so a page without scripting is longer rather than incomplete.
    root.classList.add('js');
    applyStored();

    const toggle = document.querySelector('[data-theme-toggle]');
    if (toggle) {
      toggle.setAttribute('aria-pressed', String(dark()));
      toggle.addEventListener('click', () => {
        toggleTheme(toggle);
      });
    }

    // Panels start closed only once scripting is known to work. Without this a reader with
    // no scripting would face rows that can never be opened.
    const rows = document.querySelectorAll('[data-expands]');
    Array.prototype.forEach.call(rows, (row) => {
      const id = row.getAttribute('data-expands');
      const panel = document.querySelector(`#${CSS.escape(id)}`);
      const [head] = row.cells;
      if (!(panel && head)) {
        return;
      }
      panel.hidden = true;

      // A real button inside the row header rather than a role on the row itself: the row
      // has to stay a row, or its cells stop being cells for anyone listening to the page.
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'disclose';
      button.setAttribute('aria-controls', id);
      button.setAttribute('aria-expanded', 'false');
      while (head.firstChild) {
        button.appendChild(head.firstChild);
      }
      head.appendChild(button);

      // Listened for on the row, so the whole row stays a target and the button's own
      // keyboard activation arrives here as one click rather than two.
      row.addEventListener('click', () => {
        // Reading a table of rates means selecting numbers out of it, and a drag that ends
        // inside a row is somebody copying a figure rather than asking to fold it away.
        const selection = globalThis.getSelection();
        if (selection && String(selection).length > 0) {
          return;
        }
        const open = button.getAttribute('aria-expanded') === 'true';
        button.setAttribute('aria-expanded', String(!open));
        panel.hidden = open;
      });
    });

    Array.prototype.forEach.call(
      document.querySelectorAll('table.categories, table.corpora'),
      sortable,
    );

    const mores = document.querySelectorAll('[data-expands-text]');
    Array.prototype.forEach.call(mores, (button) => {
      const text = button.previousElementSibling;
      if (!text) {
        return;
      }
      button.addEventListener('click', () => {
        const open = text.classList.toggle('open');
        button.setAttribute('aria-expanded', String(open));
        button.textContent = open ? 'Show less' : 'Show the rest';
      });
    });

    // After the panels, which decide for themselves whether they start open.
    const form = document.querySelector('[data-filter]');
    if (form?.querySelector('[data-filter-input]')) {
      narrow(form);
    }

    unfoldToPrint();
    openWhatTheAddressAsksFor();
    globalThis.addEventListener('hashchange', openWhatTheAddressAsksFor);
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', setUp);
  } else {
    setUp();
  }
})();
