// Option renderer: section files have populated SPECS; render by hash.
(function () {
  const mount = document.getElementById("mount");
  function render() {
    const id = (location.hash || "").replace(/^#/, "");
    const spec = window.SPECS[id];
    if (!spec) { mount.innerHTML = '<div class="lab-screen"><p class="ae-dim">Unknown option: ' + parts.esc(id || "(none)") + "</p></div>"; return; }
    // replace the node so entrance state resets cleanly
    const fresh = document.createElement("div");
    fresh.id = "mount";
    fresh.innerHTML = typeof spec.build === "function" ? spec.build() : "";
    mount.replaceWith(fresh);
    // demo links never navigate the hash
    fresh.querySelectorAll('a[href="#0"]').forEach((a) => a.addEventListener("click", (e) => e.preventDefault()));
    window.__labRound = 1; // round marker for cache verification
  }
  window.addEventListener("hashchange", () => { location.reload(); });
  render();
})();
