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
    document.querySelector("#tagFilters .chip")?.click();
    await sleep(300);

    // Copy URL clicked twice quickly must go back to "Copy URL" (0.3.1 stayed on "Copied!")
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: { writeText: async () => {} } });
    const copyButton = document.querySelector("#sections .copy");
    copyButton.click();
    await sleep(100);
    copyButton.click();
    await sleep(2500);
    r.copyLabelAfterTwoClicks = copyButton.textContent;

    // Another device over plain http has no clipboard API: the copy has to happen inside the enlarged view,
    // because the page behind it can't take focus, and a copy that fails mustn't say "Copied!"
    Object.defineProperty(navigator, "clipboard", { configurable: true, value: undefined });
    let focused = "(no copy attempted)";
    document.execCommand = () => {
        const e = document.activeElement;
        focused = (e ? e.tagName : "nothing") + (e && e.closest("dialog") ? " in the view" : " behind the view");
        return false;   // as if the browser refused
    };
    document.querySelector("#sections .view").click();
    await sleep(500);
    const bigCopy = document.querySelector("dialog.big .copy");
    bigCopy.click();
    await sleep(100);
    r.copyFocusWithoutClipboard = focused;
    r.copyLabelWhenItFails = bigCopy.textContent;
    r.errors = errors.join("; ");
    await fetch("/__result", { method: "POST", body: JSON.stringify(r) });
})();
