// Settings page (settings.html): reads and changes config.toml

const EQ = ["eq_floor_db", "eq_ceil_db", "eq_tilt_db"];
const notice = document.getElementById("notice");
const eqStatus = document.getElementById("eq-status");
let defaults = {};

// ==============================
// Reading the settings
// ==============================

async function load() {
    const res = await fetch("/settings", { cache: "no-store" });
    const state = await res.json();
    defaults = state.defaults || {};
    EQ.forEach(name => setSlider(name, state.settings[name]));
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
// Sliders 
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

// Save once the slider has rested for a moment, not on every pixel of a drag
let saveTimer;
EQ.forEach(name => {
    const input = document.getElementById(name);
    input.addEventListener("input", () => {
        showValue(input);
        clearTimeout(saveTimer);
        saveTimer = setTimeout(() => save({ [name]: Number(input.value) }), 300);
    });
});

document.getElementById("eq-reset").addEventListener("click", () => {
    const changes = {};
    EQ.forEach(name => {
        changes[name] = defaults[name];
        setSlider(name, defaults[name]);
    });
    save(changes);
});

// ==============================
// Saving
// ==============================

function setStatus(text, bad = false) {
    eqStatus.textContent = text;
    eqStatus.classList.toggle("bad", bad);
}

async function save(changes) {
    setStatus("Saving...");
    let res, text;
    try {
        res = await fetch("/settings", {
            method: "PUT",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify(changes),
        });
        text = await res.text();
    } catch {
        setStatus("Couldn't reach Now Playing. Is it running?", true);
        return;
    }
    let reply;
    try { reply = JSON.parse(text); } catch {
        // not JSON: a refusal from the guard, e.g. "Changes can only be made on the PC running Now Playing"
        setStatus(text, true);
        load();
        return;
    }
    if (!res.ok) {
        setStatus(Object.values(reply.errors || {}).join(" "), true);
        load(); // put the sliders back to what's saved
        return;
    }
    showState(reply);
    setStatus("Saved");
}

// ==============================
// Live bars, from the same feed the overlay uses
// ==============================

const bars = document.getElementById("bars");
for (let i = 0; i < 8; i++) bars.appendChild(document.createElement("span"));

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

load().catch(() => {
    notice.textContent = "Couldn't load the settings. Is Now Playing running?";
    notice.hidden = false;
});
connectBars();