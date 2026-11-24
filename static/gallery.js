// Theme gallery page script (gallery.html)

const LABELS = name => name.split(/[-_]/).map(w => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");
let themes = [];

// Gallery shows examples first and custom last
function galleryOrder() {
    const rank = n => n === "examples" ? 0 : n === "custom" ? 2 : 1;
    return [...themes].sort((a, b) => rank(a.name) - rank(b.name));
}

function addFolderChips() {
    const row = document.querySelector(".filters");
    for (const f of galleryOrder()) {
        const chip = document.createElement("button");
        chip.className = "chip";
        chip.dataset.filter = f.name;
        chip.textContent = LABELS(f.name);

        const n = document.createElement("span");
        n.className = "n";
        n.textContent = f.themes.length;
        chip.appendChild(n);

        row.append(chip);
    }
}

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

// Adds or removes one theme on the server, then shows the server's full list,
// which also picks up anything another gallery tab changed
function saveFavorite(name, on) {
    try { localStorage.setItem(FAV_KEY, JSON.stringify([...favorites])); } catch (e) {}
    fetch("/favorites/" + encodeURIComponent(name), { method: on ? "PUT" : "DELETE" })
        .then(r => r.ok ? r.json() : Promise.reject())
        .then(applyServerFavorites)
        .catch(() => refreshFavorites());   // refused (e.g. from another device): show the server's list again
}

// Replaces this tab's favorites with the server's list and redraws the stars
function applyServerFavorites(list) {
    favorites = new Set(list);
    try { localStorage.setItem(FAV_KEY, JSON.stringify(list)); } catch (e) {}
    document.querySelectorAll(".fav").forEach(b =>
        styleStar(b, favorites.has(b.dataset.name))
    );
    renderFavorites();
    updateCount();
}

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
        const state = await res.json();
        if (state.saved) {
            // there is a saved favorites.json: the server's list is the truth
            favorites = new Set(state.list);
            try { localStorage.setItem(FAV_KEY, JSON.stringify(state.list)); } catch (e) {}
        } else if (favorites.size > 0) {
            // no saved file (fresh folder): this browser's stars go up to the server
            saveFavorites();
        } else {
            // nothing anywhere: show the default list from config.toml
            favorites = new Set(state.list);
        }
    } catch {}
}

// Coming back to this tab or window: pick up stars changed in another tab or browser
async function refreshFavorites() {
    try {
        const res = await fetch("/favorites", { cache: "no-store" });
        const state = await res.json();
        if (!state.saved) return; // nothing saved yet: keep what this tab shows
        const same = state.list.length === favorites.size && state.list.every(n => favorites.has(n));
        if (!same) applyServerFavorites(state.list);
    } catch {}
}
document.addEventListener("visibilitychange", () => {
    if (document.visibilityState === "visible") refreshFavorites();
});
window.addEventListener("focus", refreshFavorites);

function styleStar(button, on) {
    button.classList.toggle("on", on);
    button.textContent = on ? "★" : "☆";
    button.title = on ? "Remove from favorites" : "Add to favorites";
}

function toggleFavorite(name) {
    const on = !favorites.has(name);
    if (on) favorites.add(name); else favorites.delete(name);
    saveFavorite(name, on);
    // update every star for this theme (its card, the favorites copy, the enlarged view)
    document.querySelectorAll(".fav").forEach(b => {
        if (b.dataset.name === name) styleStar(b, favorites.has(name));
    });
    renderFavorites();
    updateCount();
}

// ====================================================
// Play / Pause Toggle
// ====================================================

function applyPlaying(frame) {
    try {
        frame.contentWindow.setPreviewPaused?.(!playing);
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
    frame.src = `overlay.html?theme=${encodeURIComponent(view.dataset.theme)}&bg=transparent&preview=1`;
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
// and re-sizes it whenever the overlay's size changes (fonts loading, a new song's text)
function whenReady (frame, view) {
    frame.addEventListener("load", () => {
        const win = frame.contentWindow;
        const ready = () => {
            if (!frame.isConnected) return;
            applyPlaying(frame);
            const overlay = win.document.getElementById("overlay");
            if (overlay) new win.ResizeObserver(() => layout(view)).observe(overlay);
            layout(view);
            showTags(frame, view);
            view.dispatchEvent(new CustomEvent("preview-ready", { detail: frame }));
        };
        if (win.overlayReady) ready();
        else win.addEventListener("overlay-ready", ready, { once: true });
    }, { once: true });
}

// Works out what theme includes by asking the page inside the frame, so the list never goes stale
function readTags(frame) {
    const win = frame.contentWindow;
    const doc = frame.contentDocument;
    // Does the theme show this element? (hidden elements are still in the DOM, but not visible)
    const shown = id => {
        const el = doc.getElementById(id);
        if (!el) return false;
        const wasHidden = el.hidden;
        el.hidden = false;
        const visible = win.getComputedStyle(el).display !== "none";
        el.hidden = wasHidden;
        return visible;
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
    return tags;
}

function showTags(frame, view) {
    if (!frame.isConnected) return;
    const box = view.parentElement.querySelector(".tags");
    box.replaceChildren(box.querySelector(".size"), ...readTags(frame).map(t => 
        el("span", { className: "tag" }, t)));
}

// Measures the overlay and scales the frame so the overlay fits the card, centered
function layout(view) {
    const frame = view.querySelector("iframe");
    const overlay = frame?.contentDocument?.getElementById("overlay");
    if (!overlay) return;
    const r = overlay.getBoundingClientRect();
    if (!r.width || !r.height) return;

    // Size for the OBS browser source: the overlay plus the page's padding (--offset) on both sides
    // times the scale picked in the enlarged view (?scale= zooms the whole page)
    const s = Number(view.dataset.urlScale) || 1;
    const obsW = Math.ceil((r.width + r.left * 2) * s);
    const obsH = Math.ceil((r.height + r.top * 2) * s);
    view.dataset.obsSize = `${obsW}×${obsH}`; 
    const sizeTag = view.parentElement.querySelector(".size");
    if (sizeTag) sizeTag.textContent = `${obsW}×${obsH}`;

    const cardW = view.clientWidth;
    const maxH = Number(view.dataset.maxH) || MAX_H;    // the enlarged view sets its own limits
    const maxScale = Number(view.dataset.maxScale) || Infinity;
    const scale = Math.min((cardW - PAD * 2) / r.width, (maxH - PAD * 2) / r.height, maxScale);
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

function themeUrl(name, scale = 1) {
    const url = `${location.origin}/overlay.html?theme=${encodeURIComponent(name)}`;
    return scale === 1 ? url : `${url}&scale=${scale}`;
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

// Makes an element: el("a", { className: "open", href: "/" }, "Open")
// Props are set as properties; children can be elements or text
function el(tag, props = {}, ...children) {
    const node = Object.assign(document.createElement(tag), props);
    node.append(...children);
    return node;
}

// "vinyl-eq-bars" -> "Vinyl Eq Bars"
function themeLabel(name) {
    return name.replace(/[-_]/g, " ").replace(/\b\w/g, c => c.toUpperCase());
}

function makeCard(name) {
    const view = el("div", { className: "view" }, el("div", { className: "ph" }, "Loading Preview"));
    view.dataset.theme = name;
    view.addEventListener("click", () => openBig(name));

    // Keyboard: Tab reaches the preview, Enter or Space opens it bigger
    view.tabIndex = 0;
    view.setAttribute("role", "button");
    view.setAttribute("aria-label", `Enlarge ${themeLabel(name)}`);
    view.addEventListener("keydown", e => {
        if (e.key === "Enter" || e.key === " ") {
            e.preventDefault(); // Space would otherwise scroll the page
            openBig(name);
        }
    });

    const star = el("button", { className: "fav" });
    styleStar(star, favorites.has(name));
    star.dataset.name = name;
    star.addEventListener("click", () => toggleFavorite(name));

    const copyBtn = el("button", { className: "copy" }, "Copy URL");
    copyBtn.addEventListener("click", () => copy(themeUrl(name), copyBtn));

    const label = themeLabel(name);

    const size = el("span", { className: "tag size", title: "Width × Height for the OBS browser source" });

    const card = el("div", { className: "card" },
        view,
        el("div", { className: "row" },
            el("span", { className: "name" }, label),
            el("div", { className: "action" },
                star,
                el("a", { className: "open", href: themeUrl(name), target: "_blank", rel: "noopener" }, "Open"),
                copyBtn,
            ),
        ),
        el("div", { className: "tags" }, size),
    );
    card.dataset.name = name;

    observer.observe(view);
    return card;
}

// ====================================================
// Enlarged view: Click a preview to see that theme bigger
// ====================================================

const BIG_MAX_H = 520;  // tallest the enlarged preview gets, in pixels
let bigDialog = null;   // the open enlarged view, if any

function openBig(name, fromHistory = false) {
    const view = el("div", { className: "view" });
    view.dataset.theme = name;
    view.dataset.maxH = BIG_MAX_H;
    view.dataset.maxScale = 1;  // never larger than real size

    // favorites star, kept in step with the card's
    const star = el("button", { className: "fav" });
    star.dataset.name = name;
    styleStar(star, favorites.has(name));
    star.addEventListener("click", () => toggleFavorite(name));

    // Replay popup: only shown for popup themes, once the preview has loaded
    const replayBtn = el("button", { className: "chip", hidden: true }, "↻ Replay popup");
    view.addEventListener("preview-ready", e => {
        const win = e.detail.contentWindow;
        const isPopup = win.getComputedStyle(win.document.documentElement).getPropertyValue("--popup").trim() === "1";
        replayBtn.hidden = !isPopup;
        replayBtn.onclick = () =>  win.replayPopup?.();
    }, { once: true });

    // the URL to paste into OBS; click the box to select it all
    let scale = 1;
    const urlBox = el("input", { className: "big-url", type: "text", readOnly: true });
    urlBox.addEventListener("focus", () => urlBox.select());
    const openLink = el("a", { className: "open", target: "_blank", rel: "noopener" }, "Open");
    const copyBtn = el("button", { className: "copy" }, "Copy URL");
    copyBtn.addEventListener("click", () => copy(themeUrl(name, scale), copyBtn));

    // Scale: 1x, 1.5x, 2x. Updates the URL and the OBS size
    const scaleBtns = [1, 1.5, 2].map(s => {
        const b = el("button", { className: "chip" }, `${s}×`);
        b.addEventListener("click", () => setScale(s));
        return b;
    });
    function setScale(s) {
        scale = s;
        scaleBtns.forEach((b, i) => b.classList.toggle("on", [1, 1.5, 2][i] === s));
        urlBox.value = openLink.href = themeUrl(name, s);
        view.dataset.urlScale = s;
        layout(view);
    }

    // background swatches: a copy of the page's, sharing the same setting
    const swatches = document.querySelector(".bar .swatches").cloneNode(true);
    swatches.querySelectorAll(".sw").forEach(sw =>
        sw.addEventListener("click", () => setPreviewBg(sw.dataset.bg)));

    const closeBtn = el("button", { className: "close", title: "Close (Esc)", autofocus: true }, "×");
    const dialog = el("dialog", { className: "big" },
        el("div", { className: "big-body" },
            el("div", { className: "big-head" },
                el("h2", {}, themeLabel(name)),
                el("div", { className: "action" }, replayBtn, star, closeBtn),
            ),
            view,
            el("div", { className: "tags" },
                el("span", { className: "tag size", title: "Width × Height for the OBS browser source" })),
            el("div", { className: "big-foot" },
                urlBox,
                openLink,
                copyBtn,
                el("span", { className: "scale" }, ...scaleBtns),
                swatches,
            ),
        ),
    );

    closeBtn.addEventListener("click", () => dialog.close());
    // a click on the dimmed area around the box lands on the dialog itself
    dialog.addEventListener("click", e => { if (e.target === dialog) dialog.close(); });
    // closing (Esc, ×, outside) removes it, which also stops its preview
    dialog.addEventListener("close", () => {
        dialog.remove();
        if (bigDialog === dialog) bigDialog = null;
        // closed with (Esc, ×, outside or back): leave its history entry too, so Back doesn't reopen it
        if (history.state?.big) history.back();
    });

    document.body.appendChild(dialog);
    setScale(1);
    // give it its own history entry (gallery.html#name), so the Back button closes it
    if (!fromHistory) history.pushState({ big: name }, "", "#" + name);
    bigDialog = dialog;
    dialog.showModal();
    loadView(view);
}

// Back closes the enlarged view; Forward opens it again
window.addEventListener("popstate", () => {
    if (bigDialog) bigDialog.close();
    const name = history.state?.big;
    if (name && allThemeNames().includes(name)) openBig(name, true);
});

function allThemeNames() {
    return themes.flatMap(f => f.themes);
}

// The installed theme with this name: the exact spelling first, then any capitalization
function findThemeName(name) {
    const all = allThemeNames();
    const lower = name.toLowerCase();
    return all.find(n => n === name) || all.find(n => n.toLowerCase() === lower);
}

// Alphabetical, ignoring capitals (the same order as the tray menu)
function byName(a, b) {
    const x = a.toLowerCase(), y = b.toLowerCase();
    return x < y ? -1 : x > y ? 1 : 0;
}

// Opens the theme used in the address (gallery.html#name)
function openFromHash() {
    const name = findThemeName(location.hash.slice(1));
    if (!name) return;
    // drop the #name from this entry first, so closing the view stays on the gallery
    history.replaceState(null, "", location.pathname + location.search);
    openBig(name);
}

// The #name was changed while the gallery is open
window.addEventListener("hashchange", () => {
    if (history.state?.big) return;     // Back or Forward to an enlarged view: popstate handles it
    if (bigDialog) {
        // wait for the open view to finish closing, so it can't close the new one
        bigDialog.addEventListener("close", openFromHash, { once: true });
        bigDialog.close();
    } else {
        openFromHash();
    }
});

function searchQuery() {
    return document.getElementById("search").value.trim().toLowerCase();
}

// ======================================
// Tag Filters
// ======================================

const themeTags = new Map();    // theme name -> its tags, filled in by scanTags
const activeTags = new Set();   // the tag chips that are on
const tagBar = document.getElementById("tagFilters");

// Does this theme fit the search box and every selected tag?
function matches(name) {
    if (!name.toLowerCase().includes(searchQuery())) return false;
    const tags = themeTags.get(name) || [];
    return [...activeTags].every(t => tags.includes(t));
}

// Loads one theme off-screen and reads its tags. A theme that doesn't start in time gets none
function scanTheme(name) {
    return new Promise(resolve => {
        const frame = el("iframe", { className: "scan-frame", tabIndex: -1 });
        frame.setAttribute("aria-hidden", "true");
        frame.src = `overlay.html?theme=${encodeURIComponent(name)}&bg=transparent&preview=1`;
        const finish = () => {
            clearTimeout(timer);
            let tags = [];
            try { tags = readTags(frame); } catch {}
            frame.remove();
            resolve(tags);
        };
        const timer = setTimeout(finish, 4000);
        frame.addEventListener("load", () => {
            const win = frame.contentWindow;
            if (win.overlayReady) finish();
            else win.addEventListener("overlay-ready", finish, { once: true });
        }, { once: true });
        document.body.appendChild(frame);
    });
}

// One theme at a time, so the page stays responsive
async function scanTags() {
    const names = [...new Set(allThemeNames())];
    for (let i = 0; i < names.length; i++) {
        tagBar.replaceChildren(el("span", { className: "tag-status" },
            `Checking themes for filters: ${i + 1} of ${names.length}`));
        themeTags.set(names[i], await scanTheme(names[i]));
    }
    showTagChips();
}

function showTagChips() {
    const counts = new Map();
    for (const tags of themeTags.values()) {
        tags.forEach(t => counts.set(t, (counts.get(t) || 0) + 1));
    }

    const clear = el("button", { className: "chip", hidden: true }, "Clear");
    const chips = [...counts.keys()].sort().map(tag => {
        const chip = el("button", { className: "chip" }, tag, el("span", { className: "n" }, counts.get(tag)));
        chip.setAttribute("aria-pressed", "false");
        chip.addEventListener("click", () => {
            if (activeTags.has(tag)) activeTags.delete(tag); else activeTags.add(tag);
            chip.classList.toggle("on", activeTags.has(tag));
            chip.setAttribute("aria-pressed", activeTags.has(tag));
            clear.hidden = activeTags.size === 0;
            render();
        });
        return chip;
    });

    clear.addEventListener("click", () => {
        activeTags.clear();
        chips.forEach(chip => {
            chip.classList.remove("on");
            chip.setAttribute("aria-pressed", "false");
        });
        clear.hidden = true;
        render();
    });
    tagBar.replaceChildren(...chips, clear);
}

// The Favorites section at the top. Cards that are already there stay as they are,
// so adding or removing one doesn't reload the others.
function renderFavorites(rebuild = false) {
    const box = document.getElementById("favorites");
    const known = allThemeNames();
    const wanted = (filter === "all" || filter === "favorites") 
        ? [...favorites].filter(n => known.includes(n) && matches(n)).sort(byName)
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

    // add the new ones in alphabetical order
    wanted.forEach((name, i) => {
        const has = [...grid.children].some(card => card.dataset.name === name);
        if (!has) grid.insertBefore(makeCard(name), grid.children[i] || null);
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

    for (const folder of galleryOrder()) {
        const group = folder.name;
        if (filter !== "all" && filter !== group) continue;
        const names = folder.themes.filter(n => matches(n));
        if (names.length === 0 && (query || activeTags.size > 0)) continue;

        const title = document.createElement("h2");
        title.textContent = `${LABELS(group)} (${names.length})`;
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
document.querySelector(".filters").addEventListener("click", e => {
    const chip = e.target.closest(".chip[data-filter]");
    if (!chip) return;
    document.querySelectorAll(".chip[data-filter]").forEach(c => c.classList.remove("on"));
    chip.classList.add("on");
    filter = chip.dataset.filter;
    render();
});
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
    .then(data => { themes = data; addFolderChips(); })
    .then(loadFavorites)
    .then(() => {
        render();
        setTimeout(scanTags, 1500);     // after the first previews have had a chance to load
        openFromHash(); // gallery.html#name opens that theme
    })
    .catch(() => { document.getElementById("count").textContent = "Couldn't load the theme list. Is the server running?"; });