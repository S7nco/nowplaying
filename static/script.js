 // ============================================================
 // Now Playing Overlay
 // Sections: Settings, Page Elements, State, Theme Loading,
 //           Helpers, Progress Bar, Data Polling, Marquee Text,
 //           Equalizer, Ring of Bars, Popup mode, Start
 //
 // ============================================================

 // ============================================================
 // Settings
 // ============================================================

 // Load the theme chosen in the URL (?theme=name), defaulting to the default theme if not specified
const params = new URLSearchParams(window.location.search);

// Where the server lives (works from OBS, a browser, or another device)
const API = location.protocol.startsWith("http") ? location.origin : "http://localhost:5000";

// Theme from the URL (?theme=name), defaulting to "default". Letters, numbers, _ and - only.
const theme = (params.get("theme") || "default").replace(/[^a-z0-9_-]/gi, "");

// Gallery preview (?preview=1): popup themes stay visible, and the gallery controls play/pause
const PREVIEW = params.get("preview") === "1";

// How often things update, in milliseconds
const NOWPLAYING_POLL_MS = 5000;
const PROGRESS_TICK_MS = 200;
const SPECTRUM_POLL_MS = 70;    // about 14 updates per second

// Progress bar: seconds of disagreement before we trust the server's position
const RESYNC_THRESHOLD = 2.5;

// Scrolling text
const MARQUEE_SPEED = 40;   // default pixels per second
const MARQUEE_PAUSE = 2;    // default seconds to wait before each scroll
const MARQUEE_GAP = "\u00A0\u00A0\u00A0\u2022\u00A0\u00A0\u00A0"; // " • " between the two copies

// Equalizer
const SEGMENTS = 16;        // must match the segment count in the theme's CSS
const EQ_FALL = 0.85;       // how slowly bars fall (closer to 1 = slower)

// ============================================================
// Page Elements
// ============================================================

const overlayEl = document.getElementById("overlay");
const themeLink = document.getElementById("theme-link");
const artworkEl = document.getElementById("artwork");
const stationEl = document.getElementById("station");
const eqEl = document.getElementById("eq");
const eqBars = document.querySelectorAll("#eq span");
const progressEl = document.getElementById("progress-bar"); 
const elapsedEl = document.getElementById("time-elapsed");  // may not exist in every theme
const totalEl = document.getElementById("time-total");      // may not exist in every theme

// ============================================================
// State
// ============================================================

// Progress Bar Estimation
let lastKnownPosition = 0;
let lastKnownDuration = 0;
let lastSyncTime = Date.now();
let isPlaying = false;
let lastTrack = null;

// Last cover shown, so the blurred background only updates when the cover changes
let lastCover = "";

// Equalizer bar heights (smoothed)
const eqLevels = new Array(eqBars.length).fill(0);

// Does this theme want audio levels? (decided once the theme has loaded)
let useSpectrum = false;
let energy = 0;

// Pending popup timers
let popupTimers = [];

// The latest song data (the gallery's Replay popup button uses it)
let lastData = null;

// "1" or "0" for data-playing: in a gallery preview the Play/Pause button decides, otherwise the player does
function playingAttr() {
    if (PREVIEW) return window.__previewPaused ? "0" : "1";
    return isPlaying ? "1" : "0";
}

// The gallery's "Replay popup" button calls this: hide, then run the popup like a song change
window.replayPopup = () => {
    if (!popupConfig().enabled || !lastData) return;
    popupTimers.forEach(clearTimeout);
    popupTimers = [];
    overlayEl.dataset.phase = "hidden";     // start hidden, so the entrance animation plays
    popupTimers.push(setTimeout(() =>
        showPopup(cardTextFor(cleanAlbum(lastData.album), lastData.title, lastData.album)), 600));
};

// The gallery's Play/Pause button calls this
window.setPreviewPaused = (paused) => {
    window.__previewPaused = paused;
    overlayEl.dataset.playing = playingAttr();
};

// ============================================================
// Theme Loading
// ============================================================
async function loadTheme(name) {
    let found;
    try {
        const folders = await (await fetch(`${API}/themes.json`)).json();
        const all = folders.flatMap(f => f.themes.map(theme => ({ folder: f.name, theme })));
        // the exact spelling first, then any capitalization (?theme=halloween finds Halloween.css)
        const lower = name.toLowerCase();
        found = all.find(t => t.theme === name) || all.find(t => t.theme.toLowerCase() === lower);
    } catch (e) {}
    if (!found) { start(); return; }   //not found: a bad theme name still starts the page

    themeLink.addEventListener("load", start, { once: true });
    themeLink.addEventListener("error", start, { once: true });
    themeLink.href = `themes/${found.folder}/${found.theme}.css`;
}

// Optional page background from the URL (?bg=black). Black in a browser, none inside OBS.
function applyBackground() {
    const bg = params.get("bg") || (window.obsstudio ? null : "black");
    if (bg && CSS.supports("color", bg)) document.body.style.background = bg; 
}

// Optional size from the URL (?scale=1.5)
function applyScale() {
    const scale = parseFloat(params.get("scale"));
    if (scale > 0 && scale <= 5) document.body.style.zoom = scale;
}

// ============================================================
// Helpers
// ============================================================

// Cleans "Single or EP" from the end of album names
function cleanAlbum(album) {
    return album ? album.replace(/\s+-\s+(Single|EP)$/i, "") : "";
}

// 83 -> "1:23"
function formatTime(seconds) {
    const s = Math.max(0, Math.floor(seconds));
    const h = Math.floor(s / 3600);
    const m = Math.floor(s / 60) % 60;
    const sec = String(s % 60).padStart(2, "0");
    return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${sec}` : `${m}:${sec}`;
}

// Text for the popup's album card: empty if the album just repeats the song title
function cardTextFor(album, title, rawAlbum) {
    if (!album) return "";
    // a single named after the song: show "Song - Single" instead of repeating the title
    if (album.toLowerCase() === (title || "").toLowerCase()) return rawAlbum || "";
    return album;
}

// Reads a CSS variable from the theme (used for popup settings)
function cssSetting(name) {
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// A number from the theme, or the fallback if it's missing or outside a sensible range (like 1e999)
function cssNumber(name, fallback, min, max) {
    const value = parseFloat(cssSetting(name));
    return Number.isFinite(value) && value >= min && value <= max ? value : fallback;
}

// Sets a CSS variable / text only when it changed, so the browser skips needless work
function setVar(el, name, value) {
    if (el.style.getPropertyValue(name) !== String(value)) el.style.setProperty(name, value);
}
function setText(el, text) {
    if (el.textContent !== text) el.textContent = text;
}

// ============================================================
// Progress Bar
// ============================================================

//The server only reports the position every few seconds, so we estimate the in between
function estimatedPosition() {
    if (!isPlaying) return lastKnownPosition;
    return lastKnownPosition + (Date.now() - lastSyncTime) / 1000;
}

// Takes the server's latest data and re-anchors the estimate when needed
function syncProgress(data) {
    // Nothing playing: no song at all
    if (!data.title) { 
        lastKnownDuration = 0;
        lastTrack = null;
        isPlaying = false;
        overlayEl.dataset.playing = playingAttr();
        delete overlayEl.dataset.live;
        progressEl.style.width = "0%";
        return;
    }

    const trackKey = data.title + "|" + data.artist;
    const trackChanged = trackKey !== lastTrack;
    const playing = data.playing === true;
    const stateChanged = trackChanged || playing !== isPlaying;
    
    // Live radio and streams have no end time: no progress bar, but still playing and still popping up
    const live = data.position == null || !(data.duration > 0);
    if (live) {
        lastKnownDuration = 0;
    } else {
        // Only re-anchor on a new song, a play/pause change, or when the drift is too large
        const drift = data.position - estimatedPosition();
        if (stateChanged || Math.abs(drift) > RESYNC_THRESHOLD) {
            lastKnownPosition = data.position;
            lastSyncTime = Date.now();
        }
        lastKnownDuration = data.duration;
    }
    lastTrack = trackKey;
    isPlaying = playing;
    overlayEl.dataset.playing = playingAttr();
    overlayEl.dataset.live = live ? "1" : "0";

    if (trackChanged) {
        overlayEl.dataset.skip = "1";
        setTimeout(() => delete overlayEl.dataset.skip, 450);
    }
    if (trackChanged && popupConfig().enabled && !PREVIEW) {
        showPopup(cardTextFor(cleanAlbum(data.album), data.title, data.album));
    }
}

// Redraws the bar and the time text from the estimated position
function tickProgressBar() {
    if (window.__previewPaused) return;
    if (lastKnownDuration > 0) {
        const position = Math.min(lastKnownDuration, estimatedPosition());
        const progress = Math.min(100, (position / lastKnownDuration) * 100);
        progressEl.style.width = progress + "%";
        setVar(overlayEl, "--progress", (progress / 100).toFixed(3));
        if (elapsedEl) setText(elapsedEl, formatTime(position));
        if (totalEl) setText(totalEl, formatTime(lastKnownDuration));
    } else {
        setVar(overlayEl, "--progress", "0");
        if (elapsedEl) setText(elapsedEl, "0:00");
        if (totalEl) setText(totalEl, "0:00");
        progressEl.style.width = "0%";
    }
}

// ============================================================
// Data Polling
// ============================================================

// Sets the artwork image and the blurred variable (--cover)
function updateArtwork(data) {
    artworkEl.hidden = !data.artwork;
    if (!data.artwork) return;

    const src = data.artwork.startsWith("/") ? API + data.artwork : data.artwork;

    // Skip loading the image if the theme hides the artwork
    if (getComputedStyle(artworkEl).display !== "none") artworkEl.src = src;

    // Themes can use var(--cover) for a blurred background
    if (src !== lastCover) {
        lastCover = src;
        overlayEl.style.setProperty("--cover", `url("${src}")`);
    }
}

function applyNowPlaying(data) {
    lastData = data;
    setMarquee("title", data.title);
    setMarquee("artist", data.artist);
    setMarquee("album", cleanAlbum(data.album));
    setMarquee("line", [data.title, data.artist].filter(Boolean).join(" - "));
    updateArtwork(data);
    syncProgress(data);
}

// Fetches the current song and updates the whole overlay
function updateNowPlaying() {
    if (socketOpen) return;
    fetch(`${API}/nowplaying`)
        .then(response => response.json())
       .then(applyNowPlaying)
       .catch(console.error);
}

// ============================================================
// Marquee text (scrolls when the text is too long for its box)
// ============================================================
function setMarquee(id, text) {
    const el = document.getElementById(id);
    const span = el.firstElementChild;
    text = text ?? "";

    if (span.dataset.text === text) return; //unchanged: don't restart the animation
    span.dataset.text = text;

    span.getAnimations().forEach(a => a.cancel());
    el.classList.remove("scrolling");
    span.textContent = text;

    if (span.offsetWidth > el.clientWidth) {                        // Too long for the box
        span.textContent = text + MARQUEE_GAP + text + MARQUEE_GAP;

        //Theme settings: use them only if they're sensible numbers, otherwise the defaults
        const speed = cssNumber("--marquee-speed", MARQUEE_SPEED, 0.1, 10000);
        const pause = cssNumber("--marquee-pause", MARQUEE_PAUSE, 0, 600);
        const scroll = (span.offsetWidth / 2) / speed;
        const total = scroll + pause; 
        span.animate([
            { transform: "translateX(0)" },
            { transform: "translateX(0)", offset: pause / total },
            { transform: "translateX(-50%)" }
        ], { duration: total * 1000, iterations: Infinity });
            
        el.classList.add("scrolling");
    }
}

// Re-measure the scrolling text whenever a font finishes loading
function remeasureMarquees() {
    document.querySelectorAll(".marquee").forEach(el => {
        const span = el.firstElementChild;
        const text = span.dataset.text;
        if (text === undefined) return;
        delete span.dataset.text;
        setMarquee(el.id, text);
    });
}

// Restarts the scroll animation from the beginning
function restartMarquees() {
    document.querySelectorAll(".marquee.scrolling").forEach(el => {
        el.firstElementChild.getAnimations().forEach(a => { a.currentTime = 0; });
    });
}

// ============================================================
// Equalizer
// ============================================================

function applySpectrum(levels) {
    levels.forEach((target, i) => {
        //rise instantly, fall gradually, so bars don't flicker
        eqLevels[i] = target > eqLevels[i]
            ? target
            : eqLevels[i] * EQ_FALL + target * (1 - EQ_FALL);
        //snap to whole segments for the blocky led look
        const snapped = Math.round(eqLevels[i] * SEGMENTS) / SEGMENTS;
        setVar(eqBars[i], "--level", snapped);
    });
    // bass level (0 to 1) for themes that react to the music, e.g. a glow
    const raw = Math.max(levels[0], levels[1], levels[2]);
    const punch = Math.pow(raw, 1.8);
    energy = punch > energy ? punch : energy * 0.55 + punch * 0.45;
    setVar(overlayEl, "--energy", energy.toFixed(2));
    updateRing();
}

function updateSpectrum() {
    //skip the request if the theme doesn't include the equalizer
    if (!useSpectrum) return;
    if (window.__previewPaused) return;
    if (socketOpen) return;

    fetch(`${API}/spectrum`)
        .then(response => response.json())
        .then(data => applySpectrum(data.levels))
        .catch(() => {});   //ignore errors while the server restarts
}

let socket = null;
let socketOpen = false;

function connectSocket() {
    const url = API.replace(/^http/, "ws") + "/ws" + (useSpectrum ? "?spectrum=1" : "");
    socket = new WebSocket(url);
    socket.onopen = () => { socketOpen = true; };
    socket.onmessage = (event) => {
        const msg = JSON.parse(event.data);
        if (msg.type === "nowplaying") {
            applyNowPlaying(msg.data);
        } else if (msg.type === "spectrum" && !window.__previewPaused) {
            applySpectrum(msg.levels);
        }
    };
    socket.onclose = () => { 
        socketOpen = false;
        setTimeout(connectSocket, 2000);
    };
    socket.onerror = () => socket.close();
}

// ============================================================
// Ring of bars around the record (themes opt in with --ring-bars: 48)
// ============================================================
let ringBars = [];

function buildRing() {
    const n = parseInt(cssSetting("--ring-bars"));
    if (!n) return;
    const ring = document.createElement("div");
    ring.id = "ring";
    for (let i = 0; i < n; i++) {
        const bar = document.createElement("i");
        bar.style.setProperty("--a", (360 / n * i) + "deg");
        ring.appendChild(bar);
        ringBars.push(bar);
    }
    overlayEl.appendChild(ring);
}

function updateRing() {
    const n = ringBars.length;
    ringBars.forEach((bar, i) => {
        const t = Math.abs((i / n) * 2 - 1);
        const pos = t * (eqLevels.length - 1 );
        const lo = Math.floor(pos);
        const hi = Math.min(lo + 1, eqLevels.length - 1);
        const v = eqLevels[lo] + (eqLevels[hi] - eqLevels[lo]) * (pos - lo);
        setVar(bar, "--v", v.toFixed(2));
    })
}

// ============================================================
// Popup Mode (set by the theme's CSS variables: --popup, --popup-card, ...)
// ============================================================

function popupConfig() {
    return {
        enabled: cssSetting("--popup") === "1",
        card: cssSetting("--popup-card") || "album",
        cardMs: cssNumber("--popup-card-time", 2.5, 0.1, 600) * 1000,
        showMs: cssNumber("--popup-show-time", 7, 0.1, 600) * 1000
    }
}

// Phases: "station" (album card) -> "track" (song_info) -> "hidden"
function showPopup(albumText) {
    const cfg = popupConfig();
    popupTimers.forEach(clearTimeout);
    popupTimers = [];

    let cardText = "";
    if (cfg.card === "album") cardText = albumText || "";
    else if (cfg.card !== "none") cardText = cfg.card.replace(/^["']|["']$/g, "");      //strip quotes
    stationEl.textContent = cardText;                                                   //textContent, not innerHTML

    const cardMs = cardText ? cfg.cardMs : 0;
    overlayEl.dataset.phase = cardText ? "station" : "track";

    popupTimers.push(setTimeout(() => {
        overlayEl.dataset.phase = "track";
        restartMarquees();
    }, cardMs));
    popupTimers.push(setTimeout(() => {
        overlayEl.dataset.phase = "hidden";
    }, cardMs + cfg.showMs));

     // a gallery preview comes back after the popup ends, so it doesn't stay blank
    if (PREVIEW) popupTimers.push(setTimeout(() => {
        overlayEl.dataset.phase = "track";
    }, cardMs + cfg.showMs + 1200));
}

// ============================================================
// Start
// ============================================================

// Runs once the theme has loaded (or failed everywhere)
function start() {
    useSpectrum = getComputedStyle(eqEl).display !== "none" || cssSetting("--spectrum") === "1";
    if (useSpectrum) buildRing();
    overlayEl.dataset.phase = PREVIEW ? "track" : (popupConfig().enabled ? "hidden" : "visible");     //always set a phase
    setInterval(updateNowPlaying, NOWPLAYING_POLL_MS);
    setInterval(tickProgressBar, PROGRESS_TICK_MS);
    setInterval(updateSpectrum, SPECTRUM_POLL_MS);
    updateNowPlaying();
    connectSocket();
    // Tell the gallery (if this is a preview) that the theme is applied
    window.overlayReady = true;
    window.dispatchEvent(new Event("overlay-ready"));
}

// Re-measure scrolling text whenever a font finishes loading
document.fonts.addEventListener("loadingdone", remeasureMarquees);
document.fonts.ready.then(remeasureMarquees);

applyBackground();
applyScale();
loadTheme(theme);