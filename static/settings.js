// Settings page (settings.html): reads and changes config.toml

const EQ = ["eq_floor_db", "eq_ceil_db", "eq_tilt_db"];
const notice = document.getElementById("notice");
const eqStatus = document.getElementById("eq-status");
const playersEl = document.getElementById("players");
const playersStatus = document.getElementById("players-status");
const detectedEl = document.getElementById("detected");
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
    showState(state);
}

// Notices at the top: a broken config.toml, unknown setting names, a Restart waiting
function showState(state) {
    const notes = [];
    if (state.error) notes.push("config.toml has an error, so changes can't be saved until it's fixed:\n" + state.error);
    if (state.unknown && state.unknown.length) notes.push("config.toml has settings the app doesn't know (check their spelling): " + state.unknown.join(", "));
    if (state.restart_needed) notes.push("Restart Now Playing from the tray menu to use the new port or network setting.");
    notice.textContent = notes.join("\n\n");
    notice.hidden = notes.length === 0;
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

function connectBars() {
    const socket = new WebSocket(`ws://${location.host}/ws?spectrum=1`);
    socket.onmessage = event => {
        const msg = JSON.parse(event.data);
        if (msg.type !== "spectrum") return;
        msg.levels.forEach((level, i) => {
            if (bars.children[i]) bars.children[i].style.height = (level * 100) + "%";
        });
    };
    socket.onclose = () => setTimeout(connectBars, 2000);
}

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
// Start
// ==============================

load().catch(() => {
    notice.textContent = "Couldn't load the settings. Is Now Playing running?";
    notice.hidden = false;
});
connectBars();
loadDetected();
setInterval(loadDetected, 5000);