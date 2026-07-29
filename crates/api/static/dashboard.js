(() => {
  "use strict";

  const INSTANCES = new WeakMap();

  function mountOperationalMap({ root = document } = {}) {
    const mapElement = root.querySelector("#map");
    if (!mapElement) return null;
    if (INSTANCES.has(mapElement)) return INSTANCES.get(mapElement);

    let AOI = { west: 1.68, south: 42.57, east: 3.26, north: 43.46 };
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
    const SOURCE_DESCRIPTIONS = Object.freeze({
      bdiff: "Base française des incendies de forêt. Elle fournit les feux historiques et, lorsqu’elle est connue, leur cause.",
      calendar: "Calendrier utilisé pour tenir compte des week-ends, jours fériés et périodes de fréquentation.",
      corine: "Carte européenne de l’occupation des sols : forêts, cultures, villes et autres types de terrain.",
      firms: "Détections satellites récentes de chaleur et de feux actifs fournies par la NASA.",
      insee: "Données publiques françaises utilisées pour estimer la présence de population et l’exposition des zones habitées.",
      meteofrance_synop: "Observations météo de stations françaises utilisées comme données de référence.",
      open_meteo_arome: "Prévisions de température, humidité, pluie et vent utilisées pour anticiper l’évolution du risque.",
      osm: "Routes, bâtiments, activités, lignes électriques et autres éléments cartographiques utiles au risque humain.",
      promethee: "Historique des incendies de la zone méditerranéenne française.",
    });
    const HORIZON_LABELS = Object.freeze({
      nowcast: "Maintenant",
      hours_6: "+6 h",
      hours_24: "+24 h",
      hours_48: "+48 h",
    });
    const FACTOR_NAMES = Object.freeze({
      fwi: "Indice forêt météo",
      historical_ignitions: "Départs historiques",
      wildland_urban_interface: "Interface habitat-forêt",
      road_density: "Densité routière",
      agricultural_activity: "Activité agricole",
    });
    const FACTOR_DESCRIPTIONS = Object.freeze({
      fwi: "Synthèse du danger météo : sécheresse des combustibles, vent et potentiel de propagation.",
      historical_ignitions: "Présence passée de départs de feu autour de cette zone.",
      wildland_urban_interface: "Proximité entre végétation combustible et zones habitées.",
      road_density: "Présence de routes, associée à davantage de passages et d’activités humaines.",
      agricultural_activity: "Présence d’activités agricoles pouvant augmenter l’exposition à certaines sources d’ignition.",
    });
    const FWI_DESCRIPTIONS = Object.freeze({
      FFMC: "Humidité des végétaux fins en surface. Une valeur élevée indique qu’ils peuvent s’enflammer facilement.",
      DMC: "Sécheresse des couches organiques peu profondes. Elle renseigne sur la persistance possible du feu.",
      DC: "Sécheresse profonde et de long terme. Une valeur forte signale un déficit d’eau durable.",
      ISI: "Vitesse initiale de propagation potentielle, principalement influencée par le vent et les combustibles fins.",
      BUI: "Quantité totale de combustible disponible pour brûler.",
      FWI: "Indice global de danger d’incendie combinant propagation et combustible disponible.",
    });

    const elements = {
      connectionStatus: root.querySelector("#connection-status"),
      territoryLabel: root.querySelector("#territory-label"),
      h3Resolution: root.querySelector("#h3-resolution"),
      connectionLabel: root.querySelector("#connection-label"),
      refreshButton: root.querySelector("#refresh-button"),
      activeHorizonLabel: root.querySelector("#active-horizon-label"),
      horizonButtons: [...root.querySelectorAll("[data-horizon]")],
      horizonValidAt: root.querySelector("#horizon-valid-at"),
      lastUpdate: root.querySelector("#last-update"),
      maxScore: root.querySelector("#max-score"),
      maxScoreLabel: root.querySelector("#max-score-label"),
      visibleCells: root.querySelector("#visible-cells"),
      alertCount: root.querySelector("#alert-count"),
      alertThresholdLabel: root.querySelector("#alert-threshold-label"),
      thresholdRange: root.querySelector("#threshold-range"),
      thresholdOutput: root.querySelector("#threshold-output"),
      alertsBadge: root.querySelector("#alerts-badge"),
      alertsLoading: root.querySelector("#alerts-loading"),
      alertList: root.querySelector("#alert-list"),
      sourceSummary: root.querySelector("#source-summary"),
      sourceList: root.querySelector("#source-list"),
      mapStatus: root.querySelector("#map-status"),
      mapCellCount: root.querySelector("#map-cell-count"),
      mapLoading: root.querySelector("#map-loading"),
      emptyState: root.querySelector("#empty-state"),
      detailDrawer: root.querySelector("#detail-drawer"),
      drawerClose: root.querySelector("#drawer-close"),
      detailH3: root.querySelector("#detail-h3"),
      detailTime: root.querySelector("#detail-time"),
      scoreGauge: root.querySelector("#score-gauge"),
      detailScore: root.querySelector("#detail-score"),
      physicalValue: root.querySelector("#physical-value"),
      physicalBar: root.querySelector("#physical-bar"),
      humanValue: root.querySelector("#human-value"),
      humanBar: root.querySelector("#human-bar"),
      fwiGrid: root.querySelector("#fwi-grid"),
      factorList: root.querySelector("#factor-list"),
      historyCount: root.querySelector("#history-count"),
      historyChart: root.querySelector("#history-chart"),
      tooltipPortal: root.querySelector("#tooltip-portal"),
    };

    const state = {
      threshold: Number(elements.thresholdRange.value),
      horizon: "nowcast",
      riskLayer: null,
      selectedLayer: null,
      riskRequest: 0,
      riskController: null,
      websocket: null,
      reconnectTimer: null,
      refreshTimer: null,
      destroyed: false,
    };

    if (!window.L) {
      setConnection("offline", "Carte indisponible");
      elements.mapLoading.innerHTML = "La bibliothèque cartographique n’a pas pu être chargée.";
      return;
    }

    const map = L.map(mapElement, {
      zoomControl: false,
      minZoom: 5,
      maxZoom: 15,
      preferCanvas: true,
    });
    L.control.zoom({ position: "bottomright" }).addTo(map);
    L.tileLayer("https://{s}.basemaps.cartocdn.com/light_all/{z}/{x}/{y}{r}.png", {
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> &copy; <a href="https://carto.com/attributions">CARTO</a>',
      subdomains: "abcd",
      maxZoom: 20,
    }).addTo(map);
    state.riskLayer = L.geoJSON(null).addTo(map);
    map.fitBounds([[AOI.south, AOI.west], [AOI.north, AOI.east]], { padding: [22, 22] });

    async function loadConfig() {
      const response = await fetch("/config", { headers: { Accept: "application/json" } });
      if (!response.ok) throw new Error(`Configuration HTTP ${response.status}`);
      const config = await response.json();
      if (state.destroyed) return;
      const [west, south, east, north] = config.bbox;
      AOI = { west, south, east, north };
      elements.territoryLabel.textContent = config.territory;
      elements.h3Resolution.textContent = String(config.h3_resolution);
      map.fitBounds([[south, west], [north, east]], { padding: [22, 22] });
    }

    function scoreColor(score) {
      if (score >= 0.75) return "#b9271b";
      if (score >= 0.50) return "#d8672f";
      if (score >= 0.25) return "#d6a338";
      return "#7c9a62";
    }

    function scoreLabel(score) {
      if (score >= 0.75) return "Critique";
      if (score >= 0.50) return "Élevé";
      if (score >= 0.25) return "Modéré";
      return "Faible";
    }

    function scoreStyle(feature) {
      const score = Number(feature?.properties?.score ?? 0);
      return {
        color: scoreColor(score),
        fillColor: scoreColor(score),
        fillOpacity: Math.max(0.34, Math.min(0.82, 0.32 + score * 0.68)),
        opacity: 0.95,
        weight: 1.15,
      };
    }

    function currentBounds() {
      const bounds = map.getBounds();
      const clipped = {
        west: Math.max(AOI.west, bounds.getWest()),
        south: Math.max(AOI.south, bounds.getSouth()),
        east: Math.min(AOI.east, bounds.getEast()),
        north: Math.min(AOI.north, bounds.getNorth()),
      };
      if (clipped.west >= clipped.east || clipped.south >= clipped.north) return null;
      return clipped;
    }

    function bboxParameter() {
      const bounds = currentBounds();
      if (!bounds) return null;
      return [bounds.west, bounds.south, bounds.east, bounds.north]
        .map((value) => value.toFixed(6))
        .join(",");
    }

    async function fetchJson(path, options = {}) {
      const response = await fetch(path, {
        ...options,
        headers: { Accept: "application/json", ...(options.headers || {}) },
      });
      if (!response.ok) {
        let message = `${response.status} ${response.statusText}`;
        try {
          const payload = await response.json();
          message = payload?.error?.message || message;
        } catch (_) {
          // The status text remains the best available message.
        }
        throw new Error(message);
      }
      return response.json();
    }

    async function loadRisk() {
      const bbox = bboxParameter();
      const requestId = ++state.riskRequest;
      state.riskController?.abort();
      const controller = new AbortController();
      state.riskController = controller;
      elements.mapLoading.hidden = false;
      elements.emptyState.hidden = true;
      elements.mapStatus.textContent = "Mise à jour de la carte";

      if (!bbox) {
        state.riskLayer.clearLayers();
        renderRiskSummary([]);
        elements.mapLoading.hidden = true;
        elements.emptyState.hidden = false;
        return;
      }

      const query = new URLSearchParams({
        bbox,
        min_score: state.threshold.toFixed(2),
        at: "latest",
        horizon: state.horizon,
        limit: map.getZoom() <= 7 ? "2000" : "5000",
        geometry: map.getZoom() <= 7 ? "point" : "polygon",
      });
      try {
        const collection = await fetchJson(`/risk?${query}`, { signal: controller.signal });
        if (requestId !== state.riskRequest) return;
        const features = collection.features || [];
        state.riskLayer.remove();
        state.selectedLayer = null;
        state.riskLayer = L.geoJSON(collection, {
          style: scoreStyle,
          pointToLayer: (feature, latlng) => {
            const score = Number(feature?.properties?.score || 0);
            return L.circleMarker(latlng, {
              radius: 4 + score * 9,
              color: "rgba(255,255,255,0.85)",
              weight: 1,
              fillColor: scoreColor(score),
              fillOpacity: 0.82,
              opacity: 0.7,
            });
          },
          onEachFeature: (feature, layer) => {
            const properties = feature.properties || {};
            const tooltip = `<div class="tooltip-content"><strong>${formatScore(properties.score)}</strong><span>${escapeHtml(properties.h3 || "—")}</span></div>`;
            const tooltipOptions = { className: "risk-tooltip", direction: "top", offset: [0, -4] };
            layer.bindTooltip(tooltip, tooltipOptions);
            layer.on({
              click: () => selectCell(properties.h3, layer),
              mouseover: () => layer.setStyle({ weight: 2.2, fillOpacity: 0.9 }),
              mouseout: () => {
                if (layer !== state.selectedLayer) layer.setStyle(scoreStyle(feature));
              },
            });
          },
        }).addTo(map);
        renderRiskSummary(features, Boolean(collection.truncated));
        elements.emptyState.querySelector("strong").textContent = "Aucune cellule à ce seuil";
        elements.emptyState.querySelector("p").textContent = "Réduisez le seuil ou relancez un calcul du risque.";
        elements.emptyState.hidden = features.length > 0;
        elements.mapStatus.textContent = collection.truncated
          ? "Vue allégée · zoomez pour afficher le détail"
          : "Vue synchronisée";
        setConnection("online", "API opérationnelle");
      } catch (error) {
        if (error.name === "AbortError") return;
        if (requestId !== state.riskRequest) return;
        state.riskLayer.clearLayers();
        renderRiskSummary([]);
        elements.emptyState.hidden = false;
        elements.emptyState.querySelector("strong").textContent = "Données indisponibles";
        elements.emptyState.querySelector("p").textContent = error.message;
        elements.mapStatus.textContent = "Erreur de synchronisation";
        setConnection("offline", "API indisponible");
      } finally {
        if (requestId === state.riskRequest) elements.mapLoading.hidden = true;
      }
    }

    function renderRiskSummary(features, truncated = false) {
      const sorted = [...features].sort(
        (left, right) => Number(right.properties?.score || 0) - Number(left.properties?.score || 0),
      );
      const highest = sorted[0]?.properties;
      elements.visibleCells.textContent = formatInteger(features.length);
      const unit = map.getZoom() <= 7 ? "points" : (features.length > 1 ? "hexagones" : "hexagone");
      elements.mapCellCount.textContent = `${formatInteger(features.length)} ${unit}${truncated ? " prioritaires" : ""}`;
      if (!highest) {
        elements.maxScore.textContent = "—";
        elements.maxScoreLabel.textContent = "Aucune cellule visible";
        return;
      }
      elements.maxScore.textContent = formatScore(highest.score);
      elements.maxScoreLabel.textContent = `${scoreLabel(Number(highest.score))} · ${highest.h3}`;
      elements.lastUpdate.dateTime = highest.computed_at;
      elements.lastUpdate.textContent = formatDate(highest.computed_at);
      elements.horizonValidAt.dateTime = highest.valid_at;
      elements.horizonValidAt.textContent = formatDate(highest.valid_at);
    }

    async function loadAlerts() {
      elements.alertsLoading.hidden = false;
      const query = new URLSearchParams({
        threshold: state.threshold.toFixed(2),
        horizon: state.horizon,
        limit: "100",
      });
      try {
        const alerts = await fetchJson(`/alerts?${query}`);
        elements.alertCount.textContent = formatInteger(alerts.length);
        elements.alertsBadge.textContent = alerts.length > 99 ? "99+" : String(alerts.length);
        elements.alertThresholdLabel.textContent = `Seuil ≥ ${formatScore(state.threshold)}`;
        elements.alertList.replaceChildren(...alerts.slice(0, 20).map(alertNode));
        if (alerts.length === 0) {
          elements.alertsLoading.textContent = "Aucune alerte au seuil courant.";
          elements.alertsLoading.hidden = false;
        } else {
          elements.alertsLoading.hidden = true;
        }
      } catch (error) {
        elements.alertCount.textContent = "—";
        elements.alertsLoading.textContent = `Alertes indisponibles · ${error.message}`;
      }
    }

    function alertNode(alert, index) {
      const item = document.createElement("li");
      const button = document.createElement("button");
      button.type = "button";
      button.className = "alert-item";
      button.innerHTML = `
        <span class="alert-rank">${String(index + 1).padStart(2, "0")}</span>
        <span class="alert-main">
          <strong>${escapeHtml(alert.h3)}</strong>
          <span>${Number(alert.latitude).toFixed(4)}° N · ${Number(alert.longitude).toFixed(4)}° E</span>
        </span>
        <span class="alert-score" style="color:${scoreColor(Number(alert.score))}">${formatScore(alert.score)}</span>
      `;
      button.addEventListener("click", () => {
        map.flyTo([alert.latitude, alert.longitude], Math.max(map.getZoom(), 12), { duration: 0.7 });
        selectCell(alert.h3, null);
      });
      item.append(button);
      return item;
    }

    async function loadSources() {
      try {
        const [health, sources] = await Promise.all([fetchJson("/health"), fetchJson("/sources")]);
        const hasForecastSource = sources.some((source) => source.id === "open_meteo_arome");
        const operationalSources = sources.filter(
          (source) => !hasForecastSource || source.id !== "meteofrance_synop",
        );
        const healthy = operationalSources.filter((source) => !source.recent_error && !isMissingHumanHistory(source)).length;
        elements.sourceSummary.textContent = `${healthy}/${operationalSources.length} OK`;
        elements.sourceList.replaceChildren(...operationalSources.map(sourceNode));
        setConnection(health.status === "ok" ? "online" : "offline", health.status === "ok" ? "API opérationnelle" : "Service dégradé");
      } catch (error) {
        elements.sourceSummary.textContent = "Hors ligne";
        const item = document.createElement("li");
        item.className = "list-state";
        item.textContent = `Sources indisponibles · ${error.message}`;
        elements.sourceList.replaceChildren(item);
        setConnection("offline", "API indisponible");
      }
    }

    function sourceNode(source) {
      const item = document.createElement("li");
      const missingHumanHistory = isMissingHumanHistory(source);
      item.className = `source-item${source.recent_error ? " has-error" : ""}${missingHumanHistory ? " is-empty" : ""}`;
      const lastSuccess = source.last_success ? relativeDate(source.last_success) : "aucun succès";
      item.innerHTML = `
        <i class="source-light" aria-hidden="true"></i>
        <span class="source-name">${escapeHtml(SOURCE_NAMES[source.id] || source.id)} <button class="info-tip" type="button" aria-label="Comprendre ${escapeHtml(SOURCE_NAMES[source.id] || source.id)}" data-tooltip="${escapeHtml(SOURCE_DESCRIPTIONS[source.id] || "Source de données utilisée par le modèle ERYTHEON.")}">?</button></span>
        <span class="source-meta" title="${escapeHtml(source.recent_error || "")}">${source.recent_error ? "ERREUR" : missingHumanHistory ? "NON CHARGÉE" : `${formatInteger(source.observation_count)} · ${lastSuccess}`}</span>
      `;
      return item;
    }

    function isMissingHumanHistory(source) {
      return ["bdiff", "promethee"].includes(source.id) && Number(source.observation_count) === 0;
    }

    async function selectCell(h3, layer) {
      if (!h3) return;
      if (state.selectedLayer && state.selectedLayer !== layer) {
        state.selectedLayer.setStyle(scoreStyle(state.selectedLayer.feature));
      }
      state.selectedLayer = layer;
      if (layer) layer.setStyle({ color: "#171815", weight: 2.5, fillOpacity: 0.92 });
      elements.detailH3.textContent = h3;
      elements.detailTime.textContent = "Chargement du détail…";
      elements.detailDrawer.classList.add("is-open");
      elements.detailDrawer.setAttribute("aria-hidden", "false");
      try {
        const query = new URLSearchParams({ horizon: state.horizon });
        const detail = await fetchJson(`/risk/cell/${encodeURIComponent(h3)}?${query}`);
        renderDetail(detail);
      } catch (error) {
        elements.detailTime.textContent = `Détail indisponible · ${error.message}`;
      }
    }

    function renderDetail(detail) {
      const current = detail.current;
      const score = Number(current.score);
      const color = scoreColor(score);
      elements.detailH3.textContent = detail.h3;
      elements.detailTime.textContent = `${HORIZON_LABELS[current.horizon] || current.horizon} · ${formatDate(current.valid_at)} · ${scoreLabel(score)}`;
      elements.detailScore.textContent = formatScore(score);
      elements.scoreGauge.style.setProperty("--score-color", color);
      elements.physicalValue.textContent = formatScore(current.physical);
      elements.humanValue.textContent = formatScore(current.human);
      elements.physicalBar.style.width = `${Math.round(Number(current.physical) * 100)}%`;
      elements.humanBar.style.width = `${Math.round(Number(current.human) * 100)}%`;

      const fwi = detail.fwi;
      const fwiFields = [
        ["FFMC", fwi.ffmc], ["DMC", fwi.dmc], ["DC", fwi.dc],
        ["ISI", fwi.isi], ["BUI", fwi.bui], ["FWI", fwi.fwi],
      ];
      elements.fwiGrid.innerHTML = fwiFields
        .map(([name, value]) => `<div class="fwi-item"><span>${name} <button class="info-tip" type="button" aria-label="Comprendre ${name}" data-tooltip="${escapeHtml(FWI_DESCRIPTIONS[name])}">?</button></span><strong>${formatNumber(value, 1)}</strong></div>`)
        .join("");

      const factors = current.top_factors || [];
      if (factors.length === 0) {
        const item = document.createElement("li");
        item.className = "list-state";
        item.textContent = "Aucun facteur positif pour cette cellule.";
        elements.factorList.replaceChildren(item);
      } else {
        elements.factorList.innerHTML = factors.map((factor, index) => `
          <li class="factor-item">
            <span class="factor-rank">0${index + 1}</span>
            <span class="factor-body">
              <strong>${escapeHtml(FACTOR_NAMES[factor.name] || factor.name)} <button class="info-tip" type="button" aria-label="Comprendre ce facteur" data-tooltip="${escapeHtml(FACTOR_DESCRIPTIONS[factor.name] || "Facteur pris en compte par le modèle pour calculer le risque local.")}">?</button></strong>
              <span>Valeur normalisée ${formatScore(factor.value)}</span>
            </span>
            <span class="factor-value">+${formatScore(factor.contribution)}</span>
          </li>
        `).join("");
      }
      renderHistory(detail.history || []);
    }

    function renderHistory(history) {
      const sorted = [...history].sort((left, right) => new Date(left.computed_at) - new Date(right.computed_at));
      elements.historyCount.textContent = `${sorted.length} point${sorted.length > 1 ? "s" : ""}`;
      const width = 320;
      const height = 84;
      const padding = 5;
      const points = sorted.length > 0 ? sorted : [{ score: 0 }];
      const coordinates = points.map((item, index) => {
        const x = points.length === 1 ? width / 2 : padding + index * (width - padding * 2) / (points.length - 1);
        const y = height - padding - Number(item.score) * (height - padding * 2);
        return [x, y];
      });
      const line = coordinates.map(([x, y]) => `${x.toFixed(1)},${y.toFixed(1)}`).join(" ");
      const area = `${padding},${height - padding} ${line} ${width - padding},${height - padding}`;
      const last = coordinates.at(-1);
      elements.historyChart.innerHTML = `
        <defs><linearGradient id="history-gradient" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="#ff5c35" stop-opacity="0.28"/><stop offset="1" stop-color="#ff5c35" stop-opacity="0"/></linearGradient></defs>
        <line class="grid-line" x1="0" y1="${height * 0.25}" x2="${width}" y2="${height * 0.25}"/>
        <line class="grid-line" x1="0" y1="${height * 0.5}" x2="${width}" y2="${height * 0.5}"/>
        <line class="grid-line" x1="0" y1="${height * 0.75}" x2="${width}" y2="${height * 0.75}"/>
        <polygon class="area" points="${area}"/>
        <polyline class="line" points="${line}"/>
        <circle class="point" cx="${last[0]}" cy="${last[1]}" r="3.5"/>
      `;
    }

    function closeDetail() {
      elements.detailDrawer.classList.remove("is-open");
      elements.detailDrawer.setAttribute("aria-hidden", "true");
      if (state.selectedLayer) state.selectedLayer.setStyle(scoreStyle(state.selectedLayer.feature));
      state.selectedLayer = null;
    }

    function setConnection(status, label) {
      elements.connectionStatus.classList.toggle("is-online", status === "online");
      elements.connectionStatus.classList.toggle("is-offline", status === "offline");
      elements.connectionLabel.textContent = label;
    }

    function connectWebSocket() {
      if (state.destroyed) return;
      clearTimeout(state.reconnectTimer);
      const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
      const socket = new WebSocket(`${protocol}//${window.location.host}/stream`);
      state.websocket = socket;
      socket.addEventListener("open", () => {
        setConnection("online", "Temps réel connecté");
        subscribeSocket();
      });
      socket.addEventListener("message", (event) => {
        try {
          const update = JSON.parse(event.data);
          if (update.type === "risk_update" && update.cells?.length > 0) {
            elements.mapStatus.textContent = `${formatInteger(update.cells.length)} cellules actualisées`;
            clearTimeout(state.refreshTimer);
            state.refreshTimer = setTimeout(() => Promise.all([loadRisk(), loadAlerts(), loadSources()]), 450);
          }
        } catch (_) {
          // Ignore non-protocol messages.
        }
      });
      socket.addEventListener("close", () => {
        if (state.websocket === socket) {
          if (!state.destroyed) {
            setConnection("offline", "Reconnexion temps réel…");
            state.reconnectTimer = setTimeout(connectWebSocket, 4000);
          }
        }
      });
      socket.addEventListener("error", () => socket.close());
    }

    function subscribeSocket() {
      if (state.websocket?.readyState !== WebSocket.OPEN) return;
      const bounds = currentBounds();
      if (!bounds) return;
      state.websocket.send(JSON.stringify({
        type: "subscribe",
        bbox: [bounds.west, bounds.south, bounds.east, bounds.north],
      }));
    }

    async function refreshAll() {
      if (state.destroyed) return;
      elements.refreshButton.classList.add("is-spinning");
      await Promise.allSettled([loadRisk(), loadAlerts(), loadSources()]);
      window.setTimeout(() => elements.refreshButton.classList.remove("is-spinning"), 320);
    }

    function scheduleMapRefresh() {
      clearTimeout(state.refreshTimer);
      state.refreshTimer = setTimeout(() => {
        loadRisk();
        subscribeSocket();
      }, 260);
    }

    function formatScore(value) {
      return new Intl.NumberFormat("fr-FR", { minimumFractionDigits: 2, maximumFractionDigits: 2 }).format(Number(value));
    }

    function formatNumber(value, digits = 0) {
      return new Intl.NumberFormat("fr-FR", { maximumFractionDigits: digits }).format(Number(value));
    }

    function formatInteger(value) {
      return new Intl.NumberFormat("fr-FR", { maximumFractionDigits: 0 }).format(Number(value));
    }

    function formatDate(value) {
      if (!value) return "—";
      return new Intl.DateTimeFormat("fr-FR", {
        day: "2-digit", month: "short", hour: "2-digit", minute: "2-digit",
      }).format(new Date(value));
    }

    function relativeDate(value) {
      const seconds = Math.round((new Date(value).getTime() - Date.now()) / 1000);
      const formatter = new Intl.RelativeTimeFormat("fr-FR", { numeric: "auto" });
      if (Math.abs(seconds) < 60) return formatter.format(seconds, "second");
      const minutes = Math.round(seconds / 60);
      if (Math.abs(minutes) < 60) return formatter.format(minutes, "minute");
      const hours = Math.round(minutes / 60);
      if (Math.abs(hours) < 48) return formatter.format(hours, "hour");
      return formatter.format(Math.round(hours / 24), "day");
    }

    function escapeHtml(value) {
      return String(value)
        .replaceAll("&", "&amp;")
        .replaceAll("<", "&lt;")
        .replaceAll(">", "&gt;")
        .replaceAll('"', "&quot;")
        .replaceAll("'", "&#039;");
    }

    function showInfoTooltip(trigger) {
      const text = trigger.dataset.tooltip;
      if (!text) return;
      elements.tooltipPortal.textContent = text;
      elements.tooltipPortal.hidden = false;
      const triggerRect = trigger.getBoundingClientRect();
      const tooltipRect = elements.tooltipPortal.getBoundingClientRect();
      const gap = 10;
      const left = Math.min(
        window.innerWidth - tooltipRect.width - gap,
        Math.max(gap, triggerRect.left + triggerRect.width / 2 - tooltipRect.width / 2),
      );
      let top = triggerRect.top - tooltipRect.height - gap;
      if (top < gap) top = triggerRect.bottom + gap;
      elements.tooltipPortal.style.left = `${left}px`;
      elements.tooltipPortal.style.top = `${top}px`;
      trigger.setAttribute("aria-describedby", "tooltip-portal");
    }

    function hideInfoTooltip(trigger) {
      elements.tooltipPortal.hidden = true;
      trigger?.removeAttribute("aria-describedby");
    }

    root.addEventListener("pointerover", (event) => {
      const trigger = event.target.closest(".info-tip");
      if (trigger) showInfoTooltip(trigger);
    });
    root.addEventListener("pointerout", (event) => {
      const trigger = event.target.closest(".info-tip");
      if (trigger) hideInfoTooltip(trigger);
    });
    root.addEventListener("focusin", (event) => {
      const trigger = event.target.closest(".info-tip");
      if (trigger) showInfoTooltip(trigger);
    });
    root.addEventListener("focusout", (event) => {
      const trigger = event.target.closest(".info-tip");
      if (trigger) hideInfoTooltip(trigger);
    });
    root.addEventListener("click", (event) => {
      const trigger = event.target.closest(".info-tip");
      if (!trigger) {
        elements.tooltipPortal.hidden = true;
        return;
      }
      event.stopPropagation();
      showInfoTooltip(trigger);
    });

    elements.thresholdRange.addEventListener("input", () => {
      state.threshold = Number(elements.thresholdRange.value);
      elements.thresholdOutput.value = formatScore(state.threshold);
      elements.alertThresholdLabel.textContent = `Seuil ≥ ${formatScore(state.threshold)}`;
      clearTimeout(state.refreshTimer);
      state.refreshTimer = setTimeout(() => Promise.all([loadRisk(), loadAlerts()]), 220);
    });
    elements.horizonButtons.forEach((button) => {
      button.addEventListener("click", () => {
        state.horizon = button.dataset.horizon;
        elements.horizonButtons.forEach((candidate) => {
          candidate.classList.toggle("is-active", candidate === button);
          candidate.setAttribute("aria-pressed", String(candidate === button));
        });
        elements.activeHorizonLabel.textContent = HORIZON_LABELS[state.horizon].toUpperCase();
        closeDetail();
        refreshAll();
      });
    });
    elements.refreshButton.addEventListener("click", refreshAll);
    elements.drawerClose.addEventListener("click", closeDetail);
    root.addEventListener("keydown", (event) => {
      if (event.key === "Escape") closeDetail();
    });
    map.on("moveend", scheduleMapRefresh);

    elements.thresholdOutput.value = formatScore(state.threshold);
    elements.horizonButtons.forEach((button) => {
      button.setAttribute("aria-pressed", String(button.classList.contains("is-active")));
    });
    const controller = Object.freeze({
      refresh: refreshAll,
      resize: () => map.invalidateSize({ animate: false }),
      destroy: () => {
        if (state.destroyed) return;
        state.destroyed = true;
        state.riskController?.abort();
        clearTimeout(state.reconnectTimer);
        clearTimeout(state.refreshTimer);
        const socket = state.websocket;
        state.websocket = null;
        socket?.close();
        map.off();
        map.remove();
        INSTANCES.delete(mapElement);
      },
    });
    INSTANCES.set(mapElement, controller);

    loadConfig()
      .catch((error) => console.error("Configuration indisponible", error))
      .finally(() => {
        if (state.destroyed) return;
        refreshAll();
        connectWebSocket();
      });
    return controller;
  }

  window.ErytheonOperationalMap = Object.freeze({
    mount: mountOperationalMap,
  });

  let autoController = null;

  function autoMount() {
    if (document.querySelector("#map")) {
      autoController = mountOperationalMap({ root: document });
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", autoMount, { once: true });
  } else {
    autoMount();
  }
  window.addEventListener("pagehide", () => autoController?.destroy(), { once: true });
})();
