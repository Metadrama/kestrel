const NATIVE_HOST = "io.github.metadrama.kestrel";
const SIZE_THRESHOLD_BYTES = 2 * 1024 * 1024; // 2 MB
const INTERCEPT_EXTENSIONS = [
  "zip", "tar", "gz", "7z", "rar", "iso", "bin", "apk", "exe", "msi", "dmg",
  "mp4", "mkv", "webm", "mov", "avi", "flv", "mp3", "flac", "wav"
];

let nativePort = null;
const sniffedMedia = new Map(); // tabId -> Array of stream items

function getNativePort() {
  if (!nativePort) {
    try {
      nativePort = chrome.runtime.connectNative(NATIVE_HOST);
      nativePort.onDisconnect.addListener(() => {
        nativePort = null;
      });
    } catch (e) {
      console.warn("Kestrel native host unavailable:", e);
    }
  }
  return nativePort;
}

// Download interception
chrome.downloads.onCreated.addListener(async (downloadItem) => {
  const url = downloadItem.finalUrl || downloadItem.url;
  if (!url || url.startsWith("data:") || url.startsWith("blob:")) {
    return;
  }

  const filename = downloadItem.filename || "";
  const ext = filename.split(".").pop()?.toLowerCase() || "";
  const isTargetExt = INTERCEPT_EXTENSIONS.includes(ext);
  const isLarge = (downloadItem.fileSize || 0) >= SIZE_THRESHOLD_BYTES;

  if (isTargetExt || isLarge) {
    chrome.downloads.cancel(downloadItem.id, () => {
      chrome.downloads.erase({ id: downloadItem.id });
    });

    const cookies = await chrome.cookies.getAll({ url });
    const cookieHeader = cookies.map(c => `${c.name}=${c.value}`).join("; ");

    const headers = [
      ["User-Agent", navigator.userAgent],
      ["Referer", downloadItem.referrer || ""]
    ];
    if (cookieHeader) {
      headers.push(["Cookie", cookieHeader]);
    }

    const payload = {
      action: "enqueue",
      url: url,
      filename: filename,
      headers: headers
    };

    const port = getNativePort();
    if (port) {
      port.postMessage(payload);
    }
  }
});

// Passive stream sniffer for HLS, DASH, and raw video
chrome.webRequest.onHeadersReceived.addListener(
  (details) => {
    const url = details.url;
    const tabId = details.tabId;
    if (tabId < 0) return;

    let isStream = false;
    let type = "video";

    if (url.includes(".m3u8")) {
      isStream = true;
      type = "HLS Stream (.m3u8)";
    } else if (url.includes(".mpd")) {
      isStream = true;
      type = "DASH Stream (.mpd)";
    } else {
      const ctHeader = details.responseHeaders?.find(h => h.name.toLowerCase() === "content-type");
      const ct = ctHeader?.value?.toLowerCase() || "";
      if (ct.includes("video/mp4") || ct.includes("video/webm")) {
        isStream = true;
        type = "Direct Video";
      }
    }

    if (isStream) {
      if (!sniffedMedia.has(tabId)) {
        sniffedMedia.set(tabId, []);
      }
      const list = sniffedMedia.get(tabId);
      if (!list.some(item => item.url === url)) {
        list.push({ url, type, time: Date.now() });
        chrome.action.setBadgeText({ tabId, text: String(list.length) });
        chrome.action.setBadgeBackgroundColor({ tabId, color: "#7CB4D8" });
      }
    }
  },
  { urls: ["<all_urls>"] },
  ["responseHeaders"]
);

chrome.runtime.onMessage.addListener((request, sender, sendResponse) => {
  if (request.action === "getSniffed") {
    const tabId = request.tabId;
    sendResponse({ items: sniffedMedia.get(tabId) || [] });
  } else if (request.action === "enqueueStream") {
    const port = getNativePort();
    if (port) {
      port.postMessage({
        action: "enqueue",
        url: request.url,
        headers: [["User-Agent", navigator.userAgent]]
      });
      sendResponse({ success: true });
    } else {
      sendResponse({ success: false, error: "Native host not reachable" });
    }
  }
  return true;
});
