// Floating windows for the Dashboard and Flowcharts pages: drag a window by
// its heading, collapse it, and keep the layout for the next visit.
(() => {
  const windows = [...document.querySelectorAll('[data-window]')];
  if (!windows.length) return;
  const narrow = () => matchMedia('(max-width: 900px)').matches;
  const key = `ccm-windows-v2:${location.pathname}`;
  const content = document.querySelector('[data-content]');

  const save = () => {
    const state = {};
    for (const win of windows) {
      state[win.dataset.window] = { left: win.style.left, top: win.style.top, collapsed: win.classList.contains('collapsed') };
    }
    try {
      localStorage.setItem(key, JSON.stringify(state));
    } catch {
      // Storage may be unavailable; the layout then lasts for this visit.
    }
  };

  const place = (win, left, top) => {
    win.style.left = `${Math.max(80 - win.offsetWidth, Math.min(left, innerWidth - 80))}px`;
    win.style.top = `${Math.max(0, Math.min(top, innerHeight - 36))}px`;
    win.style.right = 'auto';
    win.style.bottom = 'auto';
  };

  const setCollapsed = (win, collapsed) => {
    win.classList.toggle('collapsed', collapsed);
    const button = win.querySelector(':scope .win-collapse');
    if (button) button.textContent = collapsed ? '+' : '–';
  };

  // The results window starts where an open window docked on the left ends.
  const fitContent = () => {
    if (!content || narrow()) return;
    let left = 16;
    for (const win of windows) {
      if (win.classList.contains('collapsed') || !win.offsetWidth) continue;
      const box = win.getBoundingClientRect();
      if (box.left < innerWidth * 0.4) left = Math.max(left, box.right + 16);
    }
    content.style.setProperty('--content-left', `${Math.round(Math.min(left, innerWidth * 0.6))}px`);
  };

  for (const win of windows) {
    const handle = win.dataset.handle ? win.querySelector(win.dataset.handle) : win;
    handle.classList.add('win-handle');
    if (win.dataset.handle && !win.hasAttribute('data-nocollapse')) {
      const button = Object.assign(document.createElement('button'), { type: 'button', className: 'win-collapse', textContent: '–' });
      button.setAttribute('aria-label', 'Collapse');
      button.addEventListener('click', () => {
        setCollapsed(win, !win.classList.contains('collapsed'));
        save();
        fitContent();
      });
      handle.append(button);
    }
    handle.addEventListener('mousedown', (event) => {
      if (event.button !== 0 || narrow() || event.target.closest('button, input, select, label, a, output')) return;
      event.preventDefault();
      const box = win.getBoundingClientRect();
      const grab = { x: event.clientX - box.left, y: event.clientY - box.top };
      const move = (e) => {
        place(win, e.clientX - grab.x, e.clientY - grab.y);
        fitContent();
      };
      const up = () => {
        removeEventListener('mousemove', move);
        removeEventListener('mouseup', up);
        save();
      };
      addEventListener('mousemove', move);
      addEventListener('mouseup', up);
    });
  }

  try {
    const state = JSON.parse(localStorage.getItem(key) || '{}');
    for (const win of windows) {
      const saved = state[win.dataset.window];
      if (!saved) continue;
      if (saved.left && saved.top) place(win, parseFloat(saved.left), parseFloat(saved.top));
      setCollapsed(win, Boolean(saved.collapsed));
    }
  } catch {
    // A missing or unreadable layout keeps the defaults.
  }

  const actions = document.querySelector('.top-actions');
  if (actions) {
    const reset = Object.assign(document.createElement('button'), { type: 'button', className: 'reset-layout', textContent: 'Reset layout' });
    reset.addEventListener('click', () => {
      for (const win of windows) {
        win.style.left = '';
        win.style.top = '';
        win.style.right = '';
        win.style.bottom = '';
        setCollapsed(win, false);
      }
      try {
        localStorage.removeItem(key);
      } catch {
        // Nothing saved to remove.
      }
      fitContent();
    });
    actions.prepend(reset);
  }

  fitContent();
  addEventListener('resize', fitContent);
  new ResizeObserver(fitContent).observe(windows[0]);
})();
