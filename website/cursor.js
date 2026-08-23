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

  var FRAMES = [
    "shots/fly/cur-trump.png",
    "shots/fly/cur-elon.png",
    "shots/fly/cur-f1.png",
    "shots/fly/cur-wheel.png",
    "shots/fly/cur-trump2.png",
    "shots/fly/cur-gun.png",
  ];

  // Slow enough to read as a frame rather than a strobe, fast enough not to
  // look like a slideshow.
  var INTERVAL = 260;

  // A pointer that will not hold still is exactly what "reduce motion" is
  // asking us not to do. Leave frame one in place.
  var still = window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)");
  if (still && still.matches) return;

  // Decode every frame before the first swap, or the cursor blinks out for a
  // moment the first time each one is needed.
  FRAMES.forEach(function (src) {
    var img = new Image();
    img.src = src;
  });

  var i = 0;
  var timer = null;

  function tick() {
    i = (i + 1) % FRAMES.length;
    document.body.style.setProperty("--cursor", 'url("' + FRAMES[i] + '")');
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
