interface Box { left: number; top: number; width: number; height: number }

// Animate reflow after CSS has settled; transforms never delay layout or pointer input.
export function resizeGrid(node: HTMLElement, enabled: boolean) {
  const reduced = window.matchMedia('(prefers-reduced-motion: reduce)');
  const boxes = new Map<HTMLElement, Box>();
  const animations = new Map<HTMLElement, Animation>();
  const style = getComputedStyle(node);
  const timing = {
    duration: Number.parseFloat(style.getPropertyValue('--motion-enter')) || 180,
    easing: style.getPropertyValue('--motion-ease').trim() || 'ease-out',
  };
  let frame = 0;
  let destroyed = false;

  function cancelAnimations() {
    for (const animation of animations.values()) animation.cancel();
    animations.clear();
  }
  function measure() {
    frame = 0;
    if (destroyed) return;
    const items = Array.from(node.querySelectorAll<HTMLElement>('.article-card, .home-section-heading'));
    const previous = new Map<HTMLElement, Box>();
    // Keep the current visual pose when a new resize interrupts an unfinished one.
    for (const item of items) {
      const box = boxes.get(item);
      if (!box) continue;
      if (animations.has(item)) {
        const matrix = new DOMMatrixReadOnly(getComputedStyle(item).transform);
        previous.set(item, { left: box.left + matrix.e, top: box.top + matrix.f,
          width: box.width * matrix.a, height: box.height * matrix.d });
      } else previous.set(item, box);
    }
    cancelAnimations();
    boxes.clear();
    // Read all final rectangles before starting any compositor animations.
    for (const item of items) {
      const rect = item.getBoundingClientRect();
      boxes.set(item, { left: rect.left + window.scrollX, top: rect.top + window.scrollY,
        width: rect.width, height: rect.height });
    }
    if (!enabled || reduced.matches) return;
    for (const item of items) {
      const before = previous.get(item);
      const after = boxes.get(item)!;
      if (!before || !after.width || !after.height) continue;
      const x = before.left - after.left; const y = before.top - after.top;
      const scaleX = before.width / after.width; const scaleY = before.height / after.height;
      if (Math.abs(x) < .5 && Math.abs(y) < .5 && Math.abs(before.width - after.width) < .5
        && Math.abs(before.height - after.height) < .5) continue;
      const animation = item.animate([
        { transform: `translate(${x}px, ${y}px) scale(${scaleX}, ${scaleY})`, transformOrigin: '0 0' },
        { transform: 'none', transformOrigin: '0 0' },
      ], timing);
      animations.set(item, animation);
      animation.onfinish = () => { if (animations.get(item) === animation) animations.delete(item); };
    }
  }
  function schedule() { if (!destroyed && !frame) frame = requestAnimationFrame(measure); }
  function preferenceChanged() { if (reduced.matches) cancelAnimations(); schedule(); }
  const observer = new ResizeObserver(() => {
    cancelAnimationFrame(frame);
    // ResizeObserver runs before paint; invert the new layout in this same frame.
    measure();
  });
  observer.observe(node);
  reduced.addEventListener('change', preferenceChanged);
  schedule();

  return {
    update(value: boolean) { enabled = value; schedule(); },
    destroy() {
      destroyed = true; observer.disconnect(); reduced.removeEventListener('change', preferenceChanged);
      cancelAnimationFrame(frame); cancelAnimations(); boxes.clear();
    },
  };
}
