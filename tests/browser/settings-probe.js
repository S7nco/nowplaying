// Runs before settings.js: records what the page shows and what it saves
window.__puts = [];
const realFetch = window.fetch;
window.fetch = (url, options = {}) => {
    if (options.method === "PUT") __puts.push(String(url) + " " + options.body);
    return realFetch(url, options);
};
window.WebSocket = class { constructor() { setTimeout(() => this.onopen && this.onopen(), 0); } close() {} };
const probeErrors = [];
window.addEventListener("error", e => probeErrors.push(e.message));

(async () => {
    const sleep = ms => new Promise(r => setTimeout(r, ms));
    await sleep(1200);
    const val = id => document.getElementById(id)?.value;
    const r = {
        sliders: ["eq_floor_db", "eq_ceil_db", "eq_tilt_db"].map(id => val(id) + " " + document.getElementById(id).nextElementSibling.textContent).join(" | "),
        players: [...document.querySelectorAll(".player")].map(p => [...p.querySelectorAll("input")].map(i => i.value).join("=")).join(", "),
        bind: [...document.querySelectorAll("input[name=bind]")].map(i => (i.checked ? "[x]" : "[ ]") + (i.value || "custom")).join(" "),
        port: val("port"),
        hosts: [...document.querySelectorAll(".host input")].map(i => i.value).join(","),
        restartBox: !document.getElementById("restart-box").hidden,
        startup: document.getElementById("startup").checked,
        notice: document.getElementById("notice").hidden ? "" : document.getElementById("notice").textContent,
        detected: document.getElementById("detected").textContent.trim(),
    };

    // Move two sliders quickly: both must be saved (0.3.0 saved only the last one)
    const move = (id, v) => { const i = document.getElementById(id); i.value = v; i.dispatchEvent(new Event("input")); };
    move("eq_ceil_db", -20);
    await sleep(50);
    move("eq_floor_db", -90);
    await sleep(700);
    r.twoSlidersSaved = __puts.splice(0).join(" ; ");
    r.statusAfterSave = document.getElementById("eq-status").textContent;
    await sleep(2500);
    r.statusLater = document.getElementById("eq-status").textContent;
    r.errors = probeErrors.join("; ");
    await realFetch("/__result", { method: "POST", body: JSON.stringify(r) });
})();
