/* rustfolio — progressive enhancements (theme toggle, copy buttons,
 * reading progress, TOC scrollspy). Everything works without this file. */
(function () {
  "use strict";

  var root = document.documentElement;

  // ── Theme toggle ──
  var toggle = document.querySelector("[data-theme-toggle]");
  if (toggle) {
    var syncLabel = function () {
      toggle.setAttribute(
        "aria-label",
        root.dataset.theme === "dark" ? "Switch to light mode" : "Switch to dark mode"
      );
    };
    toggle.addEventListener("click", function () {
      var next = root.dataset.theme === "dark" ? "light" : "dark";
      if (next === "light") {
        delete root.dataset.theme;
      } else {
        root.dataset.theme = next;
      }
      try { localStorage.setItem("theme", next); } catch (e) { /* ignore */ }
      syncLabel();
    });
    syncLabel();
  }

  // ── Copy buttons on code blocks ──
  document.querySelectorAll(".code-block").forEach(function (block) {
    var code = block.querySelector("code");
    var header = block.querySelector(".code-header");
    if (!code || !header || !navigator.clipboard) return;
    var button = document.createElement("button");
    button.type = "button";
    button.className = "copy-button";
    button.textContent = "Copy";
    var reset = null;
    button.addEventListener("click", function () {
      navigator.clipboard.writeText(code.innerText).then(
        function () {
          button.textContent = "Copied";
          clearTimeout(reset);
          reset = setTimeout(function () { button.textContent = "Copy"; }, 1600);
        },
        function () { button.textContent = "Failed"; }
      );
    });
    header.appendChild(button);
  });

  // ── Reading progress indicator ──
  var bar = document.getElementById("progress-bar");
  var body = document.getElementById("article-body");
  if (bar && body) {
    var pending = false;
    var update = function () {
      pending = false;
      var rect = body.getBoundingClientRect();
      var total = rect.height - window.innerHeight;
      var read = Math.min(Math.max(-rect.top, 0), Math.max(total, 1));
      bar.style.width = (total > 0 ? (read / total) * 100 : 100) + "%";
    };
    window.addEventListener(
      "scroll",
      function () {
        if (!pending) {
          pending = true;
          window.requestAnimationFrame(update);
        }
      },
      { passive: true }
    );
    update();
  }

  // ── TOC scrollspy ──
  var toc = document.getElementById("toc");
  if (toc && "IntersectionObserver" in window) {
    var links = Array.prototype.slice.call(toc.querySelectorAll("a"));
    var byId = {};
    links.forEach(function (link) {
      var id = decodeURIComponent((link.getAttribute("href") || "").slice(1));
      var heading = id ? document.getElementById(id) : null;
      if (heading) byId[heading.id] = link;
    });
    var observer = new IntersectionObserver(
      function (entries) {
        entries.forEach(function (entry) {
          var link = byId[entry.target.id];
          if (!link) return;
          if (entry.isIntersecting) {
            links.forEach(function (l) { l.removeAttribute("aria-current"); });
            link.setAttribute("aria-current", "true");
          }
        });
      },
      { rootMargin: "-15% 0px -70% 0px" }
    );
    Object.keys(byId).forEach(function (id) {
      observer.observe(document.getElementById(id));
    });
  }
})();
