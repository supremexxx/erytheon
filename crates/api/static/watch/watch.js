(() => {
  "use strict";

  const HORIZON_LABELS = Object.freeze({
    nowcast: "Maintenant",
    hours_6: "+6 h",
    hours_24: "+24 h",
    hours_48: "+48 h",
  });

  // Same keys/wording as the operational dashboard (dashboard.js) --
  // kept in sync deliberately so the same factor means the same thing
  // everywhere in the product.
  const FACTOR_NAMES = Object.freeze({
    fwi: "Indice feu météo",
    historical_ignitions: "Départs historiques",
    wildland_urban_interface: "Interface habitat-forêt",
    road_density: "Densité routière",
    agricultural_activity: "Activité agricole",
  });
  const FACTOR_DESCRIPTIONS = Object.freeze({
    fwi: "Synthèse du danger météo : sécheresse des combustibles, vent et potentiel de propagation.",
    historical_ignitions: "Présence passée de départs de feu autour de cette zone.",
    wildland_urban_interface: "Proximité entre végétation combustible et zones habitées.",
    road_density: "Présence de routes, associée à davantage de passages et d'activités humaines.",
    agricultural_activity: "Présence d'activités agricoles pouvant augmenter l'exposition à certaines sources d'ignition.",
  });
  const FWI_DESCRIPTIONS = Object.freeze({
    FFMC: "Humidité des végétaux fins en surface. Une valeur élevée indique qu'ils peuvent s'enflammer facilement.",
    DMC: "Sécheresse des couches organiques peu profondes. Elle renseigne sur la persistance possible du feu.",
    DC: "Sécheresse profonde et de long terme. Une valeur forte signale un déficit d'eau durable.",
    ISI: "Vitesse initiale de propagation potentielle, principalement influencée par le vent et les combustibles fins.",
    BUI: "Quantité totale de combustible disponible pour brûler.",
    FWI: "Indice global de danger d'incendie combinant propagation et combustible disponible.",
  });
  const SOURCE_NAMES = Object.freeze({
    bdiff: "BDIFF",
    calendar: "Calendrier",
    corine: "Corine Land Cover",
    firms: "NASA FIRMS",
    insee: "INSEE Filosofi",
    meteofrance_synop: "Météo-France SYNOP",
    open_meteo_arome: "AROME / ARPEGE",
    osm: "OpenStreetMap",
    promethee: "Prométhée",
  });
  const GUIDANCE = Object.freeze({
    low: "Feu possible mais peu probable.",
    moderate: "Restez prudent, en particulier en cas de vent.",
    high: "Risque important : évitez tout feu et redoublez de vigilance.",
    critical: "Risque très élevé. N'allumez aucun feu, même habituellement autorisé.",
  });
  const RISK_LABELS = Object.freeze({ low: "Faible", moderate: "Modéré", high: "Élevé", critical: "Critique" });

  function scoreCategory(score) {
    const value = Number(score) || 0;
    if (value >= 0.75) return "critical";
    if (value >= 0.50) return "high";
    if (value >= 0.25) return "moderate";
    return "low";
  }

  function formatScore(score) {
    return `${Math.round((Number(score) || 0) * 100)}`;
  }

  function formatNumber(value, digits) {
    const number = Number(value);
    return Number.isFinite(number) ? number.toFixed(digits) : "—";
  }

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (char) => (
      { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[char]
    ));
  }

  function relativeMinutes(iso) {
    if (!iso) return null;
    const diffMs = Date.now() - new Date(iso).getTime();
    return Math.max(0, Math.round(diffMs / 60000));
  }

  async function fetchJson(url, options) {
    const response = await fetch(url, options);
    if (!response.ok) {
      const body = await response.json().catch(() => null);
      const message = body && body.error ? body.error.message : response.statusText;
      throw new Error(message);
    }
    return response.json();
  }

  function debounce(fn, delayMs) {
    let timer = null;
    return (...args) => {
      if (timer) clearTimeout(timer);
      timer = setTimeout(() => fn(...args), delayMs);
    };
  }

  function main() {
    const elements = {
      horizonButtons: [...document.querySelectorAll(".watch-horizon-btn")],
      locateBtn: document.getElementById("watch-locate"),
      searchInput: document.getElementById("watch-search-input"),
      suggestions: document.getElementById("watch-suggestions"),
      sourcesBtn: document.getElementById("watch-sources-btn"),
      sourcesPop: document.getElementById("watch-sources-pop"),
      sourcesList: document.getElementById("watch-sources-list"),
      statusBanner: document.getElementById("watch-status-banner"),
      statusDetails: document.getElementById("watch-status-details"),
      metaTime: document.getElementById("watch-meta-time"),
      themeBtn: document.getElementById("watch-theme-btn"),
      loading: document.getElementById("watch-loading"),
      truncated: document.getElementById("watch-truncated"),
      territoryLabel: document.getElementById("watch-territory-label"),
      legendMode: document.getElementById("watch-legend-mode"),
      panel: document.getElementById("watch-panel"),
      tooltip: document.getElementById("watch-tooltip"),
    };

    if (!window.L) {
      elements.territoryLabel.textContent = "La carte n'a pas pu être chargée.";
      return;
    }

    const state = {
      horizon: "nowcast",
      riskLayer: null,
      selectedLayer: null,
      riskRequestId: 0,
      riskController: null,
      pendingFocus: null,
      degraded: false,
    };

    // ---------- theme ----------
    function systemPrefersDark() {
      return window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
    }
    function currentTheme() {
      return document.documentElement.getAttribute("data-theme") || (systemPrefersDark() ? "dark" : "light");
    }
    function tileUrlForTheme(theme) {
      return theme === "dark"
        ? "https://basemaps.cartocdn.com/dark_all/{z}/{x}/{y}{r}.png"
        : "https://basemaps.cartocdn.com/light_all/{z}/{x}/{y}{r}.png";
    }

    // ---------- map ----------
    const map = L.map("watch-map", { zoomControl: true, minZoom: 5, maxZoom: 15, preferCanvas: true });
    let tileLayer = L.tileLayer(tileUrlForTheme(currentTheme()), {
      attribution: "&copy; OpenStreetMap contributors &copy; CARTO",
      maxZoom: 19,
    }).addTo(map);
    map.setView([46.6, 2.2], 6);
    state.riskLayer = L.geoJSON(null).addTo(map);

    function applyTheme(theme) {
      document.documentElement.setAttribute("data-theme", theme);
      tileLayer.remove();
      tileLayer = L.tileLayer(tileUrlForTheme(theme), {
        attribution: "&copy; OpenStreetMap contributors &copy; CARTO",
        maxZoom: 19,
      }).addTo(map);
    }
    const storedTheme = window.localStorage.getItem("watch-theme");
    if (storedTheme) applyTheme(storedTheme);
    elements.themeBtn.addEventListener("click", () => {
      const next = currentTheme() === "dark" ? "light" : "dark";
      window.localStorage.setItem("watch-theme", next);
      applyTheme(next);
    });

    // ---------- config / bootstrap ----------
    // The map container's flex/dvh-driven layout can still measure as
    // 0x0 well after the config fetch resolves -- awaiting a network
    // round trip doesn't guarantee a layout pass has happened. A
    // ResizeObserver is the one thing that reliably fires exactly when
    // the container first gets a real size, so it -- not a fixed delay
    // or animation-frame guess -- is what drives the initial fit.
    let hasFitAoi = false;
    function fitToAoi() {
      if (!state.aoiBounds || hasFitAoi) return;
      const size = map.getSize();
      if (size.x === 0 || size.y === 0) return; // not really sized yet; don't consume the one-shot
      hasFitAoi = true;
      map.fitBounds(state.aoiBounds, { padding: [22, 22], animate: false });
    }
    const mapSizeObserver = new ResizeObserver((entries) => {
      const entry = entries[0];
      if (!entry || entry.contentRect.width === 0 || entry.contentRect.height === 0) return;
      map.invalidateSize(false);
      fitToAoi();
    });
    mapSizeObserver.observe(document.getElementById("watch-map"));

    async function loadConfig() {
      try {
        const config = await fetchJson("/config");
        const [west, south, east, north] = config.bbox;
        state.aoiBounds = L.latLngBounds([south, west], [north, east]);
        elements.territoryLabel.textContent = `${config.territory} · résolution H3 ${config.h3_resolution}`;
        fitToAoi();
      } catch (error) {
        elements.territoryLabel.textContent = `Territoire indisponible · ${error.message}`;
      }
    }

    // ---------- risk layer ----------
    // Below this zoom, the H3 grid is dense enough (hundreds of
    // thousands of cells over the full AOI) that rendering every cell
    // would always read as a filled mass, not the sparse "zones to
    // watch" view the product is meant to show at a glance. Overview
    // mode shows only cells that clear the alert threshold (the
    // existing /alerts endpoint, already a small, thresholded set);
    // zooming into a region switches to the full per-cell grid.
    const OVERVIEW_MAX_ZOOM = 7;
    const OVERVIEW_ALERT_THRESHOLD = 0.75;
    const MAX_ALERTS = 500; // matches the API's own hard cap (crates/api/src/lib.rs)

    // Leaflet's canvas renderer (preferCanvas: true, used for performance
    // with thousands of cells) sets these directly as a Canvas 2D
    // fillStyle/strokeStyle -- Canvas cannot resolve CSS custom
    // properties (unlike SVG/DOM styling), so var(--watch-risk-*) would
    // silently render as black. Resolve the real color value instead.
    function riskColor(category) {
      return getComputedStyle(document.documentElement).getPropertyValue(`--watch-risk-${category}`).trim();
    }

    function styleForFeature(feature) {
      const category = scoreCategory(feature?.properties?.score);
      const color = riskColor(category);
      if (feature?.geometry?.type === "Point") {
        return { radius: 4 + (Number(feature.properties.score) || 0) * 8, color, weight: 1, fillColor: color, fillOpacity: 0.8, opacity: 0.9 };
      }
      return { color, fillColor: color, fillOpacity: 0.55, opacity: 0.9, weight: 1.2 };
    }

    function alertMarkerStyle(score) {
      const category = scoreCategory(score);
      const color = riskColor(category);
      return { radius: 5 + (Number(score) || 0) * 7, color, weight: 1.4, fillColor: color, fillOpacity: 0.85, opacity: 1 };
    }

    function layerLatLng(layer) {
      if (typeof layer.getLatLng === "function") return layer.getLatLng();
      if (typeof layer.getBounds === "function") return layer.getBounds().getCenter();
      return null;
    }

    function setLegendMode(text) {
      elements.legendMode.textContent = text;
    }

    async function loadAlertsOverview() {
      const query = new URLSearchParams({
        threshold: String(OVERVIEW_ALERT_THRESHOLD),
        horizon: state.horizon,
        limit: String(MAX_ALERTS),
      });
      if (state.riskController) state.riskController.abort();
      const controller = new AbortController();
      state.riskController = controller;
      const requestId = ++state.riskRequestId;
      elements.loading.hidden = false;
      try {
        const alerts = await fetchJson(`/alerts?${query}`, { signal: controller.signal });
        if (requestId !== state.riskRequestId) return;
        state.riskLayer.remove();
        state.selectedLayer = null;
        const group = L.layerGroup();
        let latestValidAt = null;
        for (const alert of alerts) {
          const layer = L.circleMarker([alert.latitude, alert.longitude], alertMarkerStyle(alert.score));
          layer.feature = { properties: alert };
          if (alert.valid_at && (!latestValidAt || alert.valid_at > latestValidAt)) latestValidAt = alert.valid_at;
          layer.on("click", () => selectCell(alert.h3, layer, { source: "map" }));
          group.addLayer(layer);
        }
        state.riskLayer = group.addTo(map);
        // /alerts has no `truncated` flag like /risk does; landing
        // exactly on the API's own cap is the only signal available
        // that more zones exist than are shown.
        elements.truncated.hidden = alerts.length < MAX_ALERTS;
        setLegendMode(`Vue d'ensemble · zones ≥ ${Math.round(OVERVIEW_ALERT_THRESHOLD * 100)}/100 — zoomez pour la grille complète`);
        if (latestValidAt) {
          elements.metaTime.textContent = new Date(latestValidAt).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
        }
        if (state.pendingFocus) {
          focusNearestFeature(alerts);
        }
      } catch (error) {
        if (error.name === "AbortError") return;
      } finally {
        if (requestId === state.riskRequestId) elements.loading.hidden = true;
      }
    }

    const MIN_BBOX_SPAN_DEGREES = 1e-6; // ~11 cm -- far below any real viewport, well above float noise

    async function loadDenseRisk() {
      const bounds = map.getBounds();
      const west = bounds.getWest();
      const east = bounds.getEast();
      const south = bounds.getSouth();
      const north = bounds.getNorth();
      if (east - west < MIN_BBOX_SPAN_DEGREES || north - south < MIN_BBOX_SPAN_DEGREES) {
        // The container can still measure as zero-size for one query
        // right as the ResizeObserver above first fires. No real view
        // exists yet -- skip; the observer's own invalidateSize+fit
        // triggers a proper moveend, and this fires again for real.
        return;
      }
      const bbox = [west, south, east, north].join(",");
      const query = new URLSearchParams({
        bbox,
        horizon: state.horizon,
        at: "latest",
        geometry: "polygon",
        limit: "5000",
      });
      if (state.riskController) state.riskController.abort();
      const controller = new AbortController();
      state.riskController = controller;
      const requestId = ++state.riskRequestId;
      elements.loading.hidden = false;
      try {
        const collection = await fetchJson(`/risk?${query}`, { signal: controller.signal });
        if (requestId !== state.riskRequestId) return;
        state.riskLayer.remove();
        state.selectedLayer = null;
        let latestValidAt = null;
        state.riskLayer = L.geoJSON(collection, {
          style: styleForFeature,
          pointToLayer: (feature, latlng) => L.circleMarker(latlng, styleForFeature(feature)),
          onEachFeature: (feature, layer) => {
            const validAt = feature.properties?.valid_at;
            if (validAt && (!latestValidAt || validAt > latestValidAt)) latestValidAt = validAt;
            layer.on("click", () => selectCell(feature.properties.h3, layer, { source: "map" }));
          },
        }).addTo(map);
        elements.truncated.hidden = !collection.truncated;
        setLegendMode("Grille complète · toutes les cellules de la vue");
        if (latestValidAt) {
          elements.metaTime.textContent = new Date(latestValidAt).toLocaleTimeString("fr-FR", { hour: "2-digit", minute: "2-digit" });
        }
        if (state.pendingFocus) {
          focusNearestFeature(collection.features || []);
        }
      } catch (error) {
        if (error.name === "AbortError") return;
      } finally {
        if (requestId === state.riskRequestId) elements.loading.hidden = true;
      }
    }

    function loadRisk() {
      return map.getZoom() <= OVERVIEW_MAX_ZOOM ? loadAlertsOverview() : loadDenseRisk();
    }
    const scheduleRiskReload = debounce(loadRisk, 300);
    map.on("moveend zoomend", scheduleRiskReload);

    function focusNearestFeature(features) {
      const target = state.pendingFocus;
      state.pendingFocus = null;
      if (!target || features.length === 0) return;
      let nearest = null;
      let nearestDistance = Infinity;
      state.riskLayer.eachLayer((layer) => {
        const latlng = layerLatLng(layer);
        if (!latlng) return;
        const distance = Math.hypot(latlng.lat - target.lat, latlng.lng - target.lng);
        if (distance < nearestDistance) {
          nearestDistance = distance;
          nearest = layer;
        }
      });
      if (nearest && nearest.feature) {
        selectCell(nearest.feature.properties.h3, nearest, { source: target.source, name: target.name });
      }
    }

    // ---------- detail panel ----------
    function highlightLayer(layer) {
      if (state.selectedLayer && state.selectedLayer !== layer) {
        state.selectedLayer.setStyle(styleForFeature(state.selectedLayer.feature));
      }
      state.selectedLayer = layer || null;
      if (layer) {
        layer.setStyle({ weight: 2.6, opacity: 1 });
        if (typeof layer.bringToFront === "function") layer.bringToFront();
      }
    }

    async function selectCell(h3, layer, opts) {
      if (!h3) return;
      highlightLayer(layer);
      elements.panel.innerHTML = '<div class="watch-panel-empty">Chargement…</div>';
      elements.panel.classList.add("is-open");
      try {
        const detail = await fetchJson(`/risk/cell/${encodeURIComponent(h3)}?${new URLSearchParams({ horizon: state.horizon })}`);
        renderPanel(detail, opts || {});
      } catch (error) {
        elements.panel.innerHTML = `<div class="watch-panel-error">Détail indisponible · ${escapeHtml(error.message)}</div>`;
      }
    }

    function infoButton(tip) {
      if (!tip) return "";
      return `<button type="button" class="watch-info-btn" data-tip="${escapeHtml(tip)}" aria-expanded="false" aria-label="Explication">i</button>`;
    }
    function row(label, value, tip) {
      return `<div class="watch-readout-row"><span class="watch-k">${escapeHtml(label)}${infoButton(tip)}</span><span class="watch-v">${value}</span></div>`;
    }

    function renderPanel(detail, opts) {
      const current = detail.current;
      const score = Number(current.score) || 0;
      const category = scoreCategory(score);
      const title = opts.name || `Cellule ${detail.h3}`;
      const eyebrow = opts.source === "search" ? "Résultat de recherche"
        : opts.source === "geo" ? "Votre position (approximative)"
        : "Point sélectionné";

      const fwi = detail.fwi || {};
      const fwiRows = [
        ["FFMC", fwi.ffmc], ["DMC", fwi.dmc], ["DC", fwi.dc], ["ISI", fwi.isi], ["BUI", fwi.bui],
      ].map(([name, value]) => row(name, formatNumber(value, 1), FWI_DESCRIPTIONS[name])).join("");

      const factors = current.top_factors || [];
      const factorRows = factors.length
        ? factors.map((factor) => row(
            FACTOR_NAMES[factor.name] || factor.name,
            formatNumber(factor.value, 2),
            FACTOR_DESCRIPTIONS[factor.name],
          )).join("")
        : row("Facteurs", "Aucun facteur notable", null);

      elements.panel.innerHTML = `
        <div class="watch-panel-head">
          <div>
            <p class="watch-panel-eyebrow">${escapeHtml(eyebrow)}</p>
            <h2 class="watch-panel-title">${escapeHtml(title)}</h2>
            <p class="watch-panel-guidance">${escapeHtml(GUIDANCE[category])}</p>
          </div>
          <button type="button" class="watch-close-btn" id="watch-panel-close" aria-label="Fermer le détail">&times;</button>
        </div>
        <div class="watch-panel-body">
          <div class="watch-risk-readout">
            <span class="watch-risk-pill" style="background:var(--watch-risk-${category}-soft);color:var(--watch-risk-${category})">${RISK_LABELS[category]}</span>
            <span class="watch-risk-score">${formatScore(score)}<span>/100</span></span>
          </div>

          <p class="watch-section-label">Pourquoi ce niveau ?</p>
          <div class="watch-readout-rows">
            ${row("Sécheresse de la végétation", formatScore(current.physical), "Composante météo du risque : sécheresse des combustibles, vent et propagation potentielle (indice FWI).")}
            ${row("Départ d'origine humaine", formatScore(current.human), "Estimation de la probabilité qu'un feu soit déclenché par une activité humaine, à partir de l'historique local.")}
            ${factorRows}
          </div>

          <p class="watch-section-label">Indice feu météo (FWI)</p>
          <div class="watch-readout-rows">${fwiRows}</div>

          <p class="watch-section-label">Origine des données</p>
          <div class="watch-readout-rows">
            ${row("Horizon", HORIZON_LABELS[current.horizon] || current.horizon, null)}
            ${row("Calculé le", new Date(current.computed_at).toLocaleString("fr-FR"), null)}
          </div>
        </div>
      `;
      document.getElementById("watch-panel-close").addEventListener("click", closePanel);
      bindTooltips(elements.panel);
    }

    function closePanel() {
      elements.panel.classList.remove("is-open");
      highlightLayer(null);
      hideTooltip();
    }

    // ---------- tooltips ----------
    function showTooltip(button) {
      elements.tooltip.textContent = button.dataset.tip;
      const rect = button.getBoundingClientRect();
      elements.tooltip.style.left = `${Math.min(rect.left, window.innerWidth - 280)}px`;
      elements.tooltip.style.top = `${rect.bottom + 8}px`;
      elements.tooltip.classList.add("is-shown");
      document.querySelectorAll('.watch-info-btn[aria-expanded="true"]').forEach((btn) => btn.setAttribute("aria-expanded", "false"));
      button.setAttribute("aria-expanded", "true");
    }
    function hideTooltip() {
      elements.tooltip.classList.remove("is-shown");
      document.querySelectorAll('.watch-info-btn[aria-expanded="true"]').forEach((btn) => btn.setAttribute("aria-expanded", "false"));
    }
    function bindTooltips(scope) {
      scope.querySelectorAll(".watch-info-btn").forEach((button) => {
        button.addEventListener("mouseenter", () => showTooltip(button));
        button.addEventListener("mouseleave", hideTooltip);
        button.addEventListener("focus", () => showTooltip(button));
        button.addEventListener("blur", hideTooltip);
        button.addEventListener("click", (event) => {
          event.stopPropagation();
          if (button.getAttribute("aria-expanded") === "true") hideTooltip();
          else showTooltip(button);
        });
      });
    }
    document.addEventListener("click", (event) => {
      if (!event.target.closest(".watch-info-btn")) hideTooltip();
    });
    document.addEventListener("keydown", (event) => {
      if (event.key === "Escape") { hideTooltip(); closePanel(); }
    });

    // ---------- horizon tabs ----------
    elements.horizonButtons.forEach((button) => {
      button.addEventListener("click", () => {
        elements.horizonButtons.forEach((other) => other.setAttribute("aria-pressed", String(other === button)));
        state.horizon = button.dataset.horizon;
        loadRisk();
        if (state.selectedLayer && state.selectedLayer.feature) {
          selectCell(state.selectedLayer.feature.properties.h3, state.selectedLayer, {});
        }
      });
    });

    // ---------- commune search ----------
    const runSearch = debounce(async (value) => {
      if (value.trim().length < 2) {
        elements.suggestions.hidden = true;
        elements.suggestions.innerHTML = "";
        return;
      }
      try {
        const results = await fetchJson(`/api/watch/communes?${new URLSearchParams({ q: value.trim() })}`);
        renderSuggestions(results);
      } catch {
        renderSuggestions([]);
      }
    }, 250);

    function renderSuggestions(results) {
      if (results.length === 0) {
        elements.suggestions.innerHTML = '<p class="watch-suggestion-empty">Aucune commune trouvée.</p>';
        elements.suggestions.hidden = false;
        return;
      }
      elements.suggestions.innerHTML = results.map((commune) => `
        <button type="button" class="watch-suggestion" data-insee="${escapeHtml(commune.insee_code)}">
          ${escapeHtml(commune.name)}${commune.department_code ? ` <span class="watch-dept">(${escapeHtml(commune.department_code)})</span>` : ""}
        </button>
      `).join("");
      elements.suggestions.hidden = false;
      elements.suggestions.querySelectorAll(".watch-suggestion").forEach((button, index) => {
        button.addEventListener("click", () => pickCommune(results[index]));
      });
    }

    async function pickCommune(commune) {
      elements.searchInput.value = commune.name;
      elements.suggestions.hidden = true;
      try {
        const lookup = await fetchJson(`/api/watch/communes/${encodeURIComponent(commune.insee_code)}`);
        const [west, south, east, north] = lookup.bbox;
        const bounds = L.latLngBounds([south, west], [north, east]);
        state.pendingFocus = { lat: bounds.getCenter().lat, lng: bounds.getCenter().lng, source: "search", name: lookup.name };
        map.fitBounds(bounds, { padding: [30, 30] });
      } catch (error) {
        elements.suggestions.innerHTML = `<p class="watch-suggestion-empty">Commune introuvable · ${escapeHtml(error.message)}</p>`;
        elements.suggestions.hidden = false;
      }
    }

    elements.searchInput.addEventListener("input", (event) => runSearch(event.target.value));
    document.addEventListener("click", (event) => {
      if (!event.target.closest(".watch-search")) elements.suggestions.hidden = true;
    });

    // ---------- "Ma position" ----------
    elements.locateBtn.addEventListener("click", () => {
      if (!navigator.geolocation) {
        showLocateMessage("Géolocalisation indisponible sur cet appareil.");
        return;
      }
      elements.locateBtn.disabled = true;
      elements.locateBtn.querySelector(".watch-locate-label").textContent = "Localisation…";
      elements.locateBtn.querySelector("svg").classList.add("watch-spin");
      navigator.geolocation.getCurrentPosition(
        (position) => {
          resetLocateButton(true);
          const { latitude, longitude } = position.coords;
          state.pendingFocus = { lat: latitude, lng: longitude, source: "geo", name: null };
          map.setView([latitude, longitude], Math.max(map.getZoom(), 12));
        },
        () => {
          resetLocateButton(false);
          showLocateMessage("Position indisponible ou refusée.");
        },
        { timeout: 8000 },
      );
    });
    function resetLocateButton(active) {
      elements.locateBtn.disabled = false;
      elements.locateBtn.classList.toggle("is-active", active);
      elements.locateBtn.querySelector(".watch-locate-label").textContent = "Ma position";
      elements.locateBtn.querySelector("svg").classList.remove("watch-spin");
    }
    function showLocateMessage(message) {
      const label = elements.locateBtn.querySelector(".watch-locate-label");
      const original = "Ma position";
      label.textContent = message;
      window.setTimeout(() => { label.textContent = original; }, 2500);
    }

    // ---------- sources / freshness ----------
    async function loadSources() {
      try {
        const sources = await fetchJson("/sources");
        state.degraded = sources.some((source) => Boolean(source.recent_error));
        elements.sourcesBtn.classList.toggle("is-degraded", state.degraded);
        elements.statusBanner.hidden = !state.degraded;
        elements.sourcesList.innerHTML = sources.map((source) => {
          const minutes = relativeMinutes(source.last_success);
          const freshness = source.recent_error
            ? "en retard"
            : minutes === null ? "aucun succès" : `reçu il y a ${minutes} min`;
          return `<div class="watch-source-row${source.recent_error ? " is-degraded" : ""}">
            <span class="watch-source-name">${escapeHtml(SOURCE_NAMES[source.id] || source.id)}</span>
            <span class="watch-source-state">${escapeHtml(freshness)}</span>
          </div>`;
        }).join("");
      } catch (error) {
        elements.sourcesList.innerHTML = `<div class="watch-source-row">Sources indisponibles · ${escapeHtml(error.message)}</div>`;
      }
    }
    elements.sourcesBtn.addEventListener("click", (event) => {
      event.stopPropagation();
      const open = elements.sourcesPop.classList.toggle("is-open");
      elements.sourcesBtn.setAttribute("aria-expanded", String(open));
    });
    elements.statusDetails.addEventListener("click", (event) => {
      event.stopPropagation();
      elements.sourcesPop.classList.add("is-open");
      elements.sourcesBtn.setAttribute("aria-expanded", "true");
    });
    document.addEventListener("click", (event) => {
      if (!event.target.closest("#watch-sources-btn")) {
        elements.sourcesPop.classList.remove("is-open");
        elements.sourcesBtn.setAttribute("aria-expanded", "false");
      }
    });

    // ---------- bootstrap ----------
    loadConfig().then(loadRisk);
    loadSources();
  }

  document.addEventListener("DOMContentLoaded", main);
})();
