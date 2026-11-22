// Settings page (settings.html): reads and changes config.toml

const EQ = ["eq_floor_db", "eq_ceil_db", "eq_tilt_db"];
const notice = document.getElementById("notice");
const eqStatus = document.getElementById("eq-status");
const playersEl = document.getElementById("players");
const playersStatus = document.getElementById("players-status");
const detectedEl = document.getElementById("detected");
const netStatus = document.getElementById("network-status");
const hostsEl = document.getElementById("hosts");
const portBox = document.getElementById("port");
const restartBox = document.getElementById("restart-box");
let defaults = {};

// Makes an element: el("span", { className: "badge" }, "Playing")
function el(tag, props = {}, ...children) {
    const node = Object.assign(document.createElement(tag), props);
    node.append(...children);
    return node;
}

// ==============================
// Reading the settings
// ==============================

async function getState() {
    const res = await fetch("/settings", { cache: "no-store" });
    return res.json();
}

async function load() {
    const state = await getState();
    defaults = state.defaults || {};
    EQ.forEach(name => setSlider(name, state.settings[name]));
    renderPlayers(state.settings);
    setNetworkInputs(state.settings);
    hostsEl.replaceChildren();
    state.settings.allowed_hosts.forEach(name => addHostRow(name));
    showState(state);
}

// Notices at the top: a broken config.toml, unknown setting names, a Restart waiting
function showState(state) {
    const notes = [];
    if (state.error) notes.push("config.toml has an error, so changes can't be saved until it's fixed:\n" + state.error);
    if (state.unknown && state.unknown.length) notes.push("config.toml has settings the app doesn't know (check their spelling): " + state.unknown.join(", "));
    notice.textContent = notes.join("\n\n");
    notice.hidden = notes.length === 0;
    showNetworkState(state);
}

// ==============================
// Saving (shared by every section)
// ==============================

function setStatus(target, text, bad = false) {
    target.textContent = text;
    target.classList.toggle("bad", bad);
}

async function save(changes, statusEl) {
    setStatus(statusEl, "Saving...");
    let res, text;
    try {
        res = await fetch("/settings", {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(changes),
        });
        text = await res.text();
    } catch {
        setStatus(statusEl, "Couldn't reach Now Playing. Is it running?", true);
        return null;
    }
    let reply;
    try { reply = JSON.parse(text); } catch {
        // not JSON: a refusal from the guard, e.g. "Changes can only be made on the PC running Now Playing"
        setStatus(statusEl, text, true);
        return null;
    }
    if (!res.ok) {
        setStatus(statusEl, Object.values(reply.errors || {}).join(" "), true);
        return null;
    }
    showState(reply);
    setStatus(statusEl, "Saved");
    return reply;
}

// ==============================
// Equalizer
// ==============================

function setSlider(name, value) {
    const input = document.getElementById(name);
    input.value = value;
    showValue(input);
}

function showValue(input) {
    const decimals = Number(input.step) < 1 ? 1 : 0;
    input.nextElementSibling.textContent = Number(input.value).toFixed(decimals) + " dB";
}

// Not saved: put the sliders back to what's in config.toml
async function saveEq(changes) {
    if (!(await save(changes, eqStatus))) {
        const state = await getState();
        EQ.forEach(name => setSlider(name, state.settings[name]));
    }
}

// Save once the slider has rested for a moment, not on every pixel of a drag
let eqTimer;
EQ.forEach(name => {
    const input = document.getElementById(name);
    input.addEventListener("input", () => {
        showValue(input);
        clearTimeout(eqTimer);
        eqTimer = setTimeout(() => saveEq({ [name]: Number(input.value) }), 300);
    });
});

document.getElementById("eq-reset").addEventListener("click", () => {
    const changes = {};
    EQ.forEach(name => {
        changes[name] = defaults[name];
        setSlider(name, defaults[name]);
    });
    saveEq(changes);
});

// ==============================
// Live bars, from the same feed the overlay uses
// ==============================

const bars = document.getElementById("bars");
for (let i = 0; i < 8; i++) bars.appendChild(el("span"));

function showLevels(levels) {
    levels.forEach((level, i) => {
        if (bars.children[i]) bars.children[i].style.height = (level * 100) + "%";
    });
}

let barsSocket;
let barsOpen = false;
let restarting = false; // during a restart, don't retry (each failed try makes firefox wait longer)

function connectBars() {
    barsSocket = new WebSocket(`ws://${location.host}/ws?spectrum=1`);
    barsSocket.onopen = () => { barsOpen = true; };
    barsSocket.onmessage = event => {
        const msg = JSON.parse(event.data);
        if (msg.type === "spectrum") showLevels(msg.levels);
    };
    barsSocket.onclose = () => { 
        barsOpen = false;
        if (!restarting) setTimeout(connectBars, 2000); 
    };
}

// Until the WebSocket is open, ask for the levels directly. Firefox can hold a WebSocket back
// for up to a minute after a restart; ordinary requests aren't held back
setInterval(async () => {
    if (barsOpen || restarting) return;
    try {
        const res = await fetch("/spectrum", { cache: "no-store" });
        showLevels((await res.json()).levels);
    } catch { /* not running right now */ }
}, 150);

// ==============================
// Players: one row per player (media ID + process name)
// ==============================

// "a, b" -> ["a", "b"], keeping the original spelling
function listOf(text) {
    return (text || "").split(",").map(s => s.trim()).filter(Boolean);
}

function renderPlayers(settings) {
    const ids = listOf(settings.app_id_match);
    const processes = listOf(settings.process_name);
    playersEl.replaceChildren();
    ids.forEach((id, i) => addPlayerRow(id, processes[i] || ""));
}

function addPlayerRow(id = "", process = "") {
    const idBox = el("input", { value: id, placeholder: "e.g. spotify" });
    const processBox = el("input", { value: process, placeholder: "e.g. Spotify.exe" });
    const remove = el("button", { className: "close", title: "Remove this player" }, "×");
    const row = el("div", { className: "player" }, idBox, processBox, remove);
    idBox.addEventListener("input", queuePlayersSave);
    processBox.addEventListener("input", queuePlayersSave);
    remove.addEventListener("click", () => { row.remove(); queuePlayersSave(); });
    playersEl.appendChild(row);
    return row;
}

document.getElementById("player-add").addEventListener("click", () => {
    addPlayerRow().querySelector("input").focus();
});

// Save once typing has paused
let playersTimer;
function queuePlayersSave() {
    clearTimeout(playersTimer);
    playersTimer = setTimeout(savePlayers, 600);
}

async function savePlayers() {
    const rows = [...playersEl.querySelectorAll(".player")];
    if (rows.length === 0) {
        setStatus(playersStatus, "Keep at least one player.", true);
        return; 
    }
    let complete = true;
    for (const box of playersEl.querySelectorAll("input")) {
        const missing = box.value.trim() === "";
        box.classList.toggle("missing", missing);
        if (missing) complete = false;
    }
    if (!complete) {
        setStatus(playersStatus, "Fill in both boxes for each player.", true);
        return; 
    }
    const ids = rows.map(row => row.children[0].value.trim());
    const processes = rows.map(row => row.children[1].value.trim());
    if ([...ids, ...processes].some(name => name.includes(","))) {
        setStatus(playersStatus, "Names can't contain commas.", true);
        return;
    }
    if (await save({ app_id_match: ids.join(", "), process_name: processes.join(", ") }, playersStatus)) {
        loadDetected(); // the "In your list" marks may have changed
    }
}

// ==============================
// Detected right now
// ==============================

let lastDetected = "";
async function loadDetected() {
    let sessions;
    try {
        sessions = await (await fetch("/players/detected", { cache: "no-store" })).json();
    } catch {
        return;
    }
    // Unchanged: leave the list alone, so a click on Add isn't lost to a rebuild
    const key = JSON.stringify(sessions);
    if (key === lastDetected) return;
    lastDetected = key;
    if (sessions.length === 0) {
        detectedEl.replaceChildren(el("li", { className: "empty" }, "No media apps right now. Start playing something."));
        return;
    }
    detectedEl.replaceChildren(...sessions.map(session => {
        const li = el("li", {},
            el("code", {}, session.id),
            el("span", { className: "badge" + (session.playing ? " on" : "") }, session.playing ? "Playing" : "Paused"),
        );
        if (session.known) {
            li.append(el("span", { className: "badge on" }, "✓ In your list"));
        } else {
            const add = el("button", { className: "chip" }, "Add");
            add.addEventListener("click", () => addDetected(session.id));
            li.append(add);
        }
        return li;
    }));
}

// Apps that report their exe name (like Spotify.exe) fill in both boxes; others need the process typed in
function addDetected(id) {
    const process = /\.exe$/i.test(id) ? id : "";
    const row = addPlayerRow(id.toLowerCase(), process);
    if (process) {
        queuePlayersSave();
    } else {
        row.children[1].focus();
        setStatus(playersStatus, "Now type its process name (Task Manager > Details).");
    }
}

// ==============================
// Network: who can connect, port, other names, Restart
// ==============================

const wait = ms => new Promise(resolve => setTimeout(resolve, ms));

// The choice and the port box, from config.toml's values
function setNetworkInputs(settings) {
    const bind = settings.bind_address.trim();
    const thisPC = ["127.0.0.1", "localhost", "::1"].includes(bind);
    const everyone = ["0.0.0.0", "::"].includes(bind);
    document.querySelector('input[name=bind][value="127.0.0.1"]').checked = thisPC;
    document.querySelector('input[name=bind][value="0.0.0.0"]').checked = everyone;
    // anything else (e.g. one specific address) was typed into config.toml: show it as it is
    const custom = !thisPC && !everyone;
    document.getElementById("bind-custom-row").hidden = !custom;
    document.getElementById("bind-custom").checked = custom;
    document.getElementById("bind-custom-value").textContent = bind;
    portBox.value = settings.port;
}

// After every load or save: the Restart box, and the address other devices use
function showNetworkState(state) {
    restartBox.hidden = !state.restart_needed;
    const url = document.getElementById("network-url");
    url.hidden = !state.network_url;
    url.textContent = state.network_url ? "Other devices open: " + state.network_url : "";
}

// Not saved: put the choice and port back to what's in config.toml (half-typed names stay)
async function saveNetwork(changes) {
    if (!(await save(changes, netStatus))) {
        setNetworkInputs((await getState()).settings);
    }
}

// The two choices (the "Set in config.toml" one has no value, so it's left out)
document.querySelectorAll("input[name=bind][value]").forEach(radio => {
    radio.addEventListener("change", () => saveNetwork({ bind_address: radio.value }));
});

// "change" fires on Enter, on leaving the box, or on the arrows; not on every key
portBox.addEventListener("change", async () => {
    const port = Number(portBox.value);
    if (!Number.isInteger(port) || port < 1 || port > 65535) {
        setStatus(netStatus, "The port must be a whole number from 1 to 65535.", true);
        setNetworkInputs((await getState()).settings);
        return;
    }
    saveNetwork({ port });
});

// Other names for this PC: one row each, saved once typing pauses
function addHostRow(name = "") {
    const box = el("input", { value: name, placeholder: "e.g. desktop-abc.lan" });
    const remove = el("button", { className: "close", title: "Remove this name" }, "×");
    const row = el("div", { className: "host" }, box, remove);
    box.addEventListener("input", queueHostsSave);
    remove.addEventListener("click", () => { row.remove(); queueHostsSave(); });
    hostsEl.appendChild(row);
    return row;
}

document.getElementById("host-add").addEventListener("click", () => {
    addHostRow().querySelector("input").focus();
});

let hostsTimer;
function queueHostsSave() {
    clearTimeout(hostsTimer);
    hostsTimer = setTimeout(saveHosts, 600);
}

// Empty rows are skipped; an empty list is fine here
function saveHosts() {
    const names = [...hostsEl.querySelectorAll("input")].map(box => box.value.trim()).filter(Boolean);
    save({ allowed_hosts: names }, netStatus);
}

// Restart, then reopen this page on the (maybe new) port once the new copy answers
document.getElementById("restart").addEventListener("click", async event => {
    const button = event.currentTarget;
    button.disabled = true;
    restarting = true;
    barsSocket.close();
    setStatus(netStatus, "Restarting...");
    try {
        const port = (await getState()).settings.port;
        const res = await fetch("/restart", { method: "POST" });
        if (!res.ok) throw new Error(await res.text());

        // This copy closes within ~2 seconds; wait until it stops answering...
        for (let i = 0; i < 20; i++) {
            await wait(250);
            try { await fetch("/version", { cache: "no-store" }); } catch { break; }
        }
        // ...then for the new copy. ("no-cors": it may be on another port; we only need to know it answers)
        const base = `${location.protocol}//${location.hostname}:${port}`;
        for (let i = 0; i < 60; i++) {
            await wait(500);
            try {
                await fetch(base + "/version", { mode: "no-cors", cache: "no-store" });
                location.href = base + "/settings.html";
                return;
            } catch { /* not up yet */ }
        }
        setStatus(netStatus, "Now Playing didn't start again. Check for its tray icon, and nowplaying.log.", true);
    } catch (err) {
        setStatus(netStatus, err.message || "Couldn't reach Now Playing.", true);
    }
    restarting = false;
    connectBars();
    button.disabled = false;
});

// ==============================
// Start
// ==============================

load().catch(() => {
    notice.textContent = "Couldn't load the settings. Is Now Playing running?";
    notice.hidden = false;
});
connectBars();
loadDetected();
setInterval(loadDetected, 5000);