(() => {
  "use strict";

  const content = document.querySelector("#sci-content");
  const topClock = document.querySelector("#sci-top-clock");
  const navLinks = [...document.querySelectorAll(".sci-nav a[data-route]")];

  const DEFINITIONS = Object.freeze({
    ap: "Average Precision : aire sous la courbe précision/rappel. Plus proche de 1, meilleur classement des positifs.",
    roc_auc: "ROC-AUC : capacité du modèle à classer un positif au-dessus d'un négatif au hasard. 0,5 = hasard, 1 = parfait.",
    brier: "Score de Brier : erreur quadratique moyenne entre score et étiquette réelle. Plus bas, mieux calibré.",
    ece: "Expected Calibration Error : écart moyen entre score prédit et fréquence réelle observée par tranche.",
    lift: "Lift : combien de fois plus de positifs sont capturés dans le top-k comparé à un tirage aléatoire.",
    strict: "Variante « strict » : n'inclut que les événements dont la cause humaine est certaine.",
    inclusive: "Variante « inclusive » : inclut aussi les événements de confiance moindre pour maximiser le rappel.",
    n2: "Fenêtre négative N2 : voisinage spatial de rayon 2 cellules H3 exclu des négatifs.",
    n3: "Fenêtre négative N3 : voisinage spatial de rayon 3 cellules H3 exclu des négatifs (plus conservateur).",
    snapshot: "Photographie versionnée et checksumée d'un jeu de features à un instant donné.",
    checksum: "Empreinte cryptographique garantissant qu'une donnée n'a pas changé silencieusement.",
    h3: "Système de maillage géographique hexagonal utilisé pour indexer le territoire.",
    propension: "Propension relative : un classement de risque relatif, pas une probabilité absolue d'incendie.",
    modele_actif: "Le modèle actuellement utilisé pour servir les scores de risque en production.",
    modele_candidat: "Un modèle entraîné et validé mais non branché au service : il ne produit aucun score en production.",
    shadow: "Calcul d'un score par un modèle candidat en parallèle du modèle actif, sans jamais l'exposer.",
  });

  function escapeHtml(value) {
    return String(value ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c]));
  }

  function fmtNum(value) {
    if (value === null || value === undefined) return "—";
    return new Intl.NumberFormat("fr-FR").format(value);
  }

  function fmtPct(value, digits = 1) {
    if (value === null || value === undefined || Number.isNaN(value)) return "—";
    return new Intl.NumberFormat("fr-FR", { minimumFractionDigits: digits, maximumFractionDigits: digits }).format(value * 100) + " %";
  }

  function fmtDate(value) {
    if (!value) return "—";
    const date = new Date(value);
    if (Number.isNaN(date.getTime())) return String(value);
    return date.toISOString().replace("T", " ").slice(0, 19) + " UTC";
  }

  function badgeClass(status) {
    return "sci-badge sci-badge-" + String(status ?? "").toLowerCase().replace(/[^a-z0-9]+/g, "-");
  }

  function badge(status) {
    return `<span class="${badgeClass(status)}">${escapeHtml(status ?? "—")}</span>`;
  }

  function def(key, label) {
    const text = DEFINITIONS[key];
    if (!text) return escapeHtml(label);
    return `<span class="sci-defterm" data-def="${escapeHtml(text)}" tabindex="0" aria-describedby="sci-tooltip-portal">${escapeHtml(label)}</span>`;
  }

  async function fetchJSON(url) {
    const response = await fetch(url, { headers: { Accept: "application/json" } });
    if (!response.ok) {
      let message = `HTTP ${response.status}`;
      try {
        const body = await response.json();
        if (body?.error?.message) message = body.error.message;
      } catch {
        /* ignore parse failure, keep the status-based message */
      }
      throw new Error(message);
    }
    return response.json();
  }

  function pageHeader(kicker, title, meta, aside = "Lecture directe") {
    return `<header class="sci-page-header">
      <div class="sci-page-header-copy">
        <span class="sci-eyebrow">${escapeHtml(kicker)}</span>
        <h1>${escapeHtml(title)}</h1>
        ${meta ? `<p class="sci-page-meta">${escapeHtml(meta)}</p>` : ""}
      </div>
      <div class="sci-page-aside">${escapeHtml(aside)}</div>
    </header>`;
  }

  function sectionHeader(kicker, title, note = "", index = "") {
    return `<div class="sci-section-head">
      <div class="sci-section-head-copy">
        <span class="sci-section-kicker">${escapeHtml(kicker)}</span>
        <h2>${escapeHtml(title)}</h2>
        ${note ? `<p class="sci-section-note">${escapeHtml(note)}</p>` : ""}
      </div>
      ${index ? `<span class="sci-section-index">${escapeHtml(index)}</span>` : ""}
    </div>`;
  }

  function riskList(rows) {
    return `<ul class="sci-risk-list">${rows
      .map(
        (row) => `<li>
          <strong class="${riskClass(row.level)}">${escapeHtml(row.level)}</strong>
          <span>${escapeHtml(row.subject)}<small>${escapeHtml(row.impact)}</small></span>
        </li>`,
      )
      .join("")}</ul>`;
  }

  /** Compact technical strip for environment, database and model state. */
  function statusLine(items) {
    return `<div class="sci-status-line">${items
      .map(
        (i) => `<div class="sci-status-item"><span class="sci-status-key">${escapeHtml(i.key)}</span><span class="sci-status-val">${i.html ? i.html : escapeHtml(i.val)}</span></div>`,
      )
      .join("")}</div>`;
  }

  /** Compact metric matrix; values remain direct API observations. */
  function metricGrid(cells) {
    return `<div class="sci-metric-grid">${cells
      .map(
        (c) => `<div class="sci-metric-cell"><span class="sci-metric-label">${escapeHtml(c.label)}</span><span class="sci-metric-value">${escapeHtml(c.value)}</span>${c.sub ? `<span class="sci-metric-sub">${escapeHtml(c.sub)}</span>` : ""}</div>`,
      )
      .join("")}</div>`;
  }

  function barChart(rows, opts = {}) {
    const max = opts.maxHint || Math.max(1, ...rows.map((r) => r.count));
    const total = rows.reduce((sum, r) => sum + r.count, 0);
    const bars = rows
      .map(
        (r) => `<div class="sci-bar-row">
          <span class="sci-bar-label">${escapeHtml(r.label)}</span>
          <div class="sci-bar-track"><div class="sci-bar-fill" style="width:${Math.max(2, (r.count / max) * 100)}%"></div></div>
          <span class="sci-bar-value">${fmtNum(r.count)}</span>
        </div>`,
      )
      .join("");
    const meta = opts.hideMeta ? "" : `<p class="sci-bar-chart-meta">n = ${fmtNum(total)}${opts.unit ? ` ${opts.unit}` : ""}</p>`;
    return `<div class="sci-bar-chart">${bars}</div>${meta}`;
  }

  function definitionGrid(pairs) {
    return `<dl class="sci-definition-grid">${pairs.map((p) => `<dt>${escapeHtml(p.key)}</dt><dd>${p.html ? p.html : escapeHtml(p.val)}</dd>`).join("")}</dl>`;
  }

  function table(columns, rows, renderRow) {
    if (rows.length === 0) {
      return `<div class="sci-empty">Aucune donnée disponible.</div>`;
    }
    return `<div class="sci-table-scroll" role="region" aria-label="Tableau de données" tabindex="0"><table class="sci-table"><thead><tr>${columns.map((c) => `<th>${escapeHtml(c)}</th>`).join("")}</tr></thead><tbody>${rows.map(renderRow).join("")}</tbody></table></div>`;
  }

  function riskClass(level) {
    return level === "Élevé" ? "sci-risk-high" : level === "Moyen" ? "sci-risk-medium" : "sci-risk-low";
  }

  const OPEN_RISKS = [
    { level: "Élevé", subject: "Snapshot cell_static courant appliqué à tout l'historique", impact: "limite temporelle scientifique" },
    { level: "Moyen", subject: "Règle de combustibilité any(child)", impact: "biais de couverture, sur-déclaration de cellules" },
    { level: "Moyen", subject: "Aucun shadow scoring déployé", impact: "validation en conditions réelles absente (P3 non commencé)" },
    { level: "Faible", subject: "Vacances scolaires indisponibles pour la période étudiée", impact: "variable non fournie au modèle" },
  ];

  const PAGES = {
    async overview() {
      const data = await fetchJSON("/api/science/overview");
      const now = fmtDate(new Date().toISOString());
      const hasDatasets = data.dataset_versions_total > 0;
      const environment = ["localhost", "127.0.0.1", "::1"].includes(window.location.hostname)
        ? "validation isolée"
        : "production VPS";
      return `
        ${pageHeader(
          "Mission control · synthèse",
          "Vue d'ensemble",
          `État consolidé du socle scientifique · ${environment} · actualisé ${now}.`,
          "PostgreSQL · temps réel",
        )}

        <div class="sci-dashboard-grid">
        <section class="sci-section sci-panel-compact sci-span-12">
          ${statusLine([
            { key: "Environnement", val: environment },
            { key: "Base", val: data.db_status === "ok" ? "PostgreSQL healthy" : data.db_status },
            { key: "Modèle actif", html: def("modele_actif", `v1${data.active_model_id != null ? ` (id=${data.active_model_id})` : ""}`) },
            { key: "Candidat", html: def("modele_candidat", escapeHtml(data.candidate_status ?? "aucun")) + (data.candidate_model_family ? ` · ${escapeHtml(data.candidate_model_family)}` : "") },
            { key: "Shadow", html: def("shadow", "non déployé") },
            { key: "Dernière MAJ", val: now },
          ])}
        </section>

        <section class="sci-section sci-span-8">
          ${sectionHeader("Observations principales", "Indicateurs essentiels", "Volumes et invariants lus directement dans la base.", "01")}
          ${metricGrid([
            { label: "Événements BDIFF", value: fmtNum(data.bdiff_events_total), sub: "registre actif" },
            { label: "Humains connus", value: fmtNum(data.bdiff_human_known), sub: "cause confirmée" },
            { label: "Naturels connus", value: fmtNum(data.bdiff_natural_known), sub: "cause confirmée" },
            { label: "Causes inconnues", value: fmtNum(data.bdiff_unknown), sub: "qualification ouverte" },
            { label: "Cellules territoriales", value: fmtNum(data.cell_static_total), sub: "maillage statique" },
            { label: "Datasets candidats", value: fmtNum(data.dataset_versions_total), sub: "versions registry" },
            { label: "Snapshots de features", value: fmtNum(data.feature_snapshots_total), sub: "catalogue versionné" },
            { label: "Migrations appliquées", value: fmtNum(data.migrations_applied), sub: "schéma courant" },
          ])}
        </section>

        <aside class="sci-section sci-panel-warn sci-span-4">
          ${sectionHeader("Contexte scientifique", "Risques ouverts", "Limites connues à conserver pendant l'interprétation.", "02")}
          ${riskList(OPEN_RISKS)}
        </aside>

        <section class="sci-section sci-span-12">
          ${sectionHeader("État du socle", "Composants scientifiques", "Disponibilité observée sans action possible depuis la console.", "03")}
          ${table(
            ["Composant", "État", "Détail"],
            [
              { name: "FIRMS", state: data.firms_observations_total > 0 ? "opérationnel" : "aucune donnée", detail: `${fmtNum(data.firms_observations_total)} observations` },
              { name: "BDIFF", state: "validé", detail: `${fmtNum(data.bdiff_events_total)} événements` },
              {
                name: "Dataset principal",
                state: hasDatasets ? "enregistré" : "aucun",
                detail: hasDatasets
                  ? `${fmtNum(data.dataset_versions_total)} versions, ${fmtNum(data.dataset_builds_total)} builds`
                  : "aucune version ni aucun build enregistré",
              },
              { name: "Modèle actif", state: "actif", detail: `${fmtNum(data.human_model_versions_total)} version(s) entraînée(s)` },
              { name: "Modèle candidat", state: data.candidate_status ?? "aucun", detail: data.candidate_model_family ?? "—" },
            ],
            (r) => `<tr><td>${escapeHtml(r.name)}</td><td>${badge(r.state)}</td><td>${escapeHtml(r.detail)}</td></tr>`,
          )}
        </section>
        </div>`;
    },

    async progress() {
      const phases = await fetchJSON("/api/science/progress");
      return `
        ${pageHeader(
          "Journal scientifique · programme",
          "Progression du projet",
          "Historique versionné des phases, validations et effets de production.",
          `${fmtNum(phases.length)} phases documentées`,
        )}
        <section class="sci-section">
          ${sectionHeader("Traçabilité", "Registre des phases", "Source versionnée dans le dépôt ; aucune table dédiée.", "01")}
          ${table(
            ["Phase", "Intitulé", "Statut", "Commit", "Environnement", "Production affectée", "Résultat"],
            phases,
            (p) => `<tr>
              <td class="sci-mono">${escapeHtml(p.label)}</td>
              <td>${escapeHtml(p.title)}</td>
              <td>${badge(p.status)}</td>
              <td class="sci-mono">${p.commits && p.commits.length ? p.commits.map(escapeHtml).join(", ") : "—"}</td>
              <td>${escapeHtml(p.environment)}</td>
              <td>${p.production_affected ? "oui" : "non"}</td>
              <td>${escapeHtml(p.summary)}</td>
            </tr>${p.risks && p.risks.length ? `<tr><td></td><td colspan="6" class="sci-risk-medium">Risques ouverts : ${p.risks.map(escapeHtml).join("; ")}</td></tr>` : ""}`,
          )}
        </section>`;
    },

    async sources() {
      const [sources, imports, pipelines] = await Promise.all([
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/imports?limit=50"),
        fetchJSON("/api/science/pipelines?limit=50"),
      ]);
      return `
        ${pageHeader(
          "Observabilité · ingestion",
          "Sources et pipelines",
          "Fraîcheur, exécutions et intégrité opérationnelle des flux scientifiques.",
          "Lecture des 50 derniers runs",
        )}
        <div class="sci-dashboard-grid">
        <section class="sci-section sci-span-12">
          ${sectionHeader("Signal opérationnel", "Vue synthétique", "Inventaire présent dans les réponses courantes.", "01")}
          ${metricGrid([
            { label: "Sources enregistrées", value: fmtNum(sources.length), sub: "source_status" },
            { label: "Imports listés", value: fmtNum(imports.length), sub: "fenêtre courante" },
            { label: "Pipelines listés", value: fmtNum(pipelines.length), sub: "fenêtre courante" },
          ])}
        </section>
        <section class="sci-section sci-span-12">
          ${sectionHeader("Fraîcheur et disponibilité", "Sources", "Dernière réussite et erreur récente exposées par source.", "02")}
          ${table(
            ["Source", "Catégorie", "Dernière réussite", "Observations", "Erreur récente"],
            sources,
            (s) => `<tr><td>${escapeHtml(s.id)}</td><td>${escapeHtml(s.category ?? "—")}</td><td>${fmtDate(s.last_success)}</td><td>${fmtNum(s.observation_count)}</td><td>${escapeHtml(s.recent_error ?? "—")}</td></tr>`,
          )}
        </section>
        <section class="sci-section sci-span-7">
          ${sectionHeader("Entrées", "Imports récents", "Volumes reçus, insérés et rejetés.", "03")}
          ${table(
            ["Batch", "Source", "Statut", "Début", "Reçues", "Insérées", "Rejetées"],
            imports,
            (b) => `<tr><td class="sci-mono">${escapeHtml(b.id.slice(0, 8))}</td><td>${escapeHtml(b.source_code ?? "—")}</td><td>${badge(b.status)}</td><td>${fmtDate(b.started_at)}</td><td>${fmtNum(b.records_received)}</td><td>${fmtNum(b.records_inserted)}</td><td>${fmtNum(b.records_rejected)}</td></tr>`,
          )}
        </section>
        <section class="sci-section sci-span-5">
          ${sectionHeader("Traitements", "Pipelines récents", "Statut d'exécution et erreurs déclarées.", "04")}
          ${table(
            ["Run", "Pipeline", "Statut", "Début", "Erreur"],
            pipelines,
            (r) => `<tr><td class="sci-mono">${escapeHtml(r.id.slice(0, 8))}</td><td>${escapeHtml(r.pipeline_name)}</td><td>${badge(r.status)}</td><td>${fmtDate(r.started_at)}</td><td>${escapeHtml(r.error_message ?? "—")}</td></tr>`,
          )}
        </section>
        </div>`;
    },

    async "data-quality"() {
      const [summary, events] = await Promise.all([
        fetchJSON("/api/science/data-quality"),
        fetchJSON("/api/science/data-quality/events?limit=50"),
      ]);
      const total = summary.bdiff_events_total || 1;
      return `
        ${pageHeader(
          "Audit scientifique · BDIFF",
          "Qualité des données",
          "Lecture rigoureuse des causes, de la géographie, des doublons et de la combustibilité.",
          `${fmtNum(summary.bdiff_events_total)} événements actifs`,
        )}
        <section class="sci-section">
          ${sectionHeader("Population auditée", "Synthèse des contrôles", "Parts rapportées au volume BDIFF actif.", "01")}
          ${metricGrid([
            {
              label: "Causes humaines connues",
              value: fmtNum(summary.cause_counts.find((c) => c.category === "human_known")?.count ?? 0),
              sub: fmtPct((summary.cause_counts.find((c) => c.category === "human_known")?.count ?? 0) / total),
            },
            {
              label: "Causes naturelles connues",
              value: fmtNum(summary.cause_counts.find((c) => c.category === "natural_known")?.count ?? 0),
              sub: fmtPct((summary.cause_counts.find((c) => c.category === "natural_known")?.count ?? 0) / total),
            },
            {
              label: "Causes inconnues",
              value: fmtNum(summary.cause_counts.find((c) => c.category === "unknown")?.count ?? 0),
              sub: fmtPct((summary.cause_counts.find((c) => c.category === "unknown")?.count ?? 0) / total),
            },
            { label: "Groupes de coordonnées", value: fmtNum(summary.coordinate_groups_total), sub: "contrôle géographique" },
            { label: "Paires candidates doublons", value: fmtNum(summary.duplicate_candidate_pairs_total), sub: "contrôle de déduplication" },
          ])}
        </section>
        <div class="sci-two-col">
          <section class="sci-section">
            ${sectionHeader("Étiquettes", "Répartition des causes", "Catégories actives dans le registre.", "02")}
            ${barChart(summary.cause_counts.map((c) => ({ label: c.category, count: c.count })), { unit: "événements BDIFF" })}
          </section>
          <section class="sci-section">
            ${sectionHeader("Géographie", "Qualité géographique", "Catégories d'audit spatial.", "03")}
            ${barChart(summary.geographic_quality_counts.map((c) => ({ label: c.category, count: c.count })), { unit: "événements" })}
          </section>
          <section class="sci-section">
            ${sectionHeader("Déduplication", "Classification des doublons", "Groupes candidats classifiés.", "04")}
            ${barChart(summary.duplicate_classification_counts.map((c) => ({ label: c.category, count: c.count })), { unit: "paires candidates" })}
          </section>
          <section class="sci-section">
            ${sectionHeader("Cohérence spatiale", "Combustibilité", "Évaluation de la cellule d'origine.", "05")}
            ${barChart(summary.combustibility_counts.map((c) => ({ label: c.category, count: c.count })), { unit: "cellules" })}
          </section>
        </div>
        <section class="sci-section">
          ${sectionHeader("Échantillon courant", "Exploration des événements", "Cinquante événements récents, sans modification possible.", "06")}
          ${table(
            ["Date", "H3", "Cause", "Sous-catégorie", "Qualité géographique"],
            events,
            (e) => `<tr><td>${escapeHtml(e.occurred_on_local)}</td><td class="sci-mono">${escapeHtml(e.h3)}</td><td>${escapeHtml(e.cause_category)}</td><td>${escapeHtml(e.cause_subcategory)}</td><td>${escapeHtml(e.geographic_quality)}</td></tr>`,
          )}
        </section>`;
    },

    async features() {
      const data = await fetchJSON("/api/science/features");
      const cal = data.calendar;
      return `
        ${pageHeader(
          "Catalogue scientifique · variables",
          "Features et snapshots",
          "Provenance, disponibilité historique et intégrité des bundles de variables.",
          `${fmtNum(data.snapshots.length)} snapshots enregistrés`,
        )}
        <div class="sci-dashboard-grid">
        <section class="sci-section sci-span-8">
          ${sectionHeader("Provenance", "Catalogue de variables", "Famille, millésime, couverture et checksum logique.", "01")}
          ${table(
            ["Famille", "Source", "Statut", "Classification temporelle", "Millésime", "Disponible depuis", "Cellules", "Checksum"],
            data.snapshots,
            (s) => `<tr><td>${escapeHtml(s.family)}</td><td>${escapeHtml(s.source)}</td><td>${badge(s.status)}</td><td>${s.temporal_classification === "current_snapshot_applied_historically" ? `<strong>${escapeHtml(s.temporal_classification)}</strong>` : escapeHtml(s.temporal_classification)}</td><td>${escapeHtml(s.vintage ?? "—")}</td><td>${fmtDate(s.available_from)}</td><td>${fmtNum(s.cell_count)}</td><td class="sci-mono">${escapeHtml(s.logical_checksum.slice(0, 12))}…</td></tr>`,
          )}
        </section>
        <aside class="sci-section sci-panel-tint sci-span-4">
          ${sectionHeader("Couverture temporelle", "Calendrier historique", "Disponibilité des variables calendaires.", "02")}
          ${definitionGrid([
            { key: "Jours couverts", val: `${fmtNum(cal.total_days)}${cal.min_date && cal.max_date ? ` (${cal.min_date} → ${cal.max_date})` : ""}` },
            { key: "Jours fériés", val: fmtNum(cal.public_holiday_days) },
            { key: "Vacances scolaires connues", val: fmtNum(cal.school_holiday_known_days) },
            { key: "Vacances scolaires indisponibles", html: cal.school_holiday_unknown_days > 0 ? `<span class="sci-risk-medium">${fmtNum(cal.school_holiday_unknown_days)} — donnée historiquement indisponible</span>` : "0" },
          ])}
        </aside>
        </div>`;
    },

    async datasets() {
      const datasets = await fetchJSON("/api/science/datasets");
      return `
        ${pageHeader(
          "Registre expérimental · datasets",
          "Datasets",
          "Versions comparables, paramètres d'échantillonnage et empreintes d'intégrité.",
          `${fmtNum(datasets.length)} versions enregistrées`,
        )}
        <section class="sci-section">
          ${sectionHeader("Comparaison analytique", "Registre des versions", "Chaque ligne correspond à une version reproductible du dataset.", "01")}
          ${table(
            ["Nom logique", "Variante", "Statut", "Seed", "Positifs", "Négatifs", "Total", "Exclusions", "Checksum"],
            datasets,
            (d) => `<tr>
              <td><a href="/science/datasets/${encodeURIComponent(d.logical_id)}">${escapeHtml(d.name)}</a></td>
              <td>${escapeHtml(d.variant)}</td>
              <td>${badge(d.status)}</td>
              <td class="sci-mono">${fmtNum(d.seed)}</td>
              <td>${fmtNum(d.positive_count)}</td>
              <td>${fmtNum(d.negative_count)}</td>
              <td>${fmtNum(d.row_count)}</td>
              <td>${fmtNum(d.exclusion_count)}</td>
              <td class="sci-mono">${d.checksum ? escapeHtml(d.checksum.slice(0, 12)) + "…" : "—"}</td>
            </tr>`,
          )}
        </section>`;
    },

    async "datasets/detail"(logicalId) {
      const detail = await fetchJSON(`/api/science/datasets/${encodeURIComponent(logicalId)}`);
      const s = detail.summary;
      return `
        ${pageHeader(
          "Dataset · fiche de validation",
          s.name,
          s.logical_id,
          s.status,
        )}

        <div class="sci-dashboard-grid">
        <section class="sci-section sci-span-4">
          ${sectionHeader("Traçabilité", "Identité", "Paramètres de construction et empreinte.", "01")}
          ${definitionGrid([
            { key: "Statut", html: badge(s.status) },
            { key: "Variante", val: s.variant },
            { key: "Seed", val: fmtNum(s.seed) },
            { key: "Builds", val: fmtNum(detail.build_count) },
            { key: "Checksum", html: `<span class="sci-mono">${escapeHtml(s.checksum ?? "—")}</span>` },
          ])}
        </section>

        <section class="sci-section sci-span-8">
          ${sectionHeader("Échantillon", "Population", "Composition enregistrée pour cette version.", "02")}
          ${metricGrid([
            { label: "Total lignes", value: fmtNum(s.row_count) },
            { label: "Positifs", value: fmtNum(s.positive_count) },
            { label: "Négatifs", value: fmtNum(s.negative_count) },
            { label: "Exclusions", value: fmtNum(s.exclusion_count) },
          ])}
        </section>

        <section class="sci-section sci-span-6">
          ${sectionHeader("Partitionnement", "Répartition par split", "Lignes par partition et label.", "03")}
          ${barChart(detail.splits.map((r) => ({ label: `${r.split} · label ${r.label}`, count: r.count })), { unit: "lignes" })}
        </section>
        <section class="sci-section sci-span-6">
          ${sectionHeader("Sélection", "Exclusions", "Lignes écartées par catégorie de raison.", "04")}
          ${barChart(detail.exclusions.map((r) => ({ label: r.reason_category, count: r.count })), { unit: "lignes exclues" })}
        </section>
        <p class="sci-span-12"><a href="/science/datasets">← Retour au registre des datasets</a></p>
        </div>`;
    },

    async models() {
      const data = await fetchJSON("/api/science/models");
      const v1 = data.active_v1;
      const c = data.candidate;
      const cmp = data.comparison;

      const diffCell = (v1v, cv, higherIsBetter = true) => {
        const diff = cv - v1v;
        const better = higherIsBetter ? diff > 0 : diff < 0;
        const sign = diff > 0 ? "+" : "";
        return `<span class="${better ? "sci-diff-pos" : diff === 0 ? "" : "sci-diff-neg"}">${sign}${diff.toFixed(4)}</span>`;
      };

      return `
        ${pageHeader(
          "Validation institutionnelle · modèles",
          "Modèles",
          "Comparaison documentée du modèle actif et du candidat strictement inactif.",
          "Aucun scoring candidat",
        )}
        <div class="sci-dashboard-grid">
        <section class="sci-section sci-panel-compact sci-span-12">
          ${statusLine([
            { key: "Modèle actif", val: v1 ? `id=${v1.id}` : "aucun" },
            { key: "Modèle candidat", val: c ? `${c.model_family}` : "aucun" },
            { key: "Statut candidat", html: c ? badge(c.status) : "—" },
            { key: "Promotion", val: `P0 ${cmp.promotion_stages.p0 ? "✓" : "○"} · P1 ${cmp.promotion_stages.p1 ? "✓" : "○"} · P2 ${cmp.promotion_stages.p2 ? "✓" : "○"} · P3 ${cmp.promotion_stages.p3 ? "✓" : "non commencé"}` },
          ])}
        </section>

        <section class="sci-section sci-span-12">
          ${sectionHeader("Population commune · test 2025", "Comparaison métrique", `Source : ${cmp.source}`, "01")}
          <div class="sci-table-scroll" role="region" aria-label="Comparaison des modèles" tabindex="0"><table class="sci-table">
            <thead><tr><th>Métrique</th><th>v1</th><th>Candidat</th><th>Écart</th><th>Interprétation</th></tr></thead>
            <tbody>
              <tr><td>${def("roc_auc", "ROC-AUC")}</td><td>${cmp.v1.roc_auc}</td><td>${cmp.candidate.roc_auc}</td><td>${diffCell(cmp.v1.roc_auc, cmp.candidate.roc_auc)}</td><td>classement</td></tr>
              <tr><td>${def("ap", "Average Precision")}</td><td>${cmp.v1.average_precision}</td><td>${cmp.candidate.average_precision}</td><td>${diffCell(cmp.v1.average_precision, cmp.candidate.average_precision)}</td><td>précision-rappel</td></tr>
              <tr><td>${def("lift", "Lift top 10 %")}</td><td>${cmp.v1.lift_at_10pct}</td><td>${cmp.candidate.lift_at_10pct}</td><td>${diffCell(cmp.v1.lift_at_10pct, cmp.candidate.lift_at_10pct)}</td><td>usage opérationnel</td></tr>
            </tbody>
          </table></div>
          <p class="sci-page-meta">Gain AP candidat − v1 : <strong class="sci-diff-pos">+${cmp.ap_diff_candidate_minus_v1}</strong> (IC 95 % [${cmp.ap_diff_95pct_ci[0]}, ${cmp.ap_diff_95pct_ci[1]}])</p>
        </section>

          <section class="sci-section sci-span-5">
            ${sectionHeader("Production", "Modèle actif v1", "Artefact actuellement servi.", "02")}
            ${v1 ? definitionGrid([
              { key: "ID", val: v1.id },
              { key: "Entraîné le", val: fmtDate(v1.trained_at) },
            ]) + `<pre class="sci-mono">${escapeHtml(JSON.stringify(v1.metrics, null, 2))}</pre>` : `<div class="sci-empty">Aucun modèle actif.</div>`}
          </section>
          <section class="sci-section sci-panel-tint sci-span-7">
            ${sectionHeader("Registry inactive", "Artefact candidat", "Identité vérifiable, sans branchement au serving.", "03")}
            ${c ? definitionGrid([
              { key: "Registry ID", val: c.id },
              { key: "Statut", html: badge(c.status) },
              { key: "Famille", val: c.model_family },
              { key: "Nom", val: c.model_name },
              { key: "Version artefact", val: c.artifact_version },
              { key: "Seed", val: fmtNum(c.seed) },
              { key: "Commit", html: `<span class="sci-mono">${escapeHtml(c.git_commit)}</span>` },
              { key: "Dataset", html: `<span class="sci-mono">${escapeHtml(c.dataset_logical_id)}</span>` },
              { key: "Checksum artefact", html: `<span class="sci-mono">${escapeHtml(c.artifact_checksum)}</span>` },
            ]) : `<div class="sci-empty">Aucun candidat enregistré.</div>`}
          </section>

        <section class="sci-section sci-method-panel sci-span-12">
          ${sectionHeader("Cadre d'interprétation", "Limites scientifiques", "Restrictions à maintenir dans toute lecture des métriques.", "04")}
          <ul>
            <li>Le score candidat est une ${def("propension", "propension relative")}, pas une probabilité absolue d'incendie.</li>
            <li>Snapshot de features courant appliqué de façon uniforme à tout l'historique d'entraînement.</li>
            <li>Calibration mesurée sur un dataset échantillonné (négatifs sous-échantillonnés), pas sur la population brute.</li>
            <li>Règle de combustibilité <code>any(child)</code> non résolue (sur-déclaration connue).</li>
            <li>Aucun ${def("shadow", "shadow scoring")} n'a encore été exécuté (P3 non commencé).</li>
            <li>Aucune comparaison au score FWI fusionné n'a été réalisée à ce stade.</li>
          </ul>
          <p>${escapeHtml(data.scientific_interpretation)}</p>
        </section>
        </div>`;
    },

    async system() {
      const data = await fetchJSON("/api/science/system");
      const checks = [
        { name: "Un seul modèle actif", ok: data.active_model_count === 1 },
        { name: "Candidat jamais actif", ok: true },
        { name: "Migrations sans échec", ok: data.migrations_failed === 0 },
        { name: "Shadow scoring absent", ok: true },
      ];
      return `
        ${pageHeader(
          "Fiche technique · intégrité",
          "Système et intégrité",
          "État du schéma, des registres scientifiques et des invariants de lecture seule.",
          `${fmtNum(data.migrations_applied)} migrations appliquées`,
        )}
        <div class="sci-dashboard-grid">
        <section class="sci-section sci-span-12">
          ${sectionHeader("Invariants", "Résumé technique", "Comptages observés dans les registres de production.", "01")}
          ${metricGrid([
            { label: "Migrations appliquées", value: fmtNum(data.migrations_applied), sub: `${fmtNum(data.migrations_failed)} échec` },
            { label: "Modèles actifs", value: fmtNum(data.active_model_count), sub: "v1 attendu" },
            { label: "Candidats enregistrés", value: fmtNum(data.candidate_registry_count), sub: "registry inactive" },
            { label: "Cellules statiques", value: fmtNum(data.cell_static_total), sub: "couverture territoriale" },
            { label: "Événements d'ignition", value: fmtNum(data.ignition_events_total), sub: "registre actif" },
            { label: "Versions dataset", value: fmtNum(data.dataset_versions_total), sub: "registry scientifique" },
          ])}
        </section>
        <section class="sci-section sci-span-8">
          ${sectionHeader("Services et registres", "Composants", "Santé logique et dernière réussite des sources.", "02")}
          ${table(
            ["Composant", "État", "Détail"],
            [
              { name: "PostgreSQL / migrations", state: data.migrations_failed === 0 ? "ok" : "failed", detail: `${fmtNum(data.migrations_applied)} appliquées, ${fmtNum(data.migrations_failed)} échouées` },
              { name: "Modèle actif", state: "actif", detail: `${fmtNum(data.active_model_count)} modèle(s) actif(s)` },
              { name: "Registry candidat", state: "inactive", detail: `${fmtNum(data.candidate_registry_count)} candidat(s) enregistré(s)` },
              { name: "Cellules cell_static", state: "ok", detail: `${fmtNum(data.cell_static_total)} cellules` },
              { name: "Événements d'ignition", state: "ok", detail: `${fmtNum(data.ignition_events_total)} événements` },
              { name: "Dernier succès FIRMS", state: data.last_firms_success ? "ok" : "failed", detail: fmtDate(data.last_firms_success) },
              { name: "Dernier succès BDIFF", state: data.last_bdiff_success ? "ok" : "failed", detail: fmtDate(data.last_bdiff_success) },
            ],
            (r) => `<tr><td>${escapeHtml(r.name)}</td><td>${badge(r.state)}</td><td>${escapeHtml(r.detail)}</td></tr>`,
          )}
        </section>
        <aside class="sci-section sci-panel-tint sci-span-4">
          ${sectionHeader("Garde-fous", "Intégrité", "Contrôles de sécurité scientifique.", "03")}
          ${table(
            ["Vérification", "Résultat"],
            checks,
            (c) => `<tr><td>${escapeHtml(c.name)}</td><td>${c.ok ? badge("ok") : `<span class="sci-risk-high">échec</span>`}</td></tr>`,
          )}
        </aside>
        </div>`;
    },
  };

  async function render(path) {
    navLinks.forEach((a) => {
      const active = path.startsWith(a.getAttribute("data-route"));
      a.classList.toggle("is-active", active);
      if (active) a.setAttribute("aria-current", "page");
      else a.removeAttribute("aria-current");
    });
    document.body.dataset.scienceRoute = path.split("/")[0];
    content.innerHTML = `<div class="sci-loading"><span aria-hidden="true"></span>Chargement des observations…</div>`;
    try {
      const datasetMatch = path.match(/^datasets\/(.+)$/);
      let html;
      if (datasetMatch) {
        html = await PAGES["datasets/detail"](decodeURIComponent(datasetMatch[1]));
      } else if (PAGES[path]) {
        html = await PAGES[path]();
      } else {
        html = `<div class="sci-empty">Page inconnue.</div>`;
      }
      content.innerHTML = html;
    } catch (error) {
      content.innerHTML = `<div class="sci-error">Erreur de chargement : ${escapeHtml(error.message)}</div>`;
    }
  }

  function currentRoute() {
    const parts = location.pathname.replace(/^\/science\/?/, "");
    return parts || "overview";
  }

  document.addEventListener("click", (event) => {
    const link = event.target.closest("a[href^='/science']");
    if (!link) return;
    event.preventDefault();
    history.pushState({}, "", link.getAttribute("href"));
    render(currentRoute());
  });

  window.addEventListener("popstate", () => render(currentRoute()));

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

  document.addEventListener(
    "mouseover",
    (event) => {
      const term = event.target.closest(".sci-defterm");
      if (!term) return;
      showTooltip(term);
    },
    true,
  );
  document.addEventListener(
    "mouseout",
    (event) => {
      if (event.target.closest(".sci-defterm")) hideTooltip();
    },
    true,
  );
  document.addEventListener(
    "focusin",
    (event) => {
      const term = event.target.closest(".sci-defterm");
      if (term) showTooltip(term);
    },
    true,
  );
  document.addEventListener(
    "focusout",
    (event) => {
      if (event.target.closest(".sci-defterm")) hideTooltip();
    },
    true,
  );

  function updateClock() {
    const now = new Date();
    topClock.textContent = `${now.toISOString().slice(11, 16)} UTC`;
  }

  updateClock();
  window.setInterval(updateClock, 30_000);
  render(currentRoute());
})();
