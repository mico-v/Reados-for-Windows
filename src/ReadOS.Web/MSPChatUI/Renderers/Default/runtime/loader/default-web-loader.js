(function () {
  const currentScript = document.currentScript;
  const manifestPath = currentScript?.dataset?.manifest || "../../renderer.manifest.json";
  const autostart = currentScript?.dataset?.autostart !== "false";

  function resolveURL(path, baseURL) {
    return new URL(path, baseURL).href;
  }

  async function readJSON(url) {
    const response = await fetch(url, { cache: "no-cache" });
    if (!response.ok) {
      throw new Error(`Failed to load ${url}: HTTP ${response.status}`);
    }
    return response.json();
  }

  function appendStyle(href, options = {}) {
    const link = document.createElement("link");
    link.rel = "stylesheet";
    link.href = href;
    if (options.id) {
      link.id = options.id;
    }
    if (options.disabled) {
      link.disabled = true;
    }
    document.head.appendChild(link);
    return link;
  }

  function appendScript(src) {
    return new Promise((resolve, reject) => {
      const script = document.createElement("script");
      script.src = src;
      script.async = false;
      script.onload = () => resolve(script);
      script.onerror = () => reject(new Error(`Failed to load script ${src}`));
      document.head.appendChild(script);
    });
  }

  async function appendScripts(paths, baseURL) {
    for (const path of paths) {
      await appendScript(resolveURL(path, baseURL));
    }
  }

  function installSystemSymbolCatalog() {
    if (window.__chatTranscriptSystemSymbols && typeof window.__chatTranscriptSystemSymbols === "object") {
      return;
    }
    window.__chatTranscriptSystemSymbols = {};
  }

  function mathAsset(name) {
    return `Math/${name}`;
  }

  function installStyles(assetBaseURL, sourceManifest) {
    appendStyle(resolveURL(mathAsset(sourceManifest.katexStyleSheetName), assetBaseURL));
    appendStyle(resolveURL(mathAsset(sourceManifest.highlightThemeStyleSheetNames.light), assetBaseURL), {
      id: "highlight-theme-light"
    });
    appendStyle(resolveURL(mathAsset(sourceManifest.highlightThemeStyleSheetNames.dark), assetBaseURL), {
      id: "highlight-theme-dark",
      disabled: true
    });
    appendStyle(resolveURL(mathAsset(sourceManifest.documentStyleSheetName), assetBaseURL));
    (sourceManifest.additionalDocumentStyleSheetNames || []).forEach((name) => {
      appendStyle(resolveURL(mathAsset(name), assetBaseURL));
    });
  }

  function publishManifest(manifest, sourceManifest, manifestURL, assetBaseURL) {
    window.MSPChatUIDefaultManifest = Object.freeze({
      manifest,
      sourceManifest,
      manifestURL,
      assetBaseURL
    });
  }

  async function loadDefaultRenderer() {
    const manifestURL = resolveURL(manifestPath, document.baseURI);
    const manifestBaseURL = resolveURL(".", manifestURL);
    const manifest = await readJSON(manifestURL);
    const assetBaseURL = resolveURL(`${manifest.assetBasePath.replace(/\/?$/, "/")}`, manifestBaseURL);
    const sourceManifest = await readJSON(resolveURL(manifest.sourceAssetManifestPath, manifestBaseURL));

    publishManifest(manifest, sourceManifest, manifestURL, assetBaseURL);
    installStyles(assetBaseURL, sourceManifest);

    await appendScripts(manifest.contractScripts || [], manifestBaseURL);
    await appendScripts(manifest.projectionScripts || [], manifestBaseURL);
    await appendScripts(manifest.rendererAPIScripts || [], manifestBaseURL);
    await appendScripts(manifest.hostCompatibilityScripts || [], manifestBaseURL);
    await appendScripts(manifest.knowledgeMapScriptPaths || [], assetBaseURL);

    const markdownScripts = [];
    if (sourceManifest.highlightScriptName) {
      markdownScripts.push(mathAsset(sourceManifest.highlightScriptName));
    }
    markdownScripts.push(...(sourceManifest.markdownDependencyScriptNames || []).map(mathAsset));
    await appendScripts(markdownScripts, assetBaseURL);

    // These manifest entries are native-host evaluation snippets, not
    // standalone browser scripts: symbolCatalogBootstrapScriptName expects a
    // host-substituted payload, while hostCommandInvocationScriptName and
    // bootstrapProbeScriptName contain a top-level return. The browser host
    // supplies an empty catalog directly and must never append those snippets
    // as <script src> resources.
    installSystemSymbolCatalog();
    const runtimeScripts = [];
    runtimeScripts.push(...(sourceManifest.transcriptRuntimeScriptNames || []).map(mathAsset));
    if (sourceManifest.compatibilityExportBridgeScriptName) {
      runtimeScripts.push(mathAsset(sourceManifest.compatibilityExportBridgeScriptName));
    }
    await appendScripts(runtimeScripts, assetBaseURL);

    // selectionRepairPayloadScriptName is also a native evaluateJavaScript
    // function body. Only the real browser user script is loaded here.
    const userScripts = [sourceManifest.selectionContextMenuUserScriptName]
      .filter(Boolean)
      .map(mathAsset);
    await appendScripts(userScripts, assetBaseURL);

    window.dispatchEvent(new CustomEvent("msp-chat-ui-default-ready", {
      detail: { manifest, sourceManifest, manifestURL, assetBaseURL }
    }));
    return window.MSPChatUIDefaultManifest;
  }

  window.MSPChatUIDefaultLoader = Object.freeze({
    loadDefaultRenderer
  });

  if (autostart) {
    loadDefaultRenderer().catch((error) => {
      window.__mspChatUIDefaultLoaderError = error instanceof Error ? error.message : String(error);
      console.error("[MSPChatUIDefaultLoader]", error);
    });
  }
})();
