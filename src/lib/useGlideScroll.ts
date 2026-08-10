import { useEffect } from "react";

/**
 * Eased wheel scrolling, everywhere in the app.
 *
 * Windows delivers a wheel notch as one jump of roughly a hundred pixels and the
 * webview applies it in a single frame, so the content genuinely teleports a few
 * times a second. No CSS fixes this: `scroll-behavior` governs only
 * *programmatic* scrolls, never the wheel.
 *
 * So the notch is intercepted and turned into a target, and the element is eased
 * toward it a frame at a time. Successive notches add to that target rather than
 * restarting it, which is what makes a fast flick feel continuous instead of
 * stuttering as each notch cancels the last.
 *
 * Installed once, on the document, and it walks up from the event target to find
 * whatever is actually scrollable. Attaching per screen would mean remembering
 * to do it again for every screen added later, and forgetting once gives one
 * screen that scrolls differently from the rest.
 *
 * The frame loop exists only while there is distance left to cover — it starts
 * on a wheel event and stops on arrival. This app sits in the tray all day, so a
 * permanent animation frame would be real battery spent on nothing.
 */
export function useGlideScroll() {
  useEffect(() => {
    // Someone who asked for less motion has asked for exactly this: the native,
    // instant scroll.
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return;

    let element: HTMLElement | null = null;
    let target = 0;
    let frame = 0;

    const scrollableFrom = (start: EventTarget | null): HTMLElement | null => {
      let node = start instanceof HTMLElement ? start : null;
      while (node) {
        const style = getComputedStyle(node);
        const scrolls = /(auto|scroll)/.test(style.overflowY);
        if (scrolls && node.scrollHeight > node.clientHeight + 1) return node;
        node = node.parentElement;
      }
      return null;
    };

    const step = () => {
      if (!element) {
        frame = 0;
        return;
      }
      const current = element.scrollTop;
      const remaining = target - current;

      // Under a pixel there is nothing left to animate, and continuing would
      // spin a frame loop forever on a rounding error.
      if (Math.abs(remaining) < 0.5) {
        element.scrollTop = target;
        frame = 0;
        return;
      }

      // Exponential ease-out: a fixed fraction of what is left, each frame.
      // Quick while the gap is large, unnoticeably gentle as it lands, and it
      // cannot overshoot.
      element.scrollTop = current + remaining * 0.2;
      frame = requestAnimationFrame(step);
    };

    const onWheel = (event: WheelEvent) => {
      // A trackpad already sends smooth, small deltas. Intercepting those adds
      // lag to something that was already right, so only coarse wheel notches
      // are handled.
      if (event.deltaMode === 0 && Math.abs(event.deltaY) < 45) return;

      const found = scrollableFrom(event.target);
      if (!found) return;

      // A different container means the previous target is meaningless.
      if (found !== element) {
        element = found;
        target = found.scrollTop;
      }

      const max = element.scrollHeight - element.clientHeight;
      if (max <= 0) return;

      // At either end, let the event through so the gesture can pass on.
      if ((element.scrollTop <= 0 && event.deltaY < 0) || (element.scrollTop >= max && event.deltaY > 0)) {
        return;
      }

      event.preventDefault();

      // Lines and pages arrive in their own units.
      const delta =
        event.deltaMode === 1
          ? event.deltaY * 32
          : event.deltaMode === 2
            ? event.deltaY * element.clientHeight
            : event.deltaY;

      // Added to the target, not to the current position — that accumulation is
      // what turns several quick notches into one continuous glide.
      target = Math.max(0, Math.min(max, target + delta));
      if (!frame) frame = requestAnimationFrame(step);
    };

    // Anything that moves a container without the wheel — a jump to a section, a
    // scrollbar drag — becomes the new truth, or the next notch yanks the view
    // back to a stale target.
    const onScroll = (event: Event) => {
      if (!frame && event.target === element && element) target = element.scrollTop;
    };

    document.addEventListener("wheel", onWheel, { passive: false });
    document.addEventListener("scroll", onScroll, { capture: true, passive: true });
    return () => {
      document.removeEventListener("wheel", onWheel);
      document.removeEventListener("scroll", onScroll, { capture: true });
      if (frame) cancelAnimationFrame(frame);
    };
  }, []);
}
