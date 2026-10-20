/* rustfolio — client-side search over the build-time JSON index.
 * Loaded only on the blog page; fetches /search.json lazily on first focus. */
(function () {
  "use strict";

  var input = document.querySelector("[data-search-input]");
  var status = document.querySelector("[data-search-status]");
  var results = document.querySelector("[data-search-results]");
  if (!input || !results || !status) return;

  var index = null;
  var loading = false;
  var active = -1;

  function setExpanded(expanded) {
    input.setAttribute("aria-expanded", expanded ? "true" : "false");
    results.hidden = !expanded;
  }

  function load() {
    if (index || loading) return Promise.resolve();
    loading = true;
    status.textContent = "Loading search index…";
    return fetch("/search.json")
      .then(function (response) {
        if (!response.ok) throw new Error("HTTP " + response.status);
        return response.json();
      })
      .then(function (data) {
        index = data;
        loading = false;
      })
      .catch(function () {
        loading = false;
        status.textContent = "Could not load the search index.";
      });
  }

  function score(entry, query) {
    var title = entry.title.toLowerCase();
    var description = entry.description.toLowerCase();
    var tags = entry.tags.join(" ").toLowerCase();
    var text = entry.text.toLowerCase();
    var score = 0;
    query.forEach(function (token) {
      if (title.indexOf(token) !== -1) score += 5;
      if (tags.indexOf(token) !== -1) score += 3;
      if (description.indexOf(token) !== -1) score += 2;
      if (text.indexOf(token) !== -1) score += 1;
    });
    return score;
  }

  function search(query) {
    if (!index) return [];
    var tokens = query.toLowerCase().split(/\s+/).filter(Boolean);
    if (!tokens.length) return [];
    return index
      .map(function (entry) { return { entry: entry, score: score(entry, tokens) }; })
      .filter(function (item) { return item.score > 0; })
      .sort(function (a, b) { return b.score - a.score; })
      .slice(0, 8)
      .map(function (item) { return item.entry; });
  }

  function render(hits, query) {
    results.innerHTML = "";
    active = -1;
    if (!query) {
      setExpanded(false);
      status.textContent = "";
      return;
    }
    if (!hits.length) {
      setExpanded(true);
      status.textContent = "No articles match “" + query + "”.";
      return;
    }
    status.textContent = hits.length + (hits.length === 1 ? " article" : " articles") + " found.";
    hits.forEach(function (entry) {
      var li = document.createElement("li");
      li.className = "search-result";
      li.setAttribute("role", "option");
      var link = document.createElement("a");
      link.className = "search-result-link";
      link.href = entry.url;
      link.textContent = entry.title;
      var desc = document.createElement("p");
      desc.className = "search-result-desc";
      desc.textContent = entry.description;
      li.appendChild(link);
      li.appendChild(desc);
      results.appendChild(li);
    });
    setExpanded(true);
  }

  function setActive(next) {
    var items = results.querySelectorAll(".search-result");
    if (!items.length) return;
    if (next < 0) next = items.length - 1;
    if (next >= items.length) next = 0;
    active = next;
    items.forEach(function (item, i) {
      if (i === active) {
        item.setAttribute("aria-selected", "true");
        input.setAttribute("aria-activedescendant", "search-active");
        item.id = "search-active";
      } else {
        item.setAttribute("aria-selected", "false");
      }
    });
    items[active].scrollIntoView({ block: "nearest" });
  }

  input.addEventListener("focus", function () { load(); });
  input.addEventListener("input", function () {
    var query = input.value.trim();
    if (query) { load().then(function () { render(search(query), query); }); }
    else { render([], ""); }
  });
  input.addEventListener("keydown", function (event) {
    var keys = ["ArrowDown", "ArrowUp", "Enter", "Escape"];
    if (keys.indexOf(event.key) === -1) return;
    var items = results.querySelectorAll(".search-result");
    if (event.key === "ArrowDown") {
      event.preventDefault();
      setActive(active + 1);
    } else if (event.key === "ArrowUp") {
      event.preventDefault();
      setActive(active - 1);
    } else if (event.key === "Enter" && active >= 0 && items[active]) {
      event.preventDefault();
      window.location.assign(items[active].querySelector("a").href);
    } else if (event.key === "Escape") {
      render([], "");
    }
  });
  document.addEventListener("click", function (event) {
    if (results.contains(event.target) || input.contains(event.target)) return;
    render([], "");
  });
})();
