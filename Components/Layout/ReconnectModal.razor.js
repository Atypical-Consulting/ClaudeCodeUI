// Set up event handlers
const reconnectModal = document.getElementById("components-reconnect-modal");
reconnectModal.addEventListener("components-reconnect-state-changed", handleReconnectStateChanged);
// Escape would close the dialog over a dead UI: the modal only goes away when the circuit is back.
reconnectModal.addEventListener("cancel", e => e.preventDefault());

const retryButton = document.getElementById("components-reconnect-button");
retryButton.addEventListener("click", retry);

const resumeButton = document.getElementById("components-resume-button");
resumeButton.addEventListener("click", resume);

document.getElementById("components-reload-button").addEventListener("click", () => location.reload());

function handleReconnectStateChanged(event) {
    const state = event.detail.state;
    if (state === "hide") {
        reconnectModal.close();
        return;
    }
    if (state === "rejected") {
        location.reload();
        return;
    }
    // "paused" can arrive without a prior "show" (the server paused the circuit), so open on every visible state.
    if (!reconnectModal.open) reconnectModal.showModal();
    if (state === "failed") document.addEventListener("visibilitychange", retryWhenDocumentBecomesVisible);
    if (state !== "retrying") focusAction();
}

// Keyboard focus goes to the state's action (Retry / Resume), else to the title so the dialog is announced.
function focusAction() {
    const action = [retryButton, resumeButton].find(b => b.checkVisibility());
    (action ?? document.getElementById("rc-title")).focus();
}

async function retry() {
    document.removeEventListener("visibilitychange", retryWhenDocumentBecomesVisible);

    try {
        // Reconnect will asynchronously return:
        // - true to mean success
        // - false to mean we reached the server, but it rejected the connection (e.g., unknown circuit ID)
        // - exception to mean we didn't reach the server (this can be sync or async)
        const successful = await Blazor.reconnect();
        if (!successful) {
            // We have been able to reach the server, but the circuit is no longer available.
            // We'll reload the page so the user can continue using the app as quickly as possible.
            const resumeSuccessful = await Blazor.resumeCircuit();
            if (!resumeSuccessful) {
                location.reload();
            } else {
                reconnectModal.close();
            }
        }
    } catch (err) {
        // We got an exception, server is currently unavailable
        document.addEventListener("visibilitychange", retryWhenDocumentBecomesVisible);
    }
}

async function resume() {
    try {
        const successful = await Blazor.resumeCircuit();
        if (!successful) {
            location.reload();
        }
    } catch {
        reconnectModal.classList.replace("components-reconnect-paused", "components-reconnect-resume-failed");
        focusAction();
    }
}

async function retryWhenDocumentBecomesVisible() {
    if (document.visibilityState === "visible") {
        await retry();
    }
}
