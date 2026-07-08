// Lab shell: builds the registry sidebar from window.SPECS (populated by the
// section files), drives the iframe by hash, and owns viewport controls.
(function () {
  const SECTION_ORDER = [
    ["SHELL", "SHELL — the chrome & places"],
    ["NOW", "NOW — the wall"],
    ["WIRE", "WIRE — the ambient feed"],
    ["REP", "REPORTS — generator + library"],
    ["DOC", "REPORT DOC — the document"],
    ["NEED", "NEEDS YOU — the inbox"],
    ["CLIP", "CLIPS — captured moments"],
    ["AGENT", "AGENT — the drill-down"],
  ];
  const ROUND = { n: 1, winners: {} }; // updated per operator verdicts

  const ids = Object.keys(window.SPECS || {});
  const byPrefix = (p) => ids.filter((i) => i.split("-")[0] === p)
    .sort((a, b) => Number(a.split("-")[1]) - Number(b.split("-")[1]));

  const registry = document.getElementById("registry");
  const frame = document.getElementById("frame");
  const frameWrap = document.getElementById("frameWrap");
  const optlabel = document.getElementById("optlabel");
  const stage = document.getElementById("stage");

  let flat = [];
  SECTION_ORDER.forEach(([prefix, label]) => {
    const secIds = byPrefix(prefix);
    if (!secIds.length) return;
    const h = document.createElement("h2");
    const win = ROUND.winners[prefix];
    h.innerHTML = `${label} <span class="round">· round ${ROUND.n}${win ? " · winner: " + win : ""}</span>`;
    registry.appendChild(h);
    secIds.forEach((id) => {
      const s = window.SPECS[id];
      const a = document.createElement("a");
      a.href = "#" + id;
      a.dataset.id = id;
      a.innerHTML = `<strong>${id}</strong> ${s.label || ""}<span class="thesis">${s.thesis || ""}</span>`;
      registry.appendChild(a);
      flat.push(id);
    });
  });

  function currentId() {
    const h = (location.hash || "").replace(/^#/, "");
    return window.SPECS[h] ? h : (localStorage.getItem("lab001.opt") || flat[0]);
  }
  function select(id, push) {
    if (!window.SPECS[id]) return;
    localStorage.setItem("lab001.opt", id);
    if (push) history.replaceState(null, "", "#" + id);
    frame.src = "frame.html?v=1#" + id;
    optlabel.textContent = id + " — " + (window.SPECS[id].label || "") + (window.SPECS[id].thesis ? " · " + window.SPECS[id].thesis : "");
    registry.querySelectorAll("a").forEach((a) => a.classList.toggle("on", a.dataset.id === id));
    const on = registry.querySelector("a.on");
    if (on) on.scrollIntoView({ block: "nearest" });
  }
  registry.addEventListener("click", (e) => {
    const a = e.target.closest("a[data-id]");
    if (!a) return;
    e.preventDefault();
    select(a.dataset.id, true);
  });
  window.addEventListener("keydown", (e) => {
    if (e.target.tagName === "INPUT") return;
    if (e.key !== "ArrowDown" && e.key !== "ArrowUp" && e.key !== "ArrowLeft" && e.key !== "ArrowRight") return;
    e.preventDefault();
    const dir = (e.key === "ArrowDown" || e.key === "ArrowRight") ? 1 : -1;
    const i = flat.indexOf(currentId());
    select(flat[(i + dir + flat.length) % flat.length], true);
  });

  // viewport control
  const vpread = document.getElementById("vpread");
  function applyVp(vp) {
    localStorage.setItem("lab001.vp", vp);
    document.querySelectorAll("#vpctl button[data-vp]").forEach((b) => b.classList.toggle("on", b.dataset.vp === vp));
    let w, h;
    if (vp === "fit") {
      frameWrap.style.transform = "";
      frameWrap.style.width = "100%";
      frameWrap.style.height = (stage.clientHeight - 16) + "px";
      vpread.textContent = "fit";
      return;
    }
    [w, h] = vp.split("x").map(Number);
    frameWrap.style.width = w + "px";
    frameWrap.style.height = h + "px";
    const availW = stage.clientWidth - 20, availH = stage.clientHeight - 20;
    const scale = Math.min(1, availW / w, availH / h);
    frameWrap.style.transform = scale < 1 ? `scale(${scale})` : "";
    vpread.textContent = `${w}×${h}` + (scale < 1 ? ` @ ${Math.round(scale * 100)}%` : "");
  }
  document.querySelectorAll("#vpctl button[data-vp]").forEach((b) =>
    b.addEventListener("click", () => applyVp(b.dataset.vp)));
  document.getElementById("applyCustom").addEventListener("click", () => {
    const w = Number(document.getElementById("cw").value), h = Number(document.getElementById("ch").value);
    if (w >= 240 && h >= 240) applyVp(`${w}x${h}`);
  });
  window.addEventListener("resize", () => applyVp(localStorage.getItem("lab001.vp") || "fit"));

  applyVp(localStorage.getItem("lab001.vp") || "fit");
  select(currentId(), true);
})();
