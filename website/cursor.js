/**
 * The page's animated cursor.
 *
 * An animated cursor is a list of images shown one after another — that is all
 * an `.ani` file is — and CSS cannot do it: `cursor` is not an animatable
 * property, and browsers do not play an animated GIF or an `.ani` used as one.
 * So the frames are swapped here instead.
 *
 * What this deliberately is *not*: a `<div>` chasing the mouse. That is the one
 * thing the app itself refuses to do, because a sprite in an overlay is
 * composited by the desktop and trails the real pointer permanently. Setting
 * `cursor` hands the image to the OS, which draws it on the hardware cursor
 * plane with no lag — the same argument the about section makes, applied to the
 * page making it.
 *
 * This file is the only script on the site, which is why the CSP is
 * `script-src 'self'` rather than `'none'`. If it never runs, `styles.css` has
 * already set frame one and the page keeps a working pointer.
 */
(function () {
  "use strict";

  // Each frame twice: a 32 px copy and a 64 px one. A cursor image is sized in
  // CSS pixels, so on a 150% or 200% display the 32 px file is enlarged by the
  // browser and the pointer goes soft — which is most laptops.
  var FRAMES = ["trump", "elon", "f1", "wheel", "trump2", "gun"].map(function (name) {
    return {
      one: 'url("shots/fly/cur-' + name + '.png")',
      two: 'url("shots/fly/cur-' + name + '@2x.png")',
    };
  });

  // Slow enough to read as a frame rather than a strobe, fast enough not to
  // look like a slideshow.
  var INTERVAL = 260;

  // A pointer that will not hold still is exactly what "reduce motion" is
  // asking us not to do. Leave frame one in place.
  var still = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)");
  if (still && still.matches) return;

  // Decode every frame before the first swap, or the cursor blinks out for a
  // moment the first time each one is needed.
  FRAMES.forEach(function (frame) {
    [frame.one, frame.two].forEach(function (css) {
      var img = new Image();
      img.src = css.slice(5, -2); // strip url(" and ")
    });
  });

  var i = 0;
  var timer = null;

  function tick() {
    i = (i + 1) % FRAMES.length;
    // Only the two custom properties move. The `cursor` declarations that read
    // them, `image-set()` included, stay in the stylesheet where the fallback
    // ordering is already correct.
    document.body.style.setProperty("--cursor", FRAMES[i].one);
    document.body.style.setProperty("--cursor-2x", FRAMES[i].two);
  }

  function start() {
    if (timer === null) timer = window.setInterval(tick, INTERVAL);
  }

  function stop() {
    if (timer !== null) {
      window.clearInterval(timer);
      timer = null;
    }
  }

  // Nothing to animate on a tab nobody is looking at.
  document.addEventListener("visibilitychange", function () {
    if (document.hidden) stop();
    else start();
  });

  start();
})();
