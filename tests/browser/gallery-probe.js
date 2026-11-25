// Runs at the end of gallery.html: waits for the tag scan, then records what the gallery shows
(async () => {
    const sleep = ms => new Promise(r => setTimeout(r, ms));
    const text = sel => [...document.querySelectorAll(sel)].map(e => e.textContent.trim().replace(/\s+/g, " "));
    const cards = sel => [...document.querySelectorAll(sel)].map(c => c.dataset.name).join(",");
    const errors = [];
    window.addEventListener("error", e => errors.push(e.message));

    // gallery.html#name: only record which theme opened
    if (location.hash) {
        await sleep(1500);
        await fetch("/__result", { method: "POST", body: JSON.stringify({ opened: document.querySelector("dialog.big h2")?.textContent || "(nothing)" }) });
        return;
    }

    for (let i = 0; i < 600 && !document.querySelector("#tagFilters .chip"); i++) await sleep(100);
    const r = {
        folderChips: text(".filters .chip").join(" | "),
        tagChips: text("#tagFilters .chip").join(" | "),
        count: document.getElementById("count").textContent,
        favorites: cards("#favorites .card"),
        sections: text("#sections h2").join(" | "),
        cardOrder: cards("#sections .card"),
    };
    const search = async q => {
        const s = document.getElementById("search");
        s.value = q;
        s.dispatchEvent(new Event("input"));
        await sleep(500);
        return document.querySelectorAll("#sections .card").length;
    };
    r.searchNeon = await search("neon");
    await search("");
    document.querySelector("#tagFilters .chip")?.click();
    await sleep(300);
    r.firstTagFilter = document.getElementById("count").textContent;
    r.errors = errors.join("; ");
    await fetch("/__result", { method: "POST", body: JSON.stringify(r) });
})();
