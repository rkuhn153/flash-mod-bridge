// Keep service worker alive lightly; bridge UI opens hub status.
chrome.runtime.onInstalled.addListener(() => {
  console.info("[FlashModBridge] installed");
});
