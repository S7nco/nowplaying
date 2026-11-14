// Theme gallery page script (gallery.html)

const LABELS = { examples: "Examples", styles: "Styles", "game-inspired": "Game Inspired", custom: "Custom" };
let themes = {};
let filter = "all";

// Preview settings
const PAD = 12;     // space around the overlay inside its card, in pixels
const MAX_H = 240;  // tallest a preview a card can be, in pixels
const MARGIN = 200; // previews start loading this far before they scroll into view, and unload this far after
const LOAD_GAP = 120; // milliseconds before starting one preview and the next
let playing = true;

// ====================================================
// Favorites (saved in this browser)
// ====================================================

const FAV_KEY = "nowplaying-favorites";
let favorites = new Set();
try { favorites = new Set(JSON.parse(localStorage.getItem(FAV_KEY) || "[]")); } catch (e) {}

const MIGRATED_KEY = "nowplaying-favorites-migrated";

function saveFavorites() {
    try { localStorage.setItem(FAV_KEY, JSON.stringify([...favorites])); } catch (e) {}
    fetch("/favorites", {
        method: "PUT",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify([...favorites]),
    }).catch(() => {});
}

async function loadFavorites() {
    try { 
        const res = await fetch("/favorites", { cache: "no-store" });
        const server = await res.json();
        server.forEach((n) => favorites.add(n));
        // one time: merge stars you set before this feature existed
        if (!localStorage.getItem(MIGRATED_KEY)) {
            const before = favorites.size;
            const local = JSON.parse(localStorage.getItem(FAV_KEY) || "[]");
            local.forEach((n) => favorites.add(n));
            localStorage.setItem(MIGRATED_KEY, "1");
            if (favorites.size !== before) saveFavorites();
        }
    } catch {}
}

function styleStar(button, on) {
    button.classList.toggle("on", on);
    button.textContent = on ? "★" : "☆";
    button.title = on ? "Remove from favorites" : "Add to favorites";
}

function toggleFavorite(name) {
    if (favorites.has(name)) favorites.delete(name); else favorites.add(name);
    saveFavorites();
    // update every card for this theme (it can be in Favorites and its own group)
    document.querySelectorAll(".card").forEach(card => {
        if (card.dataset.name === name) styleStar(card.querySelector(".fav"), favorites.has(name));
    });
    renderFavorites();
    updateCount();
}

// ====================================================
// Play / Pause Toggle
// ====================================================

function applyPlaying(frame) {
    try {
        frame.contentWindow.__previewPaused = !playing;
        const ov = frame.contentDocument.getElementById("overlay");
        const want = playing ? "1" : "0";
        if (ov && ov.dataset.playing !== want) ov.dataset.playing = want;
    } catch (e) {}
}

function applyPlayingAll() {
    document.querySelectorAll(".view iframe").forEach(applyPlaying);
}

// ====================================================
// Loading Previews
// Only the previews near the screen are alive. Ones that scroll far away are
// removed (and brought back when you scroll to them), and they start one at a
// time so opening the page doesn't freeze it
// ====================================================

const loadQueue = [];
let pumping = false;

function pump() {
    if (pumping) return;
    pumping = true;
   const timer = setInterval(() => {
    const view = loadQueue.shift();
    if (!view) { clearInterval(timer); pumping = false; return; }
    if (view.isConnected) loadView(view);
   }, LOAD_GAP);
}

function loadView(view) {
    if (view.querySelector("iframe")) return;
    const frame = document.createElement("iframe");
    frame.src = `overlay.html?theme=${encodeURIComponent(view.dataset.theme)}&bg=transparent`;
    frame.setAttribute("tabindex", "-1");
    view.appendChild(frame);
    view.querySelector(".ph")?.remove();
    whenReady(frame, view);
}

function unloadView(view) {
    const i = loadQueue.indexOf(view);
    if (i >= 0) loadQueue.splice(i, 1);
    const frame = view.querySelector("iframe");
    if (!frame) return;
    frame.remove();
    if (!view.querySelector(".ph")) {
        const ph = document.createElement("div");
        ph.className = "ph";
        ph.textContent = "Loading Preview";
        view.appendChild(ph);
    }
}

const observer = new IntersectionObserver((entries) => {
    entries.forEach(entry => {
        const view = entry.target;
        if (entry.isIntersecting) {
            if (!view.querySelector("iframe") && !loadQueue.includes(view)) {
                loadQueue.push(view);
                pump();
            }
        } else {
            unloadView(view);
        }
    });
}, { rootMargin: `${MARGIN}px` });

// Removes everything in a container and stops watching its previews
function clearViews(container) {
    container.querySelectorAll(".view").forEach(view => {
        observer.unobserve(view);
        unloadView(view);
    });
    container.innerHTML = "";
}

// Waits for the overlay inside the frame to start, then sizes the card around it
function whenReady (frame, view) {
    let tries = 0;
    const timer = setInterval(() => {
        const overlay = frame.contentDocument?.getElementById("overlay");
        tries++;
        if (!frame.isConnected) { clearInterval(timer); return; }
        if (overlay && overlay.dataset.phase) {
            clearInterval(timer);
            // popup themes are hidden until a song change, so show them
            const enforce = () => {
                if (overlay.dataset.phase !== "track") overlay.dataset.phase = "track";
                applyPlaying(frame);
            };
            const hold = setInterval(() => {
                if (!frame.isConnected) { clearInterval(hold); return; }
                enforce();
            }, 500);
            new MutationObserver(enforce)
                .observe(overlay, { attributes: true, attributeFilter: ["data-phase", "data-playing"] });
            enforce();
            layout(view);
            setTimeout(() => layout(view), 700);    // again once fonts and text have settled
            setTimeout(() => layout(view), 2000);
            setTimeout(() => showTags(frame, view), 1500);
        } else if (tries > 40) {
            clearInterval(timer);
            layout(view);
        }
    }, 100);
}

// Works out what theme includes by asking the page inside the frame, so teh list never goes stale
function showTags(frame, view) {
    if (!frame.isConnected) return;
    const win = frame.contentWindow;
    const doc = frame.contentDocument;
    const shown = id => {
        const el = doc.getElementById(id);
        return !!el && win.getComputedStyle(el).display !== "none";
    };
    const setting = name => win.getComputedStyle(doc.documentElement).getPropertyValue(name).trim();

    const tags = [];
    if (setting("--popup") === "1") tags.push("popup");
    if (shown("eq")) tags.push("equalizer");
    if(setting("--spectrum") === "1" || setting("--ring-bars")) tags.push("music reactive");
    if(shown("artwork")) tags.push("cover");
    if(shown("progress")) tags.push("progress bar");
    if(shown("time")) tags.push("time");
    if(shown("album")) tags.push("album");
    if(shown("line")) tags.push("one line");

    const box = view.parentElement.querySelector(".tags");
    box.replaceChildren(...tags.map(t => {
        const chip = document.createElement("span");
        chip.className = "tag";
        chip.textContent = t;
        return chip;
    }));
}

// Measures the overlay and scales the frame so the overlay fits the card, centered
function layout(view) {
    const frame = view.querySelector("iframe");
    const overlay = frame?.contentDocument?.getElementById("overlay");
    if (!overlay) return;
    const r = overlay.getBoundingClientRect();
    if (!r.width || !r.height) return;

    const cardW = view.clientWidth;
    const scale = Math.min((cardW - PAD * 2) / r.width, (MAX_H - PAD * 2) / r.height);
    view.style.height = Math.round(r.height * scale + PAD * 2) + "px";
    const x = (cardW - r.width * scale) / 2 - r.left * scale;
    const y = PAD - r.top * scale;
    frame.style.transform = `translate(${x}px, ${y}px) scale(${scale})`;
}

// the window can fire many resize events at once, so measure once per frame
let resizeFrame;
window.addEventListener("resize", () => {
    cancelAnimationFrame(resizeFrame);
    resizeFrame = requestAnimationFrame(() => {
        document.querySelectorAll(".view").forEach(layout);
    });
});

function themeUrl(name) {
    return `${location.origin}/overlay.html?theme=${encodeURIComponent(name)}`;
}

function copy(text, button) {
    const done = () => {
        const old = button.textContent;
        button.textContent = "Copied!";
        button.classList.add("done");
        setTimeout(() => { button.textContent = old; button.classList.remove("done"); }, 1200);
    };
    if (navigator.clipboard) {
        navigator.clipboard.writeText(text).then(done).catch(() => { fallbackCopy(text, done); });
    } else {
        fallbackCopy(text, done);
    }
}
function fallbackCopy(text, done) {
    const box = document.createElement("textarea");
    box.value = text;
    document.body.appendChild(box);
    box.select();
    document.execCommand("copy");
    box.remove();
    done();
}

// ====================================================
// Cards and Sections
// ====================================================

function makeCard(name) {
    const card = document.createElement("div");
    card.className = "card";
    card.dataset.name = name;
    card.innerHTML = `
        <div class="view" data-theme="${name}"><div class="ph">Loading Preview</div></div>
        <div class="row">
            <span class="name"></span>
            <div class="action">
                <button class="fav"></button>
                <a class="open" target="_blank" rel="noopener">Open</a>
                <button class="copy">Copy URL</button>
            </div>
        </div>
        <div class="tags"></div>`;
    card.querySelector(".name").textContent = 
        name.replace(/[-_]/g, " ").replace(/\b\w/g, c => c.toUpperCase());
    card.querySelector(".open").href = themeUrl(name);
    card.querySelector(".copy").addEventListener("click", e => copy(themeUrl(name), e.target));
    const star = card.querySelector(".fav");
    styleStar(star, favorites.has(name));
    star.addEventListener("click", () => toggleFavorite(name));
    observer.observe(card.querySelector(".view"));
    return card;
}

function allThemeNames() {
    return [...(themes.examples || []), ...(themes.styles || []), ...(themes["game-inspired"] || []), ...(themes.custom || [])];
}

function searchQuery() {
    return document.getElementById("search").value.trim().toLowerCase();
}

// The Favorites section at the top. Cards that are already there stay as they are,
// so adding or removing one doesn't reload the others.
function renderFavorites(rebuild = false) {
    const box = document.getElementById("favorites");
    const query = searchQuery();
    const known = allThemeNames();
    const wanted = (filter === "all" || filter === "favorites") 
        ? [...favorites].filter(n => known.includes(n) && n.toLowerCase().includes(query))
        : [];

    if (rebuild || wanted.length === 0) clearViews(box);

    if (wanted.length === 0) {
        if (filter === "favorites") {
            const note = document.createElement("div");
            note.className = "empty";
            note.textContent = favorites.size === 0
                ? "No favorites yet. Click on the ☆ on a theme to add it here."
                : "No results found.";
            box.appendChild(note);
        }
        return;
    }
    
    box.querySelector(".empty")?.remove();
    let grid = box.querySelector(".grid");
    if (!grid) {
        const title = document.createElement("h2");
        box.appendChild(title);
        grid = document.createElement("div");
        grid.className = "grid";
        box.appendChild(grid);
    }
    box.querySelector("h2").textContent = `★ Favorites (${wanted.length})`;

    // take out the ones that are no longer favorites
    [...grid.children].forEach(card => {
        if (!wanted.includes(card.dataset.name)) {
            clearViews(card);
            card.remove();
        }
    });

    // add the new ones to the end
    wanted.forEach(name => {
        const has = [...grid.children].some(card => card.dataset.name === name);
        if (!has) grid.appendChild(makeCard(name));
    });
}

function updateCount() {
    const el = document.getElementById("count");
    if (filter === "favorites") {
        const n = document.querySelectorAll("#favorites .card").length;
        el.textContent = `Showing ${n} favorite ${n === 1 ? "theme" : "themes"}.`;
        return;
    }
    const total = allThemeNames().length;
    const shown = document.querySelectorAll("#sections .card").length;
    el.textContent = `Showing ${shown} of ${total} themes.`;
}

function render() {
    const query = searchQuery();
    const sections = document.getElementById("sections");
    clearViews(sections);

    for (const group of ["examples", "styles", "game-inspired", "custom"]) {
        const all = themes[group] || [];
        if (filter !== "all" && filter !== group) continue;
        const names = all.filter(n => n.toLowerCase().includes(query));
        if (names.length === 0 && query) continue;

        const title = document.createElement("h2");
        title.textContent = `${LABELS[group]} (${names.length})`;
        sections.appendChild(title);

        if (names.length === 0) {
            const note = document.createElement("div");
            note.className = "empty";
            note.textContent = "No results found.";
            sections.appendChild(note);
            continue;
        }

        const grid = document.createElement("div");
        grid.className = "grid";
        names.forEach(name => grid.appendChild(makeCard(name)));
        sections.appendChild(grid);
    }
    renderFavorites(true);
    updateCount();
}

let searchTimer;
document.getElementById("search").addEventListener("input", () => {
    clearTimeout(searchTimer);
    searchTimer = setTimeout(render, 300);
});
document.querySelectorAll(".chip[data-filter]").forEach(chip => chip.addEventListener("click", () => {
    document.querySelectorAll(".chip[data-filter]").forEach(c => c.classList.remove("on"));
    chip.classList.add("on");
    filter = chip.dataset.filter;
    render();
}));
document.getElementById("playToggle").addEventListener("click", e => {
    playing = !playing;
    e.target.classList.toggle("paused", !playing);
    e.target.textContent = playing ? "Playing" : "Paused";
    applyPlayingAll();
});

// =======================================
// Preview Background
// =======================================
const BG_KEY = "nowplaying-gallery-bg";

function setPreviewBg(name) {
    document.body.dataset.bg = name;
    document.querySelectorAll(".sw").forEach(c =>
        c.classList.toggle("on", c.dataset.bg === name));
    try { localStorage.setItem(BG_KEY, name); } catch (e) {}
}

document.querySelectorAll(".sw").forEach(chip => 
    chip.addEventListener("click", () => setPreviewBg(chip.dataset.bg)));

let savedBg = "dark";
try { savedBg = localStorage.getItem(BG_KEY) || "dark"; } catch (e) {}
setPreviewBg(savedBg);

fetch("/themes.json")
    .then(r => r.json())
    .then(data => { themes = data; })
    .then(loadFavorites)
    .then(render)
    .catch(() => { document.getElementById("count").textContent = "Couldn't load the theme list. Is the server running?"; });