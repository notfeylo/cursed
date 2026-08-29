(function () {
  "use strict";

  var config = window.CURSED_DOWNLOAD_ANALYTICS;
  if (!config || !config.token || !config.endpoint || navigator.doNotTrack === "1" || navigator.globalPrivacyControl === true) return;

  function architectureFromLink(link) {
    var href = link.getAttribute("href") || "";
    if (/ARM64/i.test(href)) return "arm64";
    if (/x86/i.test(href)) return "x86";
    if (/Offline/i.test(href)) return "x64-offline";
    return "x64";
  }

  function isInstallerLink(link) {
    var href = link.getAttribute("href") || "";
    return href === "/download" || /Cursed-Setup(?:-[^/]+)?\.exe(?:$|\?)/i.test(href);
  }

  function sendDownloadEvent(link) {
    var payload = JSON.stringify({
      api_key: config.token,
      event: "installer download clicked",
      properties: {
        distinct_id: crypto.randomUUID ? crypto.randomUUID() : String(Date.now()) + "-" + Math.random(),
        version: config.version,
        architecture: architectureFromLink(link),
        page_path: window.location.pathname,
        $process_person_profile: false
      },
      timestamp: new Date().toISOString()
    });

    if (navigator.sendBeacon) {
      navigator.sendBeacon(config.endpoint, new Blob([payload], { type: "application/json" }));
      return;
    }

    fetch(config.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: payload,
      keepalive: true,
      credentials: "omit"
    }).catch(function () {});
  }

  document.addEventListener("click", function (event) {
    var link = event.target instanceof Element ? event.target.closest("a[href]") : null;
    if (link && isInstallerLink(link)) sendDownloadEvent(link);
  });
})();
