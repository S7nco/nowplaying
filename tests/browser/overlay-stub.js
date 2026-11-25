// Runs before script.js in every overlay the tests load: no live song (the probe sends its own),
// no WebSocket, and script errors are remembered so the probe can report them
window.WebSocket = class { constructor() {} close() {} };
const realFetch = window.fetch;
window.fetch = (url, options) => String(url).includes("/nowplaying") ? new Promise(() => {}) : realFetch(url, options);
window.addEventListener("error", e => { window.__probeErrors = (window.__probeErrors || "") + e.message + "; "; });
