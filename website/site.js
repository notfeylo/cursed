(function () {
  "use strict";
  var openers = document.querySelectorAll("[data-dialog]");
  var dialogs = document.querySelectorAll("dialog");

  function closeDialog(dialog) {
    dialog.close();
    document.body.classList.remove("modal-open");
  }

  openers.forEach(function (button) {
    button.addEventListener("click", function () {
      var dialog = document.getElementById(button.dataset.dialog);
      if (!dialog || typeof dialog.showModal !== "function") return;
      dialog.showModal();
      document.body.classList.add("modal-open");
    });
  });

  dialogs.forEach(function (dialog) {
    dialog.querySelector("[data-close]").addEventListener("click", function () { closeDialog(dialog); });
    dialog.addEventListener("click", function (event) { if (event.target === dialog) closeDialog(dialog); });
    dialog.addEventListener("close", function () { document.body.classList.remove("modal-open"); });
  });
})();
