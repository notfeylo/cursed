(function () {
  "use strict";
  var openers = document.querySelectorAll("[data-dialog]");
  var dialogs = document.querySelectorAll("dialog");
  var activeOpener = null;

  function dialogName(dialog) {
    return dialog.id;
  }

  function updateModalState() {
    document.body.classList.toggle("modal-open", Boolean(document.querySelector("dialog[open]")));
  }

  function clearDialogHash(dialog) {
    if (window.location.hash !== "#" + dialogName(dialog)) return;
    window.history.replaceState(null, "", window.location.pathname + window.location.search);
  }

  function closeDialog(dialog, keepHash) {
    if (!dialog.open) return;
    dialog.close();
    if (!keepHash) clearDialogHash(dialog);
    updateModalState();
    if (activeOpener) activeOpener.focus();
    activeOpener = null;
  }

  function openDialog(name, opener) {
    var dialog = document.getElementById(name);
    if (!dialog || typeof dialog.showModal !== "function") return;

    dialogs.forEach(function (otherDialog) {
      if (otherDialog !== dialog && otherDialog.open) closeDialog(otherDialog, true);
    });

    activeOpener = opener || null;
    if (!dialog.open) dialog.showModal();
    updateModalState();
  }

  function syncDialogToHash() {
    var name = window.location.hash.slice(1);
    var target = name ? document.getElementById(name) : null;
    if (target && target.matches("dialog.legal-dialog")) {
      openDialog(name, null);
      return;
    }

    dialogs.forEach(function (dialog) {
      if (dialog.open) closeDialog(dialog, true);
    });
    updateModalState();
  }

  openers.forEach(function (opener) {
    opener.addEventListener("click", function (event) {
      event.preventDefault();
      var name = opener.dataset.dialog;
      if (window.location.hash !== "#" + name) {
        window.history.pushState(null, "", "#" + name);
      }
      openDialog(name, opener);
    });
  });

  dialogs.forEach(function (dialog) {
    dialog.querySelector("[data-close]").addEventListener("click", function () { closeDialog(dialog); });
    dialog.addEventListener("click", function (event) { if (event.target === dialog) closeDialog(dialog); });
    dialog.addEventListener("close", function () {
      clearDialogHash(dialog);
      updateModalState();
    });
  });

  window.addEventListener("hashchange", syncDialogToHash);
  window.addEventListener("popstate", syncDialogToHash);
  syncDialogToHash();
})();
