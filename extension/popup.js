document.addEventListener("DOMContentLoaded", async () => {
  const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
  const container = document.getElementById("list");

  if (!tab || !tab.id) {
    container.innerHTML = '<div class="empty">No active tab</div>';
    return;
  }

  chrome.runtime.sendMessage({ action: "getSniffed", tabId: tab.id }, (response) => {
    const items = response?.items || [];
    if (items.length === 0) {
      container.innerHTML = '<div class="empty">No streams detected on this page</div>';
      return;
    }

    container.innerHTML = "";
    items.forEach((item) => {
      const el = document.createElement("div");
      el.className = "stream-item";
      el.innerHTML = `
        <div class="stream-type">${item.type}</div>
        <div class="stream-url">${item.url}</div>
        <button data-url="${encodeURIComponent(item.url)}">Capture in Kestrel</button>
      `;
      el.querySelector("button").addEventListener("click", () => {
        chrome.runtime.sendMessage({ action: "enqueueStream", url: item.url }, (res) => {
          if (res?.success) {
            el.querySelector("button").textContent = "Enqueued!";
            el.querySelector("button").disabled = true;
          }
        });
      });
      container.appendChild(el);
    });
  });
});
