(() => {
  "use strict";

  const content = document.querySelector("#sci-content");
  const topClock = document.querySelector("#sci-top-clock");
  const topDate = document.querySelector("#sci-top-date");
  const navLinks = [...document.querySelectorAll(".sci-nav a[data-route]")];
  const navToggle = document.querySelector(".sci-nav-toggle");
  const navPanel = document.querySelector("#sci-nav-panel");
  const responseCache = new Map();
  let activeOperationalMap = null;
  let renderSequence = 0;

  const DEFINITIONS = Object.freeze({
    ap: "Average Precision : aire sous la courbe précision/rappel. Plus proche de 1, meilleur classement des positifs.",
    roc_auc: "ROC-AUC : capacité du modèle à classer un positif au-dessus d'un négatif au hasard. 0,5 = hasard, 1 = parfait.",
    brier: "Score de Brier : erreur quadratique moyenne entre score et étiquette réelle. Plus bas, mieux calibré.",
    ece: "Expected Calibration Error : écart moyen entre score prédit et fréquence réelle observée par tranche.",
    lift: "Lift : concentration des positifs dans le premier décile comparée à un tirage aléatoire.",
    strict: "Variante « strict » : n'inclut que les événements dont la cause humaine est certaine.",
    inclusive: "Variante « inclusive » : inclut aussi les événements de confiance moindre pour maximiser le rappel.",
    n2: "Fenêtre négative N2 : voisinage spatial de rayon 2 cellules H3 exclu des négatifs.",
    n3: "Fenêtre négative N3 : voisinage spatial de rayon 3 cellules H3 exclu des négatifs.",
    snapshot: "Photographie versionnée et checksumée d'un jeu de features à un instant donné.",
    checksum: "Empreinte cryptographique garantissant qu'une donnée n'a pas changé silencieusement.",
    h3: "Système de maillage géographique hexagonal utilisé pour indexer le territoire.",
    propension: "Propension relative : un classement de risque relatif, pas une probabilité absolue d'incendie.",
    modele_actif: "Le modèle actuellement utilisé pour servir les scores de risque en production.",
    modele_candidat: "Un modèle validé mais non branché au service : il ne produit aucun score en production.",
    shadow: "Calcul d'un score candidat en parallèle du modèle actif, sans jamais l'exposer.",
  });

  const OPEN_RISKS = [
    { level: "Élevé", factor: "Temporalité features", value: "snapshot courant", impact: "appliqué à tout l'historique" },
    { level: "Moyen", factor: "Combustibilité", value: "any(child)", impact: "sur-déclaration connue" },
    { level: "Moyen", factor: "Validation live", value: "absente", impact: "P3 non commencé" },
    { level: "Faible", factor: "Calendrier scolaire", value: "indisponible", impact: "variable non fournie" },
  ];

  const CAUSE_LABELS = Object.freeze({
    human_known: "Humain connu",
    natural_known: "Naturel connu",
    unknown: "Inconnu",
    indeterminate: "Indéterminé",
  });

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (character) => (
      { "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[character]
    ));
  }

  function fmtNum(value) {
    if (value === null || value === undefined || Number.isNaN(Number(value))) return "—";
    return new Intl.NumberFormat("fr-FR").format(Number(value));
  }

  function fmtPct(value, digits = 1) {
    if (value === null || value === undefined || Number.isNaN(Number(value))) return "—";
    return `${new Intl.NumberFormat("fr-FR", {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(Number(value) * 100)} %`;
  }

  function fmtDate(value) {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return String(value);
    return `${date.toISOString().replace("T", " ").slice(0, 19)} UTC`;
  }

  function fmtShortDate(value) {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return String(value);
    return new Intl.DateTimeFormat("fr-FR", {
      day: "2-digit",
      month: "short",
      hour: "2-digit",
      minute: "2-digit",
      timeZone: "UTC",
    }).format(date).replace(",", "");
  }

  function fmtTime(value) {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return "—";
    return date.toISOString().slice(11, 16);
  }

  function fmtAge(value) {
    if (!value) return "Non exposée";
    const elapsed = Date.now() - new Date(value).getTime();
    if (!Number.isFinite(elapsed) || elapsed < 0) return fmtShortDate(value);
    const minutes = Math.floor(elapsed / 60_000);
    if (minutes < 60) return `${minutes} min`;
    const hours = Math.floor(minutes / 60);
    if (hours < 48) return `${hours} h`;
    return `${Math.floor(hours / 24)} j`;
  }

  function statusTone(status) {
    const normalized = String(status ?? "").toLowerCase();
    if (/(ok|actif|active|valid|termin|success|finalized|operationnel|opérationnel|healthy)/.test(normalized)) return "ok";
    if (/(fail|erreur|bloqu|rejected|indisponible|aucune donnée)/.test(normalized)) return "danger";
    if (/(inactive|inactif|cours|pending|running|draft|degrad|dégrad)/.test(normalized)) return "warning";
    return "neutral";
  }

  function riskTone(level) {
    if (level === "Élevé") return "danger";
    if (level === "Moyen") return "warning";
    return "neutral";
  }

  function status(text, tone = statusTone(text)) {
    return `<span class="sci-status sci-status-${escapeHtml(tone)}"><i aria-hidden="true"></i>${escapeHtml(text ?? "—")}</span>`;
  }

  function def(key, label) {
    const text = DEFINITIONS[key];
    if (!text) return escapeHtml(label);
    return `<span class="sci-defterm" data-def="${escapeHtml(text)}" tabindex="0" aria-describedby="sci-tooltip-portal">${escapeHtml(label)}</span>`;
  }

  async function fetchJSON(url, options = {}) {
    if (!options.fresh && responseCache.has(url)) return responseCache.get(url);
    const request = (async () => {
      const response = await fetch(url, { headers: { Accept: "application/json" } });
      if (!response.ok) {
        let message = `HTTP ${response.status}`;
        try {
          const body = await response.json();
          if (body?.error?.message) message = body.error.message;
        } catch {
          /* Keep the status-based message when the error body is not JSON. */
        }
        throw new Error(message);
      }
      return response.json();
    })();
    responseCache.set(url, request);
    try {
      return await request;
    } catch (error) {
      responseCache.delete(url);
      throw error;
    }
  }

  function pageHeader(kicker, title, meta, aside = "Lecture directe") {
    return `<header class="sci-page-heading">
      <div>
        <span class="sci-eyebrow">${escapeHtml(kicker)}</span>
        <h1>${escapeHtml(title)}</h1>
      </div>
      <p>${escapeHtml(meta)}</p>
      <span class="sci-page-aside">${status(aside, "ok")}</span>
    </header>`;
  }

  function panelHeader(title, meta = "", aside = "") {
    return `<header class="sci-panel-head">
      <div>
        <h2>${escapeHtml(title)}</h2>
        ${meta ? `<p>${escapeHtml(meta)}</p>` : ""}
      </div>
      ${aside ? `<span>${escapeHtml(aside)}</span>` : ""}
    </header>`;
  }

  function metricStrip(cells) {
    return `<section class="sci-metric-strip" aria-label="Indicateurs essentiels">${cells.map((cell) => `
      <article class="sci-metric-tile sci-metric-${escapeHtml(cell.kind ?? "registry")}">
        <span>${escapeHtml(cell.label)}</span>
        <strong>${escapeHtml(cell.value)}</strong>
        <small>${cell.status ? status(cell.status, cell.tone) : escapeHtml(cell.detail ?? "")}</small>
      </article>`).join("")}
    </section>`;
  }

  function operationalMapPanel() {
    return `<section class="sci-panel sci-operational-map sci-primary-span-7" aria-labelledby="sci-map-title">
      <header class="sci-panel-head sci-map-panel-head">
        <div>
          <h2 id="sci-map-title">Risque opérationnel spatial</h2>
          <p><span id="territory-label">Territoire opérationnel</span> · maille H3 <span id="h3-resolution">—</span> · modèle v1</p>
        </div>
        <div class="sci-map-head-context">
          <div class="sci-map-connection" id="connection-status" role="status">
            <i aria-hidden="true"></i><span id="connection-label">Connexion…</span>
          </div>
          <span class="sci-map-validity"><span id="active-horizon-label">MAINTENANT</span> · <time id="horizon-valid-at">—</time></span>
        </div>
      </header>

      <div class="sci-map-command">
        <div class="sci-map-horizons">
          <span class="sci-map-control-label">Horizon</span>
          <div class="horizon-picker" role="group" aria-label="Échéance de prévision">
            <button type="button" class="is-active" data-horizon="nowcast">Maintenant</button>
            <button type="button" data-horizon="hours_6">+6 h</button>
            <button type="button" data-horizon="hours_24">+24 h</button>
            <button type="button" data-horizon="hours_48">+48 h</button>
          </div>
        </div>
        <div class="sci-map-threshold">
          <label for="threshold-range"><span class="sci-map-control-label">Seuil de risque</span><output id="threshold-output" for="threshold-range">0,10</output></label>
          <input id="threshold-range" type="range" min="0.01" max="0.80" step="0.01" value="0.10" aria-label="Seuil minimum du score">
        </div>
        <button class="sci-map-refresh" id="refresh-button" type="button" aria-label="Actualiser la carte">
          <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M20 11a8.1 8.1 0 0 0-14.9-4L3 9m0 0V4m0 5h5M4 13a8.1 8.1 0 0 0 14.9 4L21 15m0 0v5m0-5h-5"></path></svg>
        </button>
      </div>

      <div class="sci-map-stage">
        <div id="map" role="application" aria-label="Carte interactive du risque de départ de feu"></div>
        <div class="map-toolbar">
          <span class="map-chip"><i class="pulse-dot" aria-hidden="true"></i><span id="map-status">Synchronisation</span></span>
          <span class="map-chip map-chip-muted" id="map-cell-count">0 hexagone</span>
        </div>
        <div class="map-loading" id="map-loading" aria-live="polite">
          <span class="loader-ring" aria-hidden="true"></span>
          <span>Chargement des données…</span>
        </div>
        <div class="empty-state" id="empty-state" hidden>
          <span class="empty-icon" aria-hidden="true">∅</span>
          <strong>Aucune cellule à ce seuil</strong>
          <p>Réduisez le seuil ou actualisez la carte.</p>
        </div>

        <aside class="detail-drawer" id="detail-drawer" aria-label="Détail de la cellule" aria-hidden="true">
          <button class="drawer-close" id="drawer-close" type="button" aria-label="Fermer le détail">×</button>
          <div class="drawer-header">
            <p class="eyebrow">CELLULE SÉLECTIONNÉE</p>
            <h2 id="detail-h3">—</h2>
            <p id="detail-time">—</p>
          </div>
          <div class="score-overview">
            <div class="score-gauge" id="score-gauge"><div><strong id="detail-score">—</strong><span>score</span></div></div>
            <div class="component-bars">
              <div class="component-row">
                <span>Conditions feu</span><strong id="physical-value">—</strong>
                <div class="bar-track"><i id="physical-bar"></i></div>
              </div>
              <div class="component-row">
                <span>Exposition humaine</span><strong id="human-value">—</strong>
                <div class="bar-track"><i id="human-bar"></i></div>
              </div>
            </div>
          </div>
          <section class="detail-section">
            <h3>Indice forêt météo</h3>
            <div class="fwi-grid" id="fwi-grid"></div>
          </section>
          <section class="detail-section">
            <h3>Facteurs dominants</h3>
            <ol class="factor-list" id="factor-list"></ol>
          </section>
          <section class="detail-section">
            <div class="detail-section-heading"><h3>Évolution sur 24 h</h3><span id="history-count">—</span></div>
            <svg class="sparkline" id="history-chart" viewBox="0 0 320 84" role="img" aria-label="Évolution du score sur 24 heures"></svg>
          </section>
        </aside>
      </div>

      <div class="sci-map-facts" aria-label="Synthèse de la carte">
        <div><span>Score maximal</span><strong id="max-score">—</strong><small id="max-score-label">En attente</small></div>
        <div><span>Zones visibles</span><strong id="visible-cells">—</strong><small>au-dessus du seuil</small></div>
        <div><span>Alertes actives</span><strong id="alert-count">—</strong><small id="alert-threshold-label">Seuil ≥ 0,10</small></div>
        <div><span>Dernier calcul</span><strong><time id="last-update">—</time></strong><small>API opérationnelle v1</small></div>
      </div>

      <div class="sci-map-legend" aria-label="Légende du risque">
        <span><i class="risk-low"></i>Faible · 0–0,25</span>
        <span><i class="risk-moderate"></i>Modéré · 0,25–0,50</span>
        <span><i class="risk-high"></i>Élevé · 0,50–0,75</span>
        <span><i class="risk-critical"></i>Critique · 0,75–1</span>
      </div>

      <div class="sci-map-data-sinks" hidden>
        <span id="alerts-badge">0</span>
        <span id="alerts-loading">Chargement</span>
        <ol id="alert-list"></ol>
        <span id="source-summary">—</span>
        <ul id="source-list"></ul>
      </div>
      <div class="tooltip-portal" id="tooltip-portal" role="tooltip" hidden></div>
    </section>`;
  }

  function emptyState(title, detail) {
    return `<div class="sci-empty">
      <span aria-hidden="true"></span>
      <div><strong>${escapeHtml(title)}</strong>${detail ? `<p>${escapeHtml(detail)}</p>` : ""}</div>
    </div>`;
  }

  function table(columns, rows, renderRow, label = "Tableau de données", compact = false) {
    if (!rows || rows.length === 0) {
      return emptyState("Aucune donnée disponible", "L'API scientifique ne renvoie aucune ligne pour cette vue.");
    }
    return `<div class="sci-table-scroll" role="region" aria-label="${escapeHtml(label)}" tabindex="0">
      <table class="sci-table${compact ? " sci-table-compact" : ""}">
        <thead><tr>${columns.map((column) => `<th scope="col">${escapeHtml(column)}</th>`).join("")}</tr></thead>
        <tbody>${rows.map(renderRow).join("")}</tbody>
      </table>
    </div>`;
  }

  function definitionGrid(pairs) {
    return `<dl class="sci-definition-grid">${pairs.map((pair) => `
      <dt>${escapeHtml(pair.key)}</dt>
      <dd>${pair.html ? pair.html : escapeHtml(pair.value)}</dd>`).join("")}
    </dl>`;
  }

  function horizontalBars(rows, options = {}) {
    const max = options.max ?? Math.max(1, ...rows.map((row) => Number(row.value) || 0));
    return `<div class="sci-bars">${rows.map((row, index) => {
      const numeric = Number(row.value) || 0;
      const width = max > 0 ? Math.max(numeric > 0 ? 2 : 0, Math.min(100, (numeric / max) * 100)) : 0;
      return `<div class="sci-bar">
        <div><span><i class="sci-series-${(index % 4) + 1}" aria-hidden="true"></i>${escapeHtml(row.label)}</span><strong>${escapeHtml(row.display ?? fmtNum(numeric))}</strong></div>
        <span class="sci-bar-track" aria-hidden="true"><i style="width:${width.toFixed(2)}%"></i></span>
      </div>`;
    }).join("")}</div>`;
  }

  function donutChart(rows, label) {
    const normalized = rows.map((row) => ({ ...row, value: Math.max(0, Number(row.value) || 0) }));
    const total = normalized.reduce((sum, row) => sum + row.value, 0);
    if (total === 0) return emptyState("Répartition indisponible", "Aucune catégorie n'est exposée pour cette population.");
    let offset = 0;
    const segments = normalized.map((row, index) => {
      const percentage = (row.value / total) * 100;
      const segment = `<circle class="sci-donut-segment sci-series-${(index % 4) + 1}" cx="50" cy="50" r="38" pathLength="100" stroke-dasharray="${percentage.toFixed(4)} ${(100 - percentage).toFixed(4)}" stroke-dashoffset="${(-offset).toFixed(4)}"></circle>`;
      offset += percentage;
      return segment;
    }).join("");
    return `<div class="sci-donut-layout">
      <svg class="sci-donut" viewBox="0 0 100 100" role="img" aria-labelledby="sci-donut-title sci-donut-desc">
        <title id="sci-donut-title">${escapeHtml(label)}</title>
        <desc id="sci-donut-desc">${normalized.map((row) => `${row.label}: ${fmtNum(row.value)}`).join(", ")}</desc>
        <circle class="sci-donut-track" cx="50" cy="50" r="38"></circle>
        ${segments}
        <text x="50" y="47">${fmtNum(total)}</text>
        <text class="sci-donut-caption" x="50" y="59">événements</text>
      </svg>
      <ul class="sci-chart-legend">${normalized.map((row, index) => `
        <li><span><i class="sci-series-${(index % 4) + 1}" aria-hidden="true"></i>${escapeHtml(row.label)}</span><strong>${fmtPct(row.value / total)}</strong></li>`).join("")}
      </ul>
    </div>`;
  }

  function findSource(sources, pattern) {
    return sources.find((source) => pattern.test([
      source.id,
      source.provider,
      source.category,
      source.description,
    ].filter(Boolean).join(" ").toLowerCase()));
  }

  function journal(imports, pipelines, limit = 6) {
    const entries = [
      ...imports.map((item) => ({
        at: item.started_at,
        title: `Import ${item.source_code ?? item.batch_type}`,
        detail: `${fmtNum(item.records_inserted)} insérées · ${fmtNum(item.records_rejected)} rejetées`,
        state: item.status,
      })),
      ...pipelines.map((item) => ({
        at: item.started_at,
        title: item.pipeline_name,
        detail: item.error_message ?? "Exécution de pipeline",
        state: item.status,
      })),
    ].sort((a, b) => new Date(b.at) - new Date(a.at)).slice(0, limit);
    if (entries.length === 0) return emptyState("Aucune exécution récente", "Aucun import ou pipeline n'est exposé dans la fenêtre courante.");
    return `<ol class="sci-journal">${entries.map((entry) => `
      <li>
        <time datetime="${escapeHtml(entry.at)}">${fmtTime(entry.at)}</time>
        <i class="sci-dot sci-dot-${statusTone(entry.state)}" aria-hidden="true"></i>
        <div><strong>${escapeHtml(entry.title)}</strong><span>${escapeHtml(entry.detail)}</span></div>
      </li>`).join("")}
    </ol>`;
  }

  function modelComparison(comparison, compact = false) {
    const rows = [
      { key: "roc_auc", label: "ROC-AUC", v1: comparison.v1.roc_auc, candidate: comparison.candidate.roc_auc },
      { key: "ap", label: "Average Precision", v1: comparison.v1.average_precision, candidate: comparison.candidate.average_precision },
      { key: "lift", label: "Lift @ 10 %", v1: comparison.v1.lift_at_10pct, candidate: comparison.candidate.lift_at_10pct },
    ];
    return table(
      ["Métrique", "v1 actif", "Candidat inactif", "Écart"],
      rows,
      (row) => {
        const difference = Number(row.candidate) - Number(row.v1);
        const sign = difference > 0 ? "+" : "";
        return `<tr>
          <td>${def(row.key, row.label)}</td>
          <td>${escapeHtml(row.v1)}</td>
          <td>${escapeHtml(row.candidate)}</td>
          <td class="sci-diff-pos">${sign}${difference.toFixed(4)}</td>
        </tr>`;
      },
      "Comparaison des modèles",
      compact,
    );
  }

  function updateShellContext(overview, sources = []) {
    const weather = findSource(sources, /(open.?meteo|weather|forecast|météo|meteo)/i);
    const systemState = document.querySelector("#sci-system-state");
    const coreState = document.querySelector("#sci-core-state");
    const healthy = overview.app_status === "ok" && overview.db_status === "ok";
    systemState.innerHTML = `<i class="sci-dot sci-dot-${healthy ? "ok" : "danger"}" aria-hidden="true"></i>${healthy ? "Nominal" : "Dégradé"}`;
    document.querySelector("#sci-data-freshness").textContent = weather ? fmtAge(weather.last_success) : "Non exposée";
    document.querySelector("#sci-active-model").textContent = overview.active_model_id == null ? "Aucun" : `v1 · id ${overview.active_model_id}`;
    document.querySelector("#sci-candidate-state").textContent = overview.candidate_status ?? "Aucun";
    coreState.innerHTML = `<i aria-hidden="true"></i>${healthy ? "API scientifique opérationnelle" : "API scientifique dégradée"}`;
  }

  async function warmShellContext() {
    try {
      const [overview, sources] = await Promise.all([
        fetchJSON("/api/science/overview"),
        fetchJSON("/api/science/sources"),
      ]);
      updateShellContext(overview, sources);
    } catch {
      const systemState = document.querySelector("#sci-system-state");
      systemState.innerHTML = `<i class="sci-dot sci-dot-danger" aria-hidden="true"></i>Indisponible`;
      document.querySelector("#sci-core-state").innerHTML = `<i aria-hidden="true"></i>API scientifique indisponible`;
    }
  }

  const PAGES = {
    async overview() {
      const [data, sources, imports, pipelines, quality, events, models, system] = await Promise.all([
        fetchJSON("/api/science/overview"),
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/imports?limit=6"),
        fetchJSON("/api/science/pipelines?limit=6"),
        fetchJSON("/api/science/data-quality"),
        fetchJSON("/api/science/data-quality/events?limit=6"),
        fetchJSON("/api/science/models"),
        fetchJSON("/api/science/system"),
      ]);
      updateShellContext(data, sources);
      const weather = findSource(sources, /(open.?meteo|weather|forecast|météo|meteo)/i);
      const weatherPipeline = pipelines.find((pipeline) => /(open.?meteo|weather|forecast|météo|meteo)/i.test(pipeline.pipeline_name));
      const causeRows = quality.cause_counts.map((row) => ({
        label: CAUSE_LABELS[row.category] ?? row.category,
        value: row.count,
      }));
      const knownCauses = causeRows
        .filter((row) => ["Humain connu", "Naturel connu"].includes(row.label))
        .reduce((sum, row) => sum + Number(row.value), 0);
      const comparison = models.comparison;
      const comparable = comparison.population;

      return `
        ${pageHeader(
          "Mission control",
          "Vue d'ensemble",
          "État consolidé des registres scientifiques et opérationnels, sans écriture ni scoring candidat.",
          "Données réelles",
        )}
        ${metricStrip([
          {
            label: "Fraîcheur météo",
            value: weather ? fmtAge(weather.last_success) : "Non exposée",
            detail: weather ? `Dernier succès ${fmtShortDate(weather.last_success)}` : "Aucune source météo identifiée",
            kind: "operational",
          },
          { label: "Observations FIRMS", value: fmtNum(data.firms_observations_total), detail: fmtShortDate(system.last_firms_success), kind: "operational" },
          { label: "Événements BDIFF", value: fmtNum(data.bdiff_events_total), detail: "Registre actif", kind: "historical" },
          { label: "Cellules statiques", value: fmtNum(data.cell_static_total), detail: "Maillage territorial", kind: "historical" },
          { label: "Modèle actif", value: data.active_model_id == null ? "Aucun" : `v1 · ${data.active_model_id}`, status: "Actif", tone: "ok", kind: "serving" },
          { label: "Candidat", value: data.candidate_model_family ?? "Aucun", status: data.candidate_status ?? "Absent", tone: statusTone(data.candidate_status), kind: "candidate" },
        ])}

        <div class="sci-overview-grid">
          <div class="sci-overview-primary">
            ${operationalMapPanel()}

            <section class="sci-panel sci-interpretation-panel sci-primary-span-5">
              ${panelHeader("Facteurs d'interprétation", "Limites scientifiques actuellement documentées.", "4 ouverts")}
              ${table(
                ["Facteur", "État", "Impact"],
                OPEN_RISKS,
                (risk) => `<tr>
                  <td>${escapeHtml(risk.factor)}</td>
                  <td>${status(risk.value, riskTone(risk.level))}</td>
                  <td>${escapeHtml(risk.impact)}</td>
                </tr>`,
                "Facteurs d'interprétation",
                true,
              )}
              <p class="sci-panel-footnote">Température, humidité, vent, précipitations et indices FWI ne sont pas exposés ; aucune tendance n'est simulée.</p>
              <div class="sci-interpretation-context">
                <header>
                  <h3>Contexte opérationnel exposé</h3>
                  <span>Lecture seule</span>
                </header>
                <dl>
                  <div><dt>Source météo</dt><dd>${weather?.recent_error ? status("Erreur récente", "danger") : status(weather?.last_success ? "Disponible" : "Non exposée", weather?.last_success ? "ok" : "neutral")}</dd></div>
                  <div><dt>Dernier succès</dt><dd>${escapeHtml(weather ? fmtShortDate(weather.last_success) : "—")}</dd></div>
                  <div><dt>Pipeline météo</dt><dd>${weatherPipeline ? status(weatherPipeline.status) : status("Non exposé", "neutral")}</dd></div>
                  <div><dt>Surface active</dt><dd>Carte v1 · 4 horizons</dd></div>
                </dl>
                <nav aria-label="Accès aux analyses liées">
                  <a href="/science/sources">Examiner les sources <span aria-hidden="true">→</span></a>
                  <a href="/science/models">Vérifier les modèles <span aria-hidden="true">→</span></a>
                </nav>
              </div>
            </section>

            <section class="sci-panel sci-primary-span-7">
              ${panelHeader("Événements récents", "Derniers événements d'ignition actifs exposés par l'API.", `${fmtNum(events.length)} lignes`)}
              ${table(
                ["Date", "H3", "Cause", "Sous-catégorie", "Qualité géo."],
                events,
                (event) => `<tr>
                  <td>${escapeHtml(event.occurred_on_local)}</td>
                  <td class="sci-mono">${escapeHtml(event.h3)}</td>
                  <td>${escapeHtml(CAUSE_LABELS[event.cause_category] ?? event.cause_category)}</td>
                  <td>${escapeHtml(event.cause_subcategory)}</td>
                  <td>${escapeHtml(event.geographic_quality)}</td>
                </tr>`,
                "Événements d'ignition récents",
              )}
            </section>

            <section class="sci-panel sci-primary-span-5">
              ${panelHeader("Journal système", "Imports et pipelines les plus récents.", "UTC")}
              ${journal(imports, pipelines)}
            </section>

            <section class="sci-panel sci-primary-span-12">
              ${panelHeader("Comparaison des modèles", "Population commune du test 2025 ; le candidat reste inactif.", "Phase 3B.8")}
              <div class="sci-model-overview">
                <div class="sci-model-table">
                  ${modelComparison(comparison, true)}
                  <p>Gain AP candidat − v1 : <strong class="sci-diff-pos">+${escapeHtml(comparison.ap_diff_candidate_minus_v1)}</strong> · IC 95 % [${escapeHtml(comparison.ap_diff_95pct_ci[0])}, ${escapeHtml(comparison.ap_diff_95pct_ci[1])}]</p>
                </div>
                <div class="sci-mini-analysis">
                  <h3>Calibration</h3>
                  ${emptyState("Points non exposés", "Aucune courbe n'est reconstruite depuis l'artefact dans le navigateur.")}
                </div>
                <div class="sci-mini-analysis">
                  <h3>Population comparable</h3>
                  ${horizontalBars([
                    {
                      label: "Lignes comparables",
                      value: comparable.comparable_rows,
                      display: `${fmtNum(comparable.comparable_rows)} · ${fmtPct(comparable.comparable_fraction)}`,
                    },
                  ], { max: comparable.total_rows })}
                  <p>Population appariée exposée par le rapport versionné de Phase 3B.8.</p>
                </div>
              </div>
            </section>
          </div>

          <aside class="sci-overview-context" aria-label="Synthèse contextuelle">
            <section class="sci-panel">
              ${panelHeader("Synthèse du territoire", "Configuration réellement observée.")}
              ${definitionGrid([
                { key: "Maille", value: "H3" },
                { key: "Cellules statiques", value: fmtNum(data.cell_static_total) },
                { key: "Événements actifs", value: fmtNum(data.bdiff_events_total) },
                { key: "Snapshots publiés", value: fmtNum(data.feature_snapshots_total) },
                { key: "Dernière source BDIFF", value: fmtShortDate(system.last_bdiff_success) },
              ])}
            </section>

            <section class="sci-panel">
              ${panelHeader("Répartition des causes", "Population BDIFF active.")}
              ${donutChart(causeRows, "Répartition réelle des causes BDIFF")}
            </section>

            <section class="sci-panel">
              ${panelHeader("Qualité des données", "Aucun score global synthétique n'est calculé.")}
              ${horizontalBars([
                { label: "Causes documentées", value: knownCauses, display: `${fmtNum(knownCauses)} · ${fmtPct(knownCauses / Math.max(1, quality.bdiff_events_total))}` },
                { label: "Groupes coordonnées", value: quality.coordinate_groups_total },
                { label: "Paires doublons candidates", value: quality.duplicate_candidate_pairs_total },
              ], { max: quality.bdiff_events_total })}
            </section>

            <section class="sci-panel">
              ${panelHeader("Santé du système", "États directement vérifiables.")}
              <ul class="sci-health-list">
                <li><span>Application</span>${status(data.app_status === "ok" ? "Opérationnelle" : data.app_status)}</li>
                <li><span>PostgreSQL</span>${status(data.db_status === "ok" ? "Opérationnel" : data.db_status)}</li>
                <li><span>API scientifique</span>${status("Opérationnelle", "ok")}</li>
                <li><span>FIRMS</span>${status(system.last_firms_success ? "Succès observé" : "Indisponible")}</li>
                <li><span>Prévisions météo</span>${status(weather?.last_success ? fmtShortDate(weather.last_success) : "Non exposé", "neutral")}</li>
                <li><span>Modèle v1</span>${status(data.active_model_id == null ? "Absent" : "Actif")}</li>
                <li><span>Candidat</span>${status(data.candidate_status ?? "Absent")}</li>
                <li><span>Caddy</span>${status("Non exposé", "neutral")}</li>
              </ul>
            </section>
          </aside>
        </div>`;
    },

    async sources() {
      const [sources, imports, pipelines] = await Promise.all([
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/imports?limit=50"),
        fetchJSON("/api/science/pipelines?limit=50"),
      ]);
      const weather = findSource(sources, /(open.?meteo|weather|forecast|météo|meteo)/i);
      const firms = findSource(sources, /firms/i);
      return `
        ${pageHeader("Observabilité", "Sources et pipelines", "Fraîcheur, exécutions et erreurs réellement exposées.", "50 derniers runs")}
        ${metricStrip([
          { label: "Sources enregistrées", value: fmtNum(sources.length), detail: "source_status" },
          { label: "Fraîcheur météo", value: weather ? fmtAge(weather.last_success) : "Non exposée", detail: fmtShortDate(weather?.last_success) },
          { label: "Dernier lot FIRMS", value: fmtNum(firms?.observation_count), detail: fmtShortDate(firms?.last_success) },
          { label: "Imports listés", value: fmtNum(imports.length), detail: "Fenêtre courante" },
          { label: "Pipelines listés", value: fmtNum(pipelines.length), detail: "Fenêtre courante" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-12">
            ${panelHeader("Sources", "Dernière exécution, réussite, volume et erreur récente.", `${fmtNum(sources.length)} sources`)}
            ${table(
              ["Source", "Catégorie", "Fournisseur", "Dernier run", "Dernière réussite", "Observations", "Erreur récente"],
              sources,
              (source) => `<tr>
                <td>${escapeHtml(source.id)}</td><td>${escapeHtml(source.category ?? "—")}</td>
                <td>${escapeHtml(source.provider ?? "—")}</td><td>${fmtDate(source.last_run)}</td>
                <td>${fmtDate(source.last_success)}</td><td>${fmtNum(source.observation_count)}</td>
                <td>${escapeHtml(source.recent_error ?? "—")}</td>
              </tr>`,
              "État des sources",
            )}
          </section>
          <section class="sci-panel sci-span-7">
            ${panelHeader("Imports récents", "Volumes reçus, insérés et rejetés.", `${fmtNum(imports.length)} lignes`)}
            ${table(
              ["Batch", "Source", "Statut", "Début", "Reçues", "Insérées", "Rejetées"],
              imports,
              (item) => `<tr><td class="sci-mono">${escapeHtml(item.id.slice(0, 8))}</td><td>${escapeHtml(item.source_code ?? "—")}</td><td>${status(item.status)}</td><td>${fmtDate(item.started_at)}</td><td>${fmtNum(item.records_received)}</td><td>${fmtNum(item.records_inserted)}</td><td>${fmtNum(item.records_rejected)}</td></tr>`,
              "Imports récents",
            )}
          </section>
          <section class="sci-panel sci-span-5">
            ${panelHeader("Pipelines récents", "Statuts et erreurs déclarées.", `${fmtNum(pipelines.length)} lignes`)}
            ${table(
              ["Run", "Pipeline", "Statut", "Début", "Erreur"],
              pipelines,
              (run) => `<tr><td class="sci-mono">${escapeHtml(run.id.slice(0, 8))}</td><td>${escapeHtml(run.pipeline_name)}</td><td>${status(run.status)}</td><td>${fmtDate(run.started_at)}</td><td>${escapeHtml(run.error_message ?? "—")}</td></tr>`,
              "Pipelines récents",
            )}
          </section>
        </div>`;
    },

    async "data-quality"() {
      const [summary, events] = await Promise.all([
        fetchJSON("/api/science/data-quality"),
        fetchJSON("/api/science/data-quality/events?limit=50"),
      ]);
      const total = Math.max(1, summary.bdiff_events_total);
      const causeValue = (category) => summary.cause_counts.find((row) => row.category === category)?.count ?? 0;
      return `
        ${pageHeader("Audit scientifique", "Qualité des données", "Causes, géographie, doublons et combustibilité du registre BDIFF actif.", `${fmtNum(summary.bdiff_events_total)} événements`)}
        ${metricStrip([
          { label: "Humains connus", value: fmtNum(causeValue("human_known")), detail: fmtPct(causeValue("human_known") / total) },
          { label: "Naturels connus", value: fmtNum(causeValue("natural_known")), detail: fmtPct(causeValue("natural_known") / total) },
          { label: "Causes inconnues", value: fmtNum(causeValue("unknown")), detail: fmtPct(causeValue("unknown") / total) },
          { label: "Groupes coordonnées", value: fmtNum(summary.coordinate_groups_total), detail: "Audit géographique" },
          { label: "Paires doublons", value: fmtNum(summary.duplicate_candidate_pairs_total), detail: "Candidats" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-4">
            ${panelHeader("Répartition des causes", "Catégories du registre actif.")}
            ${donutChart(summary.cause_counts.map((row) => ({ label: CAUSE_LABELS[row.category] ?? row.category, value: row.count })), "Répartition des causes")}
          </section>
          <section class="sci-panel sci-span-4">
            ${panelHeader("Qualité géographique", "Catégories d'audit spatial.")}
            ${horizontalBars(summary.geographic_quality_counts.map((row) => ({ label: row.category, value: row.count })))}
          </section>
          <section class="sci-panel sci-span-4">
            ${panelHeader("Combustibilité", "Évaluation de la cellule d'origine.")}
            ${horizontalBars(summary.combustibility_counts.map((row) => ({ label: row.category, value: row.count })))}
          </section>
          <section class="sci-panel sci-span-12">
            ${panelHeader("Exploration des événements", "Échantillon récent en lecture seule.", `${fmtNum(events.length)} lignes`)}
            ${table(
              ["Date", "H3", "Cause", "Sous-catégorie", "Qualité géographique"],
              events,
              (event) => `<tr><td>${escapeHtml(event.occurred_on_local)}</td><td class="sci-mono">${escapeHtml(event.h3)}</td><td>${escapeHtml(CAUSE_LABELS[event.cause_category] ?? event.cause_category)}</td><td>${escapeHtml(event.cause_subcategory)}</td><td>${escapeHtml(event.geographic_quality)}</td></tr>`,
              "Exploration des événements BDIFF",
            )}
          </section>
        </div>`;
    },

    async features() {
      const data = await fetchJSON("/api/science/features");
      const calendar = data.calendar;
      return `
        ${pageHeader("Catalogue scientifique", "Features et snapshots", "Provenance, disponibilité historique et intégrité des bundles.", `${fmtNum(data.snapshots.length)} snapshots`)}
        ${metricStrip([
          { label: "Snapshots publiés", value: fmtNum(data.snapshots.length), detail: data.snapshots.length ? "Catalogue versionné" : "Aucun snapshot publié" },
          { label: "Jours calendrier", value: fmtNum(calendar.total_days), detail: calendar.total_days ? `${calendar.min_date} → ${calendar.max_date}` : "Historique non chargé" },
          { label: "Jours fériés", value: fmtNum(calendar.public_holiday_days), detail: "Calendrier actif" },
          { label: "Vacances connues", value: fmtNum(calendar.school_holiday_known_days), detail: "Jours qualifiés" },
          { label: "Vacances inconnues", value: fmtNum(calendar.school_holiday_unknown_days), detail: "Donnée indisponible" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-8">
            ${panelHeader("Catalogue de variables", "Famille, millésime, couverture et checksum logique.")}
            ${data.snapshots.length ? table(
              ["Famille", "Source", "Statut", "Temporalité", "Millésime", "Disponible depuis", "H3", "Cellules", "Checksum"],
              data.snapshots,
              (snapshot) => `<tr><td>${escapeHtml(snapshot.family)}</td><td>${escapeHtml(snapshot.source)}</td><td>${status(snapshot.status)}</td><td>${escapeHtml(snapshot.temporal_classification)}</td><td>${escapeHtml(snapshot.vintage ?? "—")}</td><td>${fmtDate(snapshot.available_from)}</td><td>${fmtNum(snapshot.h3_resolution)}</td><td>${fmtNum(snapshot.cell_count)}</td><td class="sci-mono">${escapeHtml(snapshot.logical_checksum.slice(0, 12))}…</td></tr>`,
              "Catalogue des snapshots",
            ) : emptyState("Aucun snapshot publié", "Le registre de snapshots est actuellement vide.")}
          </section>
          <aside class="sci-panel sci-span-4">
            ${panelHeader("Calendrier historique", "Disponibilité des variables calendaires.")}
            ${calendar.total_days ? definitionGrid([
              { key: "Période", value: `${calendar.min_date} → ${calendar.max_date}` },
              { key: "Jours couverts", value: fmtNum(calendar.total_days) },
              { key: "Jours fériés", value: fmtNum(calendar.public_holiday_days) },
              { key: "Vacances connues", value: fmtNum(calendar.school_holiday_known_days) },
              { key: "Vacances inconnues", value: fmtNum(calendar.school_holiday_unknown_days) },
              { key: "Checksum actif", html: `<span class="sci-mono">${escapeHtml(calendar.active_rule_checksum ?? "—")}</span>` },
            ]) : emptyState("Calendrier historique non chargé", "Aucun jour historique n'est enregistré dans l'environnement courant.")}
          </aside>
        </div>`;
    },

    async datasets() {
      const datasets = await fetchJSON("/api/science/datasets");
      const finalized = datasets.filter((dataset) => dataset.finalized_at).length;
      return `
        ${pageHeader("Registre expérimental", "Datasets", "Versions, paramètres d'échantillonnage et empreintes d'intégrité.", `${fmtNum(datasets.length)} versions`)}
        ${metricStrip([
          { label: "Versions enregistrées", value: fmtNum(datasets.length), detail: datasets.length ? "Registry scientifique" : "Aucune version enregistrée" },
          { label: "Versions finalisées", value: fmtNum(finalized), detail: "Finalized_at présent" },
          { label: "Variantes", value: fmtNum(new Set(datasets.map((dataset) => dataset.variant)).size), detail: "Valeurs distinctes" },
          { label: "Lignes cumulées", value: fmtNum(datasets.reduce((sum, dataset) => sum + Number(dataset.row_count ?? 0), 0)), detail: "Versions listées" },
        ])}
        <section class="sci-panel">
          ${panelHeader("Registre des versions", "Chaque ligne correspond à une version reproductible.")}
          ${datasets.length ? table(
            ["Nom logique", "Variante", "Statut", "Seed", "Positifs", "Négatifs", "Total", "Exclusions", "Checksum"],
            datasets,
            (dataset) => `<tr>
              <td><a href="/science/datasets/${encodeURIComponent(dataset.logical_id)}">${escapeHtml(dataset.name)}</a></td>
              <td>${escapeHtml(dataset.variant)}</td><td>${status(dataset.status)}</td><td class="sci-mono">${fmtNum(dataset.seed)}</td>
              <td>${fmtNum(dataset.positive_count)}</td><td>${fmtNum(dataset.negative_count)}</td><td>${fmtNum(dataset.row_count)}</td>
              <td>${fmtNum(dataset.exclusion_count)}</td><td class="sci-mono">${dataset.checksum ? `${escapeHtml(dataset.checksum.slice(0, 12))}…` : "—"}</td>
            </tr>`,
            "Registre des datasets",
          ) : emptyState("Aucune version enregistrée", "Le registre est vide : aucune version ni aucun build enregistré ; aucun statut n'est supposé.")}
        </section>`;
    },

    async "datasets/detail"(logicalId) {
      const detail = await fetchJSON(`/api/science/datasets/${encodeURIComponent(logicalId)}`);
      const summary = detail.summary;
      return `
        ${pageHeader("Fiche de validation", summary.name, summary.logical_id, summary.status)}
        ${metricStrip([
          { label: "Lignes", value: fmtNum(summary.row_count), detail: "Population totale" },
          { label: "Positifs", value: fmtNum(summary.positive_count), detail: "Label 1" },
          { label: "Négatifs", value: fmtNum(summary.negative_count), detail: "Label 0" },
          { label: "Exclusions", value: fmtNum(summary.exclusion_count), detail: "Lignes écartées" },
          { label: "Builds", value: fmtNum(detail.build_count), detail: "Exécutions enregistrées" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-4">
            ${panelHeader("Identité", "Paramètres et empreinte.")}
            ${definitionGrid([
              { key: "Statut", html: status(summary.status) },
              { key: "Variante", value: summary.variant },
              { key: "Seed", value: fmtNum(summary.seed) },
              { key: "Checksum", html: `<span class="sci-mono">${escapeHtml(summary.checksum ?? "—")}</span>` },
            ])}
          </section>
          <section class="sci-panel sci-span-4">
            ${panelHeader("Répartition par split", "Lignes par partition et label.")}
            ${detail.splits.length ? horizontalBars(detail.splits.map((row) => ({ label: `${row.split} · label ${row.label}`, value: row.count }))) : emptyState("Aucun split enregistré", "")}
          </section>
          <section class="sci-panel sci-span-4">
            ${panelHeader("Exclusions", "Catégories de lignes écartées.")}
            ${detail.exclusions.length ? horizontalBars(detail.exclusions.map((row) => ({ label: row.reason_category, value: row.count }))) : emptyState("Aucune exclusion enregistrée", "")}
          </section>
          <p class="sci-span-12"><a href="/science/datasets">← Retour au registre des datasets</a></p>
        </div>`;
    },

    async models() {
      const data = await fetchJSON("/api/science/models");
      const v1 = data.active_v1;
      const candidate = data.candidate;
      const comparison = data.comparison;
      return `
        ${pageHeader("Validation institutionnelle", "Modèles", "Le modèle v1 reste actif ; le candidat reste inactif et ne score aucun cas.", "Aucun scoring candidat")}
        ${metricStrip([
          { label: "Modèle actif", value: v1 ? `v1 · ${v1.id}` : "Aucun", status: v1 ? "Actif" : "Absent" },
          { label: "Candidat", value: candidate?.model_family ?? "Aucun", status: candidate?.status ?? "Absent" },
          { label: "ROC-AUC candidat", value: String(comparison.candidate.roc_auc), detail: `v1 ${comparison.v1.roc_auc}` },
          { label: "AP candidat", value: String(comparison.candidate.average_precision), detail: `v1 ${comparison.v1.average_precision}` },
          { label: "Lift @ 10 %", value: String(comparison.candidate.lift_at_10pct), detail: `v1 ${comparison.v1.lift_at_10pct}` },
          { label: "Promotion", value: "P0–P2", detail: "P3 non commencé" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-8">
            ${panelHeader("Comparaison métrique", "Population commune du test 2025.", "Phase 3B.8")}
            ${modelComparison(comparison)}
            <p class="sci-panel-footnote">Gain AP candidat − v1 : <strong class="sci-diff-pos">+${escapeHtml(comparison.ap_diff_candidate_minus_v1)}</strong> · IC 95 % [${escapeHtml(comparison.ap_diff_95pct_ci[0])}, ${escapeHtml(comparison.ap_diff_95pct_ci[1])}]</p>
          </section>
          <aside class="sci-panel sci-span-4">
            ${panelHeader("Calibration", "Les points détaillés ne sont pas exposés.")}
            ${emptyState("Diagramme indisponible", "La console ne reconstruit pas la calibration depuis l'artefact candidat.")}
          </aside>
          <section class="sci-panel sci-span-5">
            ${panelHeader("Modèle actif v1", "Artefact actuellement servi.")}
            ${v1 ? definitionGrid([
              { key: "ID", value: v1.id },
              { key: "Entraîné le", value: fmtDate(v1.trained_at) },
            ]) + `<pre class="sci-mono">${escapeHtml(JSON.stringify(v1.metrics, null, 2))}</pre>` : emptyState("Aucun modèle actif", "")}
          </section>
          <section class="sci-panel sci-span-7">
            ${panelHeader("Artefact candidat", "Identité vérifiable sans branchement au serving.")}
            ${candidate ? definitionGrid([
              { key: "Registry ID", value: candidate.id },
              { key: "Statut", html: status(candidate.status) },
              { key: "Famille", value: candidate.model_family },
              { key: "Nom", value: candidate.model_name },
              { key: "Version artefact", value: candidate.artifact_version },
              { key: "Seed", value: fmtNum(candidate.seed) },
              { key: "Commit", html: `<span class="sci-mono">${escapeHtml(candidate.git_commit)}</span>` },
              { key: "Dataset", html: `<span class="sci-mono">${escapeHtml(candidate.dataset_logical_id)}</span>` },
              { key: "Checksum", html: `<span class="sci-mono">${escapeHtml(candidate.artifact_checksum)}</span>` },
            ]) : emptyState("Aucun candidat enregistré", "")}
          </section>
          <section class="sci-panel sci-span-12 sci-limit-panel">
            ${panelHeader("Limites scientifiques", "Restrictions obligatoires d'interprétation.")}
            <ul>
              <li>Le score candidat est une ${def("propension", "propension relative")}, pas une probabilité absolue d'incendie.</li>
              <li>Le snapshot de features courant est appliqué uniformément à l'historique d'entraînement.</li>
              <li>La calibration est mesurée sur un dataset échantillonné, pas sur la population brute.</li>
              <li>La règle de combustibilité <code>any(child)</code> demeure une limite connue.</li>
              <li>Aucun ${def("shadow", "shadow scoring")} n'a été exécuté ; P3 n'est pas commencé.</li>
            </ul>
            <p>${escapeHtml(data.scientific_interpretation)}</p>
          </section>
        </div>`;
    },

    async system() {
      const [data, sources, overview] = await Promise.all([
        fetchJSON("/api/science/system"),
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/overview"),
      ]);
      updateShellContext(overview, sources);
      const checks = [
        { label: "Un seul modèle actif", state: data.active_model_count === 1 ? "Conforme" : "Échec" },
        { label: "Candidat inactif", state: overview.candidate_status === "inactive" ? "Conforme" : overview.candidate_status ?? "Absent" },
        { label: "Migrations sans échec", state: data.migrations_failed === 0 ? "Conforme" : "Échec" },
        { label: "Shadow scoring", state: "Non déployé" },
        { label: "Console", state: "Lecture seule" },
      ];
      return `
        ${pageHeader("Fiche technique", "Système et intégrité", "Schéma, registres scientifiques et invariants de lecture seule.", `${fmtNum(data.migrations_applied)} migrations`)}
        ${metricStrip([
          { label: "Migrations appliquées", value: fmtNum(data.migrations_applied), detail: `${fmtNum(data.migrations_failed)} échec` },
          { label: "Modèles actifs", value: fmtNum(data.active_model_count), detail: "v1 attendu" },
          { label: "Candidats enregistrés", value: fmtNum(data.candidate_registry_count), detail: overview.candidate_status ?? "Registry" },
          { label: "Cellules statiques", value: fmtNum(data.cell_static_total), detail: "Couverture territoriale" },
          { label: "Événements", value: fmtNum(data.ignition_events_total), detail: "Registre actif" },
          { label: "Versions dataset", value: fmtNum(data.dataset_versions_total), detail: "Registry scientifique" },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-8">
            ${panelHeader("Composants", "Santé logique et dernière réussite des sources.")}
            ${table(
              ["Composant", "État", "Détail"],
              [
                { name: "PostgreSQL / migrations", state: data.migrations_failed === 0 ? "Opérationnel" : "Échec", detail: `${fmtNum(data.migrations_applied)} appliquées, ${fmtNum(data.migrations_failed)} échouées` },
                { name: "Modèle actif", state: data.active_model_count === 1 ? "Actif" : "Dégradé", detail: `${fmtNum(data.active_model_count)} modèle actif` },
                { name: "Registry candidat", state: overview.candidate_status ?? "Absent", detail: `${fmtNum(data.candidate_registry_count)} candidat enregistré` },
                { name: "Cellules statiques", state: "Opérationnel", detail: `${fmtNum(data.cell_static_total)} cellules` },
                { name: "Dernier succès FIRMS", state: data.last_firms_success ? "Observé" : "Indisponible", detail: fmtDate(data.last_firms_success) },
                { name: "Dernier succès BDIFF", state: data.last_bdiff_success ? "Observé" : "Indisponible", detail: fmtDate(data.last_bdiff_success) },
                { name: "Caddy", state: "Non exposé", detail: "Hors contrat de l'API scientifique" },
              ],
              (row) => `<tr><td>${escapeHtml(row.name)}</td><td>${status(row.state)}</td><td>${escapeHtml(row.detail)}</td></tr>`,
              "Composants système",
            )}
          </section>
          <aside class="sci-panel sci-span-4">
            ${panelHeader("Intégrité", "Contrôles de sécurité scientifique.")}
            <ul class="sci-health-list">${checks.map((check) => `<li><span>${escapeHtml(check.label)}</span>${status(check.state, check.state === "Conforme" ? "ok" : "neutral")}</li>`).join("")}</ul>
          </aside>
          <section class="sci-panel sci-span-12">
            ${panelHeader("Sources", "État opérationnel exposé par source_status.", `${fmtNum(sources.length)} sources`)}
            ${table(
              ["Source", "Dernier run", "Dernière réussite", "Observations", "Erreur"],
              sources,
              (source) => `<tr><td>${escapeHtml(source.id)}</td><td>${fmtDate(source.last_run)}</td><td>${fmtDate(source.last_success)}</td><td>${fmtNum(source.observation_count)}</td><td>${escapeHtml(source.recent_error ?? "—")}</td></tr>`,
              "Sources système",
            )}
          </section>
        </div>`;
    },

    async observability() {
      const [overview, sources, latest, history, compare, snapshots, alerts, attempts, hourlySummary, labelSummary] = await Promise.all([
        fetchJSON("/api/science/overview"),
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/observability/latest").catch(() => null),
        fetchJSON("/api/science/observability/history?days=30").catch(() => []),
        fetchJSON("/api/science/observability/compare?days=1,7").catch(() => []),
        fetchJSON("/api/science/snapshots?limit=20").catch(() => []),
        fetchJSON("/api/science/snapshot-alerts?limit=20").catch(() => []),
        fetchJSON("/api/science/observability/attempts?days=30").catch(() => []),
        fetchJSON("/api/science/observability/hourly-summary").catch(() => ({ present_slots: 0, expected_slots: 0, missing_slots: 0, failed_attempts: 0 })),
        fetchJSON("/api/science/snapshot-labels/summary").catch(() => ({ total: 0, human_known: 0, natural_known: 0, unknown_or_indeterminate: 0, mature: 0, provisional: 0 })),
      ]);
      updateShellContext(overview, sources);

      if (!latest) {
        return `
          ${pageHeader("Observabilité", "Snapshots automatisés", "Mémoire opérationnelle et scientifique du système, phase 4A.5.", "Lecture directe")}
          ${emptyState("Aucun snapshot capturé", "Le job planifié (quotidien 02:15 UTC) ou la commande `snapshot-operational` n'a pas encore produit de capture.")}`;
      }

      const freshnessRow = (labelText, ageSeconds) => {
        if (ageSeconds === null || ageSeconds === undefined) return status("Indisponible", "danger");
        const hours = ageSeconds / 3600;
        const tone = hours < 3 ? "ok" : hours < 6 ? "neutral" : hours < 12 ? "warning" : "danger";
        return status(`${labelText} · ${hours < 1 ? `${Math.round(ageSeconds / 60)} min` : `${hours.toFixed(1)} h`}`, tone);
      };

      const compareRows = compare.flatMap((report) => (
        report.available
          ? report.entries.map((entry) => ({ ...entry, days_ago: report.days_ago }))
          : [{ metric: `— aucune donnée à J-${report.days_ago} —`, days_ago: report.days_ago, current_value: null, previous_value: null, absolute_delta: null, relative_delta: null, status: "not_comparable" }]
      ));

      return `
        ${pageHeader("Observabilité", "Snapshots automatisés", "Mémoire temporelle durcie, phase 4A.6. Lecture seule ; aucun entraînement, scoring ou activation.", `${fmtNum(history.length)} fenêtres (30 j)`)}
        ${metricStrip([
          { label: "Fraîcheur forecast", value: latest.forecast_age_seconds === null ? "—" : fmtAge(latest.forecast_last_complete_at), status: latest.forecast_age_seconds === null ? "Indisponible" : "Mesurée", tone: latest.forecast_age_seconds === null ? "danger" : "ok" },
          { label: "Fraîcheur FIRMS", value: latest.firms_age_seconds === null ? "—" : fmtAge(latest.firms_last_success_at), status: latest.firms_age_seconds === null ? "Indisponible" : "Mesurée", tone: latest.firms_age_seconds === null ? "danger" : "ok" },
          { label: "Modèles actifs", value: fmtNum(latest.active_model_count), detail: "1 attendu" },
          { label: "Candidat", value: latest.candidate_status ?? "—", detail: "jamais actif" },
          { label: "Erreurs (24 h)", value: fmtNum(latest.error_count_24h), detail: `${fmtNum(latest.warning_count_24h)} avertissements` },
          { label: "Créneaux horaires", value: `${fmtNum(hourlySummary.present_slots)} / ${fmtNum(hourlySummary.expected_slots)}`, detail: `${fmtNum(hourlySummary.missing_slots)} manquants` },
        ])}
        <div class="sci-page-grid">
          <section class="sci-panel sci-span-6">
            ${panelHeader("Dernière fenêtre", `${fmtDate(latest.capture_window_start)} → ${fmtDate(latest.capture_window_end)} · empreinte ${latest.checksum.slice(0, 12)}…`)}
            <ul class="sci-health-list">
              <li><span>${def("checksum", "Fraîcheur forecast")}</span>${freshnessRow("forecast", latest.forecast_age_seconds)}</li>
              <li><span>Fraîcheur FIRMS</span>${freshnessRow("FIRMS", latest.firms_age_seconds)}</li>
              <li><span>Application</span>${status(latest.application_healthy ? "Saine" : "Dégradée", latest.application_healthy ? "ok" : "danger")}</li>
              <li><span>PostgreSQL</span>${status(latest.database_healthy ? "Saine" : "Dégradée", latest.database_healthy ? "ok" : "danger")}</li>
              <li><span>Caddy</span>${status(latest.caddy_state)}</li>
              <li><span>Migrations</span>${status(`${fmtNum(latest.migrations_applied)} appliquées, ${fmtNum(latest.migrations_failed)} échouées`, latest.migrations_failed ? "danger" : "ok")}</li>
              <li><span>Shadow scoring</span>${status(latest.shadow_scoring_enabled ? "Actif (anomalie)" : "Non déployé", latest.shadow_scoring_enabled ? "danger" : "ok")}</li>
            </ul>
          </section>
          <section class="sci-panel sci-span-6">
            ${panelHeader("Comparaison temporelle", "Aujourd'hui vs. J-1 et J-7 ; jamais de pourcentage sur dénominateur nul.")}
            ${table(
              ["Métrique", "J-N", "Actuel", "Précédent", "Écart"],
              compareRows,
              (row) => `<tr><td>${escapeHtml(row.metric)}</td><td>J-${row.days_ago}</td><td>${fmtNum(row.current_value)}</td><td>${fmtNum(row.previous_value)}</td><td>${row.absolute_delta === null ? "—" : `${row.absolute_delta > 0 ? "+" : ""}${fmtNum(row.absolute_delta)}${row.relative_delta === null ? "" : ` (${row.relative_delta > 0 ? "+" : ""}${row.relative_delta.toFixed(1)} %)`}`}</td></tr>`,
              "Comparaison J-1/J-7",
              true,
            )}
          </section>
          <section class="sci-panel sci-span-7">
            ${panelHeader("Snapshots scientifiques", "Contrat v2 : bundle statique, provenance et couverture obligatoires. Les captures v1 restent signalées legacy.", `${fmtNum(snapshots.length)} manifestes`)}
            ${table(
              ["Date", "Contrat", "Cellules", "Modélisable", "Exclusions", "Manquants inattendus", "Provenance", "Complétude", "Statut"],
              snapshots,
              (snap) => `<tr>
                <td>${fmtShortDate(snap.valid_at)}</td><td>v${fmtNum(snap.contract_version)}</td>
                <td>${fmtNum(snap.cell_count_present)} / ${fmtNum(snap.cell_count_expected)}</td>
                <td>${fmtNum(snap.modelable_cell_count)}</td><td>${fmtNum(snap.structural_exclusion_count)}</td><td>${fmtNum(snap.unexpected_missing_count)}</td>
                <td title="révision ${escapeHtml(snap.application_revision ?? "Non disponible")} · batch ${escapeHtml(snap.forecast_batch_computed_at ?? "Non disponible")}">${snap.traceability_status === "complete" ? status("Complète", "ok") : status("Legacy · incomplète", "warning")}</td>
                <td>${status(snap.completeness_status, snap.completeness_status === "complete" ? "ok" : "warning")}</td>
                <td>${status(snap.status)}</td>
              </tr>`,
              "Snapshots scientifiques",
            )}
          </section>
          <section class="sci-panel sci-span-7">
            ${panelHeader("Tentatives de capture", "Chaque exécution est conservée, y compris les replays et échecs.", `${fmtNum(attempts.length)} tentatives`)}
            ${table(
              ["Début", "Cadence", "Fenêtre", "Tentative", "Origine", "Statut"],
              attempts,
              (attempt) => `<tr><td>${fmtDate(attempt.started_at)}</td><td>${escapeHtml(attempt.cadence)}</td><td>${fmtDate(attempt.capture_window_start)}</td><td>${fmtNum(attempt.attempt_number)}</td><td>${escapeHtml(attempt.trigger_kind)}</td><td>${status(attempt.status, attempt.status === "failed" ? "danger" : "ok")}</td></tr>`,
              "Tentatives de capture",
              true,
            )}
          </section>
          <aside class="sci-panel sci-span-5">
            ${panelHeader("Labels différés BDIFF", "Liens versionnés ; aucune absence ou observation FIRMS n'est transformée en label.")}
            ${labelSummary.total ? `<ul class="sci-health-list">
              <li><span>Total courant</span><strong>${fmtNum(labelSummary.total)}</strong></li>
              <li><span>Humains connus</span><strong>${fmtNum(labelSummary.human_known)}</strong></li>
              <li><span>Naturels connus</span><strong>${fmtNum(labelSummary.natural_known)}</strong></li>
              <li><span>Inconnus / indéterminés exclus</span><strong>${fmtNum(labelSummary.unknown_or_indeterminate)}</strong></li>
              <li><span>Matures / provisoires</span><strong>${fmtNum(labelSummary.mature)} / ${fmtNum(labelSummary.provisional)}</strong></li>
            </ul>` : emptyState("Aucun label différé relié", "Le linker reste en dry-run jusqu'à validation explicite.")}
          </aside>
          <aside class="sci-panel sci-span-5">
            ${panelHeader("Alertes", "Règles versionnées ; enregistrement et affichage seulement.", `${fmtNum(alerts.length)} alertes`)}
            ${table(
              ["Date", "Niveau", "Règle", "Message"],
              alerts,
              (alert) => `<tr><td>${fmtShortDate(alert.detected_at)}</td><td>${status(alert.severity, alert.severity === "critical" ? "danger" : alert.severity === "warning" ? "warning" : "neutral")}</td><td class="sci-mono">${escapeHtml(alert.rule_id)}</td><td>${escapeHtml(alert.message)}</td></tr>`,
              "Alertes d'observabilité",
              true,
            )}
          </aside>
        </div>`;
    },

    async progress() {
      const phases = await fetchJSON("/api/science/progress");
      const productionPhases = phases.filter((phase) => phase.production_affected).length;
      return `
        ${pageHeader("Journal scientifique", "Progression du projet", "Historique versionné des phases, validations et effets de production.", `${fmtNum(phases.length)} phases`)}
        ${metricStrip([
          { label: "Phases documentées", value: fmtNum(phases.length), detail: "Catalogue versionné" },
          { label: "Production affectée", value: fmtNum(productionPhases), detail: "Phases déclarées" },
          { label: "Phase courante", value: "4A.4", detail: "Redesign frontend" },
          { label: "Shadow scoring", value: "Non commencé", detail: "P3" },
        ])}
        <section class="sci-panel">
          ${panelHeader("Registre des phases", "Source versionnée ; aucune chronologie fictive.", "Traçabilité")}
          ${table(
            ["Phase", "Intitulé", "Statut", "Commits", "Environnement", "Production", "Résultat"],
            phases,
            (phase) => `<tr>
              <td class="sci-mono">${escapeHtml(phase.label)}</td><td>${escapeHtml(phase.title)}</td><td>${status(phase.status)}</td>
              <td class="sci-mono">${phase.commits?.length ? phase.commits.map(escapeHtml).join(", ") : "—"}</td>
              <td>${escapeHtml(phase.environment)}</td><td>${phase.production_affected ? "oui" : "non"}</td><td>${escapeHtml(phase.summary)}</td>
            </tr>${phase.risks?.length ? `<tr><td></td><td colspan="6" class="sci-risk-row">Risques ouverts : ${phase.risks.map(escapeHtml).join("; ")}</td></tr>` : ""}`,
            "Progression du projet",
          )}
        </section>`;
    },
  };

  function destroyOperationalMap() {
    activeOperationalMap?.destroy();
    activeOperationalMap = null;
  }

  function mountOperationalMap() {
    const mapRoot = content.querySelector(".sci-operational-map");
    if (!mapRoot) return;
    if (!window.ErytheonOperationalMap || !window.L) {
      const loading = mapRoot.querySelector("#map-loading");
      if (loading) loading.innerHTML = "<span>Bibliothèque cartographique indisponible.</span>";
      const connection = mapRoot.querySelector("#connection-label");
      if (connection) connection.textContent = "Carte indisponible";
      return;
    }
    activeOperationalMap = window.ErytheonOperationalMap.mount({
      root: mapRoot,
      presentation: "scientific",
    });
    requestAnimationFrame(() => {
      activeOperationalMap?.resize();
      window.setTimeout(() => activeOperationalMap?.resize(), 180);
    });
  }

  async function render(path) {
    const sequence = ++renderSequence;
    destroyOperationalMap();
    navLinks.forEach((link) => {
      const active = path.startsWith(link.getAttribute("data-route"));
      link.classList.toggle("is-active", active);
      if (active) link.setAttribute("aria-current", "page");
      else link.removeAttribute("aria-current");
    });
    document.body.dataset.scienceRoute = path.split("/")[0];
    content.innerHTML = `<div class="sci-loading"><span aria-hidden="true"></span>Chargement des observations…</div>`;
    try {
      const datasetMatch = path.match(/^datasets\/(.+)$/);
      let html;
      if (datasetMatch) html = await PAGES["datasets/detail"](decodeURIComponent(datasetMatch[1]));
      else if (PAGES[path]) html = await PAGES[path]();
      else html = emptyState("Page inconnue", "Cette route n'existe pas dans la console scientifique.");
      if (sequence !== renderSequence) return;
      content.innerHTML = html;
      if (path === "overview") mountOperationalMap();
      document.title = `ERYTHEON — ${content.querySelector("h1")?.textContent ?? "Console scientifique"}`;
    } catch (error) {
      if (sequence !== renderSequence) return;
      content.innerHTML = `<div class="sci-error" role="alert"><span aria-hidden="true"></span><div><strong>Erreur de chargement</strong><p>${escapeHtml(error.message)}</p></div></div>`;
    }
  }

  function currentRoute() {
    return location.pathname.replace(/^\/science\/?/, "") || "overview";
  }

  document.addEventListener("click", (event) => {
    const link = event.target.closest("a[href^='/science']");
    if (!link) return;
    event.preventDefault();
    history.pushState({}, "", link.getAttribute("href"));
    navToggle?.setAttribute("aria-expanded", "false");
    navToggle?.setAttribute("aria-label", "Ouvrir la navigation");
    navPanel?.classList.remove("is-open");
    render(currentRoute());
  });

  navToggle?.addEventListener("click", () => {
    const expanded = navToggle.getAttribute("aria-expanded") === "true";
    navToggle.setAttribute("aria-expanded", String(!expanded));
    navToggle.setAttribute("aria-label", expanded ? "Ouvrir la navigation" : "Fermer la navigation");
    navPanel.classList.toggle("is-open", !expanded);
  });

  window.addEventListener("popstate", () => render(currentRoute()));
  window.addEventListener("pagehide", destroyOperationalMap);

  const tooltip = document.querySelector("#sci-tooltip-portal");
  function showTooltip(term) {
    tooltip.textContent = term.dataset.def;
    tooltip.hidden = false;
    const rect = term.getBoundingClientRect();
    tooltip.style.left = `${Math.max(8, Math.min(rect.left, window.innerWidth - tooltip.offsetWidth - 8))}px`;
    const below = rect.bottom + 7;
    const above = rect.top - tooltip.offsetHeight - 7;
    tooltip.style.top = `${below + tooltip.offsetHeight < window.innerHeight - 8 ? below : Math.max(8, above)}px`;
  }

  function hideTooltip() {
    tooltip.hidden = true;
  }

  document.addEventListener("mouseover", (event) => {
    const term = event.target.closest(".sci-defterm");
    if (term) showTooltip(term);
  }, true);
  document.addEventListener("mouseout", (event) => {
    if (event.target.closest(".sci-defterm")) hideTooltip();
  }, true);
  document.addEventListener("focusin", (event) => {
    const term = event.target.closest(".sci-defterm");
    if (term) showTooltip(term);
  }, true);
  document.addEventListener("focusout", (event) => {
    if (event.target.closest(".sci-defterm")) hideTooltip();
  }, true);

  function updateClock() {
    const now = new Date();
    topClock.textContent = `${now.toISOString().slice(11, 19)} UTC`;
    topDate.textContent = new Intl.DateTimeFormat("fr-FR", {
      day: "2-digit",
      month: "short",
      year: "numeric",
      timeZone: "UTC",
    }).format(now);
  }

  updateClock();
  window.setInterval(updateClock, 30_000);
  warmShellContext();
  render(currentRoute());
})();
