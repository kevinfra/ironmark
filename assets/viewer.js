(() => {
  "use strict";

  const contentEl = document.getElementById("content");
  const emptyStateEl = document.getElementById("empty-state");
  const errorStateEl = document.getElementById("error-state");
  const errorMsgEl = document.getElementById("error-message");
  const fileLabelEl = document.getElementById("file-label");
  const blockedBadgeEl = document.getElementById("blocked-badge");
  const btnOpen = document.getElementById("btn-open");
  const btnOpenEmpty = document.getElementById("btn-open-empty");
  const btnRemote = document.getElementById("btn-remote");
  const remoteIconEl = document.getElementById("remote-icon");
  const remoteLabelEl = document.getElementById("remote-label");
  const btnTheme = document.getElementById("btn-theme");
  const themeLabelEl = document.getElementById("theme-label");
  const btnFind = document.getElementById("btn-find");
  const btnZoomOut = document.getElementById("btn-zoom-out");
  const btnZoomReset = document.getElementById("btn-zoom-reset");
  const btnZoomIn = document.getElementById("btn-zoom-in");
  const dropOverlayEl = document.getElementById("drop-overlay");

  // Find bar elements
  const findBarEl = document.getElementById("find-bar");
  const findInputEl = document.getElementById("find-input");
  const findCountEl = document.getElementById("find-count");
  const findPrevBtn = document.getElementById("find-prev");
  const findNextBtn = document.getElementById("find-next");
  const findCloseBtn = document.getElementById("find-close");

  // Mermaid modal elements
  const modalEl = document.getElementById("mermaid-modal");
  const modalStageEl = document.getElementById("mermaid-modal-stage");
  const modalZoomOut = document.getElementById("modal-zoom-out");
  const modalZoomReset = document.getElementById("modal-zoom-reset");
  const modalZoomIn = document.getElementById("modal-zoom-in");
  const modalClose = document.getElementById("modal-close");

  const darkMedia = window.matchMedia("(prefers-color-scheme: dark)");

  let currentFilePath = "";
  let themeMode = localStorage.getItem("ironmark.theme") || "auto";
  let zoomLevel = parseFloat(localStorage.getItem("ironmark.zoom") || "1");
  if (!Number.isFinite(zoomLevel) || zoomLevel < 0.6 || zoomLevel > 2.2) {
    zoomLevel = 1;
  }

  let modalScale = 1;
  let mermaidRenderId = 0;
  let mermaidSources = []; // { hostEl, source }
  let findMatches = [];
  let findIndex = -1;

  function sendIpc(payload) {
    if (window.ipc && typeof window.ipc.postMessage === "function") {
      window.ipc.postMessage(JSON.stringify(payload));
    }
  }

  // --- Theme handling ---
  function isDarkActive() {
    if (themeMode === "dark") return true;
    if (themeMode === "light") return false;
    return darkMedia.matches;
  }

  function applyTheme(rerenderDiagrams) {
    if (themeMode === "auto") {
      document.documentElement.removeAttribute("data-theme");
      themeLabelEl.textContent = "Auto";
    } else if (themeMode === "dark") {
      document.documentElement.setAttribute("data-theme", "dark");
      themeLabelEl.textContent = "Dark";
    } else {
      document.documentElement.setAttribute("data-theme", "light");
      themeLabelEl.textContent = "Light";
    }
    localStorage.setItem("ironmark.theme", themeMode);
    if (rerenderDiagrams) {
      renderAllMermaid();
    }
  }

  function cycleTheme() {
    if (themeMode === "auto") themeMode = "light";
    else if (themeMode === "light") themeMode = "dark";
    else themeMode = "auto";
    applyTheme(true);
  }

  darkMedia.addEventListener("change", () => {
    if (themeMode === "auto") {
      renderAllMermaid();
    }
  });

  // --- Zoom handling ---
  function applyZoom() {
    zoomLevel = Math.round(Math.min(2.2, Math.max(0.6, zoomLevel)) * 100) / 100;
    document.documentElement.style.setProperty("--doc-zoom", String(zoomLevel));
    btnZoomReset.textContent = `${Math.round(zoomLevel * 100)}%`;
    localStorage.setItem("ironmark.zoom", String(zoomLevel));
  }

  // --- Mermaid rendering (strict offline) ---
  async function renderAllMermaid() {
    if (typeof window.mermaid === "undefined" || mermaidSources.length === 0) {
      return;
    }

    const theme = isDarkActive() ? "dark" : "default";
    window.mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme,
      flowchart: { htmlLabels: false },
      fontFamily: "var(--font-sans)",
    });

    for (const entry of mermaidSources) {
      const { hostEl, source } = entry;
      if (!hostEl.isConnected) continue;

      const id = `mdv-mmd-${++mermaidRenderId}`;
      try {
        const result = await window.mermaid.render(id, source);
        const wrapper = document.createElement("div");
        wrapper.className = "mermaid-rendered";
        wrapper.title = "Click to expand diagram";
        wrapper.innerHTML = result.svg;
        wrapper.addEventListener("click", () => openMermaidModal(result.svg));
        hostEl.replaceChildren(wrapper);
      } catch (err) {
        // Clean up any temporary error element Mermaid might have injected into document.body
        const stray = document.getElementById(`d${id}`);
        if (stray) stray.remove();

        const errBox = document.createElement("div");
        errBox.className = "mermaid-error";
        errBox.textContent = `Mermaid syntax error:\n${err && err.message ? err.message : String(err)}`;
        hostEl.replaceChildren(errBox);
      }
    }
  }

  function openMermaidModal(svgMarkup) {
    modalScale = 1;
    modalStageEl.innerHTML = svgMarkup;
    const svg = modalStageEl.querySelector("svg");
    if (svg) {
      svg.style.maxWidth = "none";
      svg.style.width = "auto";
      svg.style.height = "auto";
    }
    updateModalZoom();
    modalEl.classList.remove("hidden");
  }

  function closeMermaidModal() {
    modalEl.classList.add("hidden");
    modalStageEl.replaceChildren();
  }

  function updateModalZoom() {
    modalScale = Math.round(Math.min(4, Math.max(0.4, modalScale)) * 100) / 100;
    modalZoomReset.textContent = `${Math.round(modalScale * 100)}%`;
    const svg = modalStageEl.querySelector("svg");
    if (svg) {
      svg.style.transform = `scale(${modalScale})`;
    }
  }

  // --- Code copy buttons ---
  function enhanceCodeBlocks() {
    const pres = contentEl.querySelectorAll("pre");
    for (const pre of pres) {
      if (pre.classList.contains("mermaid")) continue;
      const code = pre.querySelector("code");
      if (!code) continue;

      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "code-copy-btn";
      btn.textContent = "Copy";
      btn.addEventListener("click", async () => {
        try {
          await navigator.clipboard.writeText(code.textContent || "");
          btn.textContent = "Copied";
          setTimeout(() => {
            btn.textContent = "Copy";
          }, 1200);
        } catch {
          btn.textContent = "Failed";
          setTimeout(() => {
            btn.textContent = "Copy";
          }, 1200);
        }
      });
      pre.appendChild(btn);
    }
  }

  // --- Remote image button state ---
  function updateRemoteButton(allowRemote, blockedCount) {
    if (allowRemote) {
      btnRemote.classList.add("remote-on");
      remoteIconEl.textContent = "🌐";
      remoteLabelEl.textContent = "Remote: On";
      btnRemote.title = "Remote images allowed — click to switch back to strict offline mode";
    } else {
      btnRemote.classList.remove("remote-on");
      remoteIconEl.textContent = "🔒";
      remoteLabelEl.textContent = "Offline";
      btnRemote.title = "Strict offline mode (remote images blocked) — click to allow remote images";
    }

    if (!allowRemote && blockedCount > 0) {
      blockedBadgeEl.textContent =
        blockedCount === 1 ? "1 remote img blocked" : `${blockedCount} remote imgs blocked`;
      blockedBadgeEl.classList.remove("hidden");
    } else {
      blockedBadgeEl.classList.add("hidden");
    }
  }

  // --- Document loading (zero-flash live reload) ---
  async function loadDocument(preserveScroll) {
    const prevScroll = window.scrollY;
    let doc;
    try {
      const resp = await fetch("/api/document", { cache: "no-store" });
      if (!resp.ok) return;
      doc = await resp.json();
    } catch {
      return;
    }

    clearFindHighlights();
    updateRemoteButton(Boolean(doc.allow_remote_images), Number(doc.blocked_remote_images || 0));

    if (doc.error) {
      contentEl.classList.add("hidden");
      emptyStateEl.classList.add("hidden");
      errorStateEl.classList.remove("hidden");
      errorMsgEl.textContent = doc.error;
      fileLabelEl.textContent = doc.file_name || "Error";
      fileLabelEl.title = doc.file_path || "";
      return;
    }

    if (!doc.has_file) {
      contentEl.classList.add("hidden");
      errorStateEl.classList.add("hidden");
      emptyStateEl.classList.remove("hidden");
      fileLabelEl.textContent = "No file open";
      fileLabelEl.title = "";
      currentFilePath = "";
      mermaidSources = [];
      return;
    }

    const sameFile = currentFilePath === doc.file_path;
    currentFilePath = doc.file_path;

    emptyStateEl.classList.add("hidden");
    errorStateEl.classList.add("hidden");
    contentEl.classList.remove("hidden");

    fileLabelEl.textContent = doc.file_name;
    fileLabelEl.title = doc.file_path;
    document.title = `${doc.file_name} — ironmark`;

    // HTML is already strictly sanitized by ammonia in Rust + enforced by HTTP CSP.
    contentEl.innerHTML = doc.html;

    // Collect Mermaid blocks before rendering so theme toggles can re-render them cleanly.
    mermaidSources = [];
    const blocks = contentEl.querySelectorAll(".mermaid-block");
    for (const block of blocks) {
      const pre = block.querySelector("pre.mermaid");
      if (!pre) continue;
      const source = pre.textContent || "";
      mermaidSources.push({ hostEl: block, source });
    }

    enhanceCodeBlocks();
    await renderAllMermaid();

    // Restore scroll if reloading the same file, or after a CSP reload via sessionStorage.
    const savedScroll = sessionStorage.getItem("ironmark.restoreScroll");
    if (savedScroll !== null) {
      sessionStorage.removeItem("ironmark.restoreScroll");
      window.scrollTo(0, Number(savedScroll) || 0);
    } else if (preserveScroll && sameFile) {
      window.scrollTo(0, prevScroll);
    } else {
      window.scrollTo(0, 0);
    }

    // Re-apply active find query if find bar is open.
    if (!findBarEl.classList.contains("hidden") && findInputEl.value) {
      runFind(findInputEl.value, false);
    }
  }

  // --- Lightweight in-page find ---
  function clearFindHighlights() {
    for (const mark of findMatches) {
      const parent = mark.parentNode;
      if (!parent) continue;
      parent.replaceChild(document.createTextNode(mark.textContent || ""), mark);
      parent.normalize();
    }
    findMatches = [];
    findIndex = -1;
    findCountEl.textContent = "";
  }

  function runFind(query, scrollToFirst) {
    clearFindHighlights();
    const needle = query.trim().toLowerCase();
    if (!needle) return;

    const walker = document.createTreeWalker(contentEl, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        if (parent.closest(".mermaid-block, .code-copy-btn, script, style")) {
          return NodeFilter.FILTER_REJECT;
        }
        return NodeFilter.FILTER_ACCEPT;
      },
    });

    const textNodes = [];
    let current;
    while ((current = walker.nextNode())) {
      textNodes.push(current);
    }

    for (const node of textNodes) {
      const text = node.nodeValue;
      const lower = text.toLowerCase();
      let pos = lower.indexOf(needle);
      if (pos === -1) continue;

      const frag = document.createDocumentFragment();
      let lastIdx = 0;
      while (pos !== -1) {
        if (pos > lastIdx) {
          frag.appendChild(document.createTextNode(text.slice(lastIdx, pos)));
        }
        const mark = document.createElement("mark");
        mark.className = "find-hit";
        mark.textContent = text.slice(pos, pos + needle.length);
        frag.appendChild(mark);
        findMatches.push(mark);

        lastIdx = pos + needle.length;
        pos = lower.indexOf(needle, lastIdx);
      }
      if (lastIdx < text.length) {
        frag.appendChild(document.createTextNode(text.slice(lastIdx)));
      }
      node.parentNode.replaceChild(frag, node);
    }

    if (findMatches.length === 0) {
      findCountEl.textContent = "0 / 0";
    } else {
      findIndex = 0;
      activateFindMatch(scrollToFirst);
    }
  }

  function activateFindMatch(scroll) {
    for (const m of findMatches) m.classList.remove("active");
    if (findIndex < 0 || findIndex >= findMatches.length) return;
    const active = findMatches[findIndex];
    active.classList.add("active");
    findCountEl.textContent = `${findIndex + 1} / ${findMatches.length}`;
    if (scroll) {
      active.scrollIntoView({ behavior: "smooth", block: "center" });
    }
  }

  function stepFind(delta) {
    if (findMatches.length === 0) return;
    findIndex = (findIndex + delta + findMatches.length) % findMatches.length;
    activateFindMatch(true);
  }

  function openFindBar() {
    findBarEl.classList.remove("hidden");
    findInputEl.focus();
    findInputEl.select();
  }

  function closeFindBar() {
    findBarEl.classList.add("hidden");
    clearFindHighlights();
  }

  // --- Safe link click interception (HTML <a> and SVG <a xlink:href>, including Mermaid modal) ---
  document.addEventListener(
    "click",
    (ev) => {
      const target = ev.target;
      if (!(target instanceof Element)) return;
      const anchor = target.closest("a");
      if (!anchor) return;
      if (!contentEl.contains(anchor) && !modalStageEl.contains(anchor)) return;

      const href = (
        anchor.getAttribute("href") ||
        anchor.getAttributeNS("http://www.w3.org/1999/xlink", "href") ||
        ""
      ).trim();

      ev.preventDefault();
      ev.stopPropagation();

      if (!href) return;

      // Internal heading anchor
      if (href.startsWith("#")) {
        if (!modalEl.classList.contains("hidden")) {
          closeMermaidModal();
        }
        const targetId = decodeURIComponent(href.slice(1));
        const targetEl = document.getElementById(targetId);
        if (targetEl) {
          targetEl.scrollIntoView({ behavior: "smooth", block: "start" });
        }
        return;
      }

      const lower = href.toLowerCase();
      if (lower.startsWith("http://") || lower.startsWith("https://") || lower.startsWith("mailto:")) {
        sendIpc({ type: "open_external", url: href });
      } else if (!lower.includes(":")) {
        // Relative link to another local Markdown file
        sendIpc({ type: "open_relative", href });
      }
    },
    true
  );

  // --- Controls & Keyboard shortcuts ---
  btnOpen.addEventListener("click", () => sendIpc({ type: "open_dialog" }));
  btnOpenEmpty.addEventListener("click", () => sendIpc({ type: "open_dialog" }));
  btnRemote.addEventListener("click", () => {
    sessionStorage.setItem("ironmark.restoreScroll", String(window.scrollY));
    sendIpc({ type: "toggle_remote_images" });
  });
  btnTheme.addEventListener("click", cycleTheme);
  btnFind.addEventListener("click", openFindBar);
  btnZoomOut.addEventListener("click", () => {
    zoomLevel -= 0.1;
    applyZoom();
  });
  btnZoomReset.addEventListener("click", () => {
    zoomLevel = 1;
    applyZoom();
  });
  btnZoomIn.addEventListener("click", () => {
    zoomLevel += 0.1;
    applyZoom();
  });

  findInputEl.addEventListener("input", () => runFind(findInputEl.value, true));
  findInputEl.addEventListener("keydown", (ev) => {
    if (ev.key === "Enter") {
      ev.preventDefault();
      stepFind(ev.shiftKey ? -1 : 1);
    } else if (ev.key === "Escape") {
      closeFindBar();
    }
  });
  findPrevBtn.addEventListener("click", () => stepFind(-1));
  findNextBtn.addEventListener("click", () => stepFind(1));
  findCloseBtn.addEventListener("click", closeFindBar);

  modalClose.addEventListener("click", closeMermaidModal);
  modalZoomOut.addEventListener("click", () => {
    modalScale -= 0.2;
    updateModalZoom();
  });
  modalZoomReset.addEventListener("click", () => {
    modalScale = 1;
    updateModalZoom();
  });
  modalZoomIn.addEventListener("click", () => {
    modalScale += 0.2;
    updateModalZoom();
  });
  modalEl.addEventListener("click", (ev) => {
    if (ev.target === modalStageEl || ev.target === modalEl) {
      closeMermaidModal();
    }
  });

  document.addEventListener("keydown", (ev) => {
    const mod = ev.metaKey || ev.ctrlKey;
    if (mod && ev.key.toLowerCase() === "o") {
      ev.preventDefault();
      sendIpc({ type: "open_dialog" });
    } else if (mod && ev.key.toLowerCase() === "f") {
      ev.preventDefault();
      openFindBar();
    } else if (mod && ev.key.toLowerCase() === "r") {
      ev.preventDefault();
      sendIpc({ type: "reload" });
    } else if (mod && (ev.key === "=" || ev.key === "+")) {
      ev.preventDefault();
      zoomLevel += 0.1;
      applyZoom();
    } else if (mod && ev.key === "-") {
      ev.preventDefault();
      zoomLevel -= 0.1;
      applyZoom();
    } else if (mod && ev.key === "0") {
      ev.preventDefault();
      zoomLevel = 1;
      applyZoom();
    } else if (mod && ev.key.toLowerCase() === "q") {
      ev.preventDefault();
      sendIpc({ type: "quit" });
    } else if (ev.key === "Escape") {
      if (!modalEl.classList.contains("hidden")) {
        closeMermaidModal();
      } else if (!findBarEl.classList.contains("hidden")) {
        closeFindBar();
      }
    }
  });

  // Prevent browser default file-drop navigation; native wry drag-drop handler opens the file.
  window.addEventListener("dragover", (ev) => ev.preventDefault());
  window.addEventListener("drop", (ev) => ev.preventDefault());

  // Hooks called by the Rust backend via WebView::evaluate_script.
  window.__ironmarkReload = (preserveScroll = true) => {
    loadDocument(Boolean(preserveScroll));
  };
  window.__ironmarkHardReload = () => {
    window.location.reload();
  };
  window.__ironmarkSetDragOver = (active) => {
    dropOverlayEl.classList.toggle("hidden", !active);
  };

  // Boot
  applyTheme(false);
  applyZoom();
  loadDocument(false);
})();
