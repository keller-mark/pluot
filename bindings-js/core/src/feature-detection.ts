import UserAgent from "lightua";

// Returns the tuple [supportsWebGpu, supportsWebGpuMessage]
export function checkWebGpuFeatureDetection(): [boolean, string|null] {
  const isSupported = typeof navigator !== 'undefined' && navigator.gpu;
  if (isSupported) {
    return [true, null];
  }
  const ua = UserAgent.parse(navigator.userAgent);
  // WebGPU is supported in the following cases:
  // - Browser Engine: Chromium. Major: 113+. OS: mac, windows, chromeOS
  // - Browser Engine: Chromium. Major: 121+. OS: android
  // - Browser Engine: Chromium. Major: 144+. OS: linux
  // - Browser: Firefox. Major: 141+. OS: windows
  // - Browser: Firefox. Major: 145+. OS: macos
  // - Browser: Safari. Major: 26+. OS: macOS. OS version: tahoe 26+.
  // - Browser: Safari. Major: 26+. OS: iOS/iPadOS. OS version: tahoe 26+.
  // Else: suggest upgrading browser version or OS version (if different version of same browser has support)
  // Else: suggest using different browser version.
  const browserName = ua.browser.name;
  const browserMajor = ua.browser.major;
  const osName = ua.os.name;
  const osVersion = ua.os.version;

  // Should be supported according to UA but support wasn't detected.
  const shouldSupportResult: [boolean, string|null] = [false, `WebGPU support was not detected. Please ensure the WebGPU feature is enabled or try a different browser.`];

  if(["Chrome", "Chromium"].includes(browserName)) {
    if(["Windows", "macOS", "ChromeOS"].includes(osName)) {
      if(browserMajor < 113) {
        return [false, `WebGPU is not supported in your web browser. If using Chrome or a Chromium-based browser on Windows/macOS/ChromeOS, WebGPU is supported in browser versions 113 and above.`];
      }
    } else if(osName === "Android") {
      if(browserMajor < 121) {
        return [false, `WebGPU is not supported in your web browser. If using Chrome or a Chromium-based browser on Android, WebGPU is supported in browser versions 121 and above.`];
      }
    } else if(osName === "Linux") {
      if(browserMajor < 144) {
        return [false, `WebGPU is not supported in your web browser. If using Chrome or a Chromium-based browser on Linux, WebGPU is supported in browser versions 144 and above.`];
      }
    }
  } else if(browserName === "Firefox") {
    if(osName === "Windows") {
      if(browserMajor < 141) {
        return [false, `WebGPU is not supported in your web browser. If using Firefox on Windows, WebGPU is supported in browser versions 141 and above.`];
      }
    }
  } else if(browserName === "Safari") {
    if(["macOS", "iOS"].includes(osName)) {
      const osVersionFloat = parseFloat(osVersion);
      if(browserMajor < 26 || osVersionFloat < 26) {
        return [false, `WebGPU is not supported in your web browser. If using Safari, WebGPU is supported in Safari version 26 and above on macOS 26 (Tahoe) and above.`];
      }
    }
  }
  return shouldSupportResult;
}
