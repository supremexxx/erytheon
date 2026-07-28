(() => {
  "use strict";

  const content = document.querySelector("#sci-content");
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
    return `<span class="sci-defterm" data-def="${escapeHtml(text)}" tabindex="0">${escapeHtml(label)}</span>`;
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

  function card(label, value, sub) {
    return `<div class="sci-card"><span class="sci-card-label">${label}</span><span class="sci-card-value">${escapeHtml(value)}</span>${sub ? `<span class="sci-card-sub">${escapeHtml(sub)}</span>` : ""}</div>`;
  }

  function barChart(rows, maxHint) {
    const max = maxHint || Math.max(1, ...rows.map((r) => r.count));
    return rows
      .map(
        (r) => `<div class="sci-bar-row">
          <span class="sci-bar-label">${escapeHtml(r.label)}</span>
          <div class="sci-bar-track"><div class="sci-bar-fill" style="width:${Math.max(2, (r.count / max) * 100)}%"></div></div>
          <span class="sci-bar-value">${fmtNum(r.count)}</span>
        </div>`,
      )
      .join("");
  }

  function table(columns, rows, renderRow) {
    if (rows.length === 0) {
      return `<div class="sci-empty">Aucune donnée pour ce filtre.</div>`;
    }
    return `<table class="sci-table"><thead><tr>${columns.map((c) => `<th>${escapeHtml(c)}</th>`).join("")}</tr></thead><tbody>${rows.map(renderRow).join("")}</tbody></table>`;
  }

  const PAGES = {
    async overview() {
      const data = await fetchJSON("/api/science/overview");
      return `
        <h1>Vue d'ensemble</h1>
        <p class="sci-page-meta">Actualisé ${fmtDate(new Date().toISOString())} · toutes les valeurs sont lues en direct depuis PostgreSQL.</p>

        <section class="sci-section">
          <h2>État système</h2>
          <div class="sci-card-grid">
            ${card("Application", data.app_status)}
            ${card("PostgreSQL", data.db_status)}
            ${card("Migrations appliquées", fmtNum(data.migrations_applied))}
            ${card(def("modele_actif", "Modèle actif"), data.active_model_id != null ? `v1 (id=${data.active_model_id})` : "aucun")}
            ${card(def("modele_candidat", "Candidat"), data.candidate_status ?? "aucun", data.candidate_model_family ?? "")}
            ${card(def("shadow", "Shadow scoring"), "non déployé")}
          </div>
        </section>

        <section class="sci-section">
          <h2>Données</h2>
          <div class="sci-card-grid">
            ${card("Événements BDIFF", fmtNum(data.bdiff_events_total))}
            ${card("Causes humaines", fmtNum(data.bdiff_human_known))}
            ${card("Causes naturelles", fmtNum(data.bdiff_natural_known))}
            ${card("Causes inconnues", fmtNum(data.bdiff_unknown))}
            ${card("Observations FIRMS", fmtNum(data.firms_observations_total))}
            ${card("Cellules H3 (res. 9)", fmtNum(data.cell_static_total))}
            ${card("Snapshots de features", fmtNum(data.feature_snapshots_total))}
            ${card("Versions de dataset", fmtNum(data.dataset_versions_total))}
            ${card("Builds de dataset", fmtNum(data.dataset_builds_total))}
            ${card("Modèles entraînés (v1)", fmtNum(data.human_model_versions_total))}
          </div>
        </section>

        <section class="sci-section sci-warning-box">
          <h2>⚠ Avertissements scientifiques ouverts</h2>
          <ul>
            <li>Le snapshot <code>cell_static</code> courant est appliqué de façon uniforme à tout l'historique d'entraînement.</li>
            <li>Les vacances scolaires sont ${def("checksum", "historiquement indisponibles")} pour la période étudiée — jamais fixées à zéro par convention.</li>
            <li>La règle de combustibilité <code>any(child)</code> sur-déclare des cellules ; sensibilité mesurée, non résolue.</li>
            <li>Le candidat n'a pas encore été validé en shadow scoring (P3 non commencé).</li>
            <li>Le score candidat est une ${def("propension", "propension relative")}, pas une probabilité absolue d'incendie.</li>
            <li>Aucun positif BDIFF confirmé sur 2026 à ce stade.</li>
          </ul>
        </section>`;
    },

    async progress() {
      const phases = await fetchJSON("/api/science/progress");
      const statusClass = (status) => (status === "terminé" || status === "validé" ? "is-done" : status === "en cours" || status === "en production" ? "is-active" : "");
      return `
        <h1>Progression du projet</h1>
        <p class="sci-page-meta">Historique versionné dans le dépôt (aucune table dédiée n'existe encore).</p>
        <ul class="sci-timeline">
          ${phases
            .map(
              (p) => `<li class="${statusClass(p.status)}">
                <div class="sci-timeline-header"><span class="sci-timeline-title">${escapeHtml(p.label)} — ${escapeHtml(p.title)}</span>${badge(p.status)}</div>
                <p class="sci-timeline-summary">${escapeHtml(p.summary)}</p>
                <p class="sci-timeline-meta">Environnement : ${escapeHtml(p.environment)} · Production affectée : ${p.production_affected ? "oui" : "non"}${p.commits && p.commits.length ? " · commits : " + p.commits.map(escapeHtml).join(", ") : ""}</p>
                ${p.risks && p.risks.length ? `<p class="sci-timeline-meta">Risques ouverts : ${p.risks.map(escapeHtml).join("; ")}</p>` : ""}
              </li>`,
            )
            .join("")}
        </ul>`;
    },

    async sources() {
      const [sources, imports, pipelines] = await Promise.all([
        fetchJSON("/api/science/sources"),
        fetchJSON("/api/science/imports?limit=50"),
        fetchJSON("/api/science/pipelines?limit=50"),
      ]);
      return `
        <h1>Sources et pipelines</h1>
        <section class="sci-section">
          <h2>Sources</h2>
          ${table(
            ["Source", "Catégorie", "Dernière réussite", "Observations", "Erreur récente"],
            sources,
            (s) => `<tr><td>${escapeHtml(s.id)}</td><td>${escapeHtml(s.category ?? "—")}</td><td>${fmtDate(s.last_success)}</td><td>${fmtNum(s.observation_count)}</td><td>${escapeHtml(s.recent_error ?? "—")}</td></tr>`,
          )}
        </section>
        <section class="sci-section">
          <h2>Imports récents</h2>
          ${table(
            ["Batch", "Source", "Statut", "Début", "Reçues", "Insérées", "Rejetées"],
            imports,
            (b) => `<tr><td class="sci-checksum">${escapeHtml(b.id.slice(0, 8))}</td><td>${escapeHtml(b.source_code ?? "—")}</td><td>${badge(b.status)}</td><td>${fmtDate(b.started_at)}</td><td>${fmtNum(b.records_received)}</td><td>${fmtNum(b.records_inserted)}</td><td>${fmtNum(b.records_rejected)}</td></tr>`,
          )}
        </section>
        <section class="sci-section">
          <h2>Pipelines récents</h2>
          ${table(
            ["Run", "Pipeline", "Statut", "Début", "Erreur"],
            pipelines,
            (r) => `<tr><td class="sci-checksum">${escapeHtml(r.id.slice(0, 8))}</td><td>${escapeHtml(r.pipeline_name)}</td><td>${badge(r.status)}</td><td>${fmtDate(r.started_at)}</td><td>${escapeHtml(r.error_message ?? "—")}</td></tr>`,
          )}
        </section>`;
    },

    async "data-quality"() {
      const [summary, events] = await Promise.all([
        fetchJSON("/api/science/data-quality"),
        fetchJSON("/api/science/data-quality/events?limit=50"),
      ]);
      return `
        <h1>Qualité des données</h1>
        <div class="sci-card-grid">
          ${card("Événements BDIFF", fmtNum(summary.bdiff_events_total))}
          ${card("Groupes de coordonnées", fmtNum(summary.coordinate_groups_total))}
          ${card("Paires candidates doublons", fmtNum(summary.duplicate_candidate_pairs_total))}
        </div>
        <div class="sci-two-col">
          <section class="sci-section">
            <h2>Causes</h2>
            ${barChart(summary.cause_counts.map((c) => ({ label: c.category, count: c.count })))}
          </section>
          <section class="sci-section">
            <h2>Classification des doublons</h2>
            ${barChart(summary.duplicate_classification_counts.map((c) => ({ label: c.category, count: c.count })))}
          </section>
          <section class="sci-section">
            <h2>Qualité géographique</h2>
            ${barChart(summary.geographic_quality_counts.map((c) => ({ label: c.category, count: c.count })))}
          </section>
          <section class="sci-section">
            <h2>Combustibilité</h2>
            ${barChart(summary.combustibility_counts.map((c) => ({ label: c.category, count: c.count })))}
          </section>
        </div>
        <section class="sci-section">
          <h2>Exploration des événements</h2>
          ${table(
            ["Date", "H3", "Cause", "Sous-catégorie", "Qualité géographique"],
            events,
            (e) => `<tr><td>${escapeHtml(e.occurred_on_local)}</td><td class="sci-checksum">${escapeHtml(e.h3)}</td><td>${escapeHtml(e.cause_category)}</td><td>${escapeHtml(e.cause_subcategory)}</td><td>${escapeHtml(e.geographic_quality)}</td></tr>`,
          )}
        </section>`;
    },

    async features() {
      const data = await fetchJSON("/api/science/features");
      const cal = data.calendar;
      return `
        <h1>Features et snapshots</h1>
        <section class="sci-section">
          <h2>Snapshots de features</h2>
          ${table(
            ["Famille", "Source", "Statut", "Classification temporelle", "Millésime", "Disponible depuis", "Cellules", "Checksum"],
            data.snapshots,
            (s) => `<tr><td>${escapeHtml(s.family)}</td><td>${escapeHtml(s.source)}</td><td>${badge(s.status)}</td><td>${s.temporal_classification === "current_snapshot_applied_historically" ? `<strong>${escapeHtml(s.temporal_classification)}</strong>` : escapeHtml(s.temporal_classification)}</td><td>${escapeHtml(s.vintage ?? "—")}</td><td>${fmtDate(s.available_from)}</td><td>${fmtNum(s.cell_count)}</td><td class="sci-checksum">${escapeHtml(s.logical_checksum.slice(0, 12))}…</td></tr>`,
          )}
        </section>
        <section class="sci-section">
          <h2>Calendrier historique</h2>
          <div class="sci-card-grid">
            ${card("Jours couverts", fmtNum(cal.total_days), cal.min_date && cal.max_date ? `${cal.min_date} → ${cal.max_date}` : "")}
            ${card("Jours fériés", fmtNum(cal.public_holiday_days))}
            ${card("Vacances scolaires connues", fmtNum(cal.school_holiday_known_days))}
            ${card("Vacances scolaires indisponibles", cal.school_holiday_unknown_days > 0 ? `${fmtNum(cal.school_holiday_unknown_days)} — donnée historiquement indisponible` : "0")}
          </div>
        </section>`;
    },

    async datasets() {
      const datasets = await fetchJSON("/api/science/datasets");
      return `
        <h1>Datasets candidats</h1>
        ${table(
          ["Variante", "Logical ID", "Statut", "Positifs", "Négatifs", "Total", "Exclusions", "Checksum"],
          datasets,
          (d) => `<tr><td><a href="/science/datasets/${encodeURIComponent(d.logical_id)}">${escapeHtml(d.variant)}</a></td><td class="sci-checksum">${escapeHtml(d.logical_id)}</td><td>${badge(d.status)}</td><td>${fmtNum(d.positive_count)}</td><td>${fmtNum(d.negative_count)}</td><td>${fmtNum(d.row_count)}</td><td>${fmtNum(d.exclusion_count)}</td><td class="sci-checksum">${d.checksum ? escapeHtml(d.checksum.slice(0, 12)) + "…" : "—"}</td></tr>`,
        )}`;
    },

    async "datasets/detail"(logicalId) {
      const detail = await fetchJSON(`/api/science/datasets/${encodeURIComponent(logicalId)}`);
      const s = detail.summary;
      return `
        <h1>${escapeHtml(s.name)}</h1>
        <p class="sci-page-meta">${escapeHtml(s.logical_id)}</p>
        <div class="sci-card-grid">
          ${card("Statut", s.status)}
          ${card("Variante", s.variant)}
          ${card("Seed", fmtNum(s.seed))}
          ${card("Total lignes", fmtNum(s.row_count))}
          ${card("Positifs", fmtNum(s.positive_count))}
          ${card("Négatifs", fmtNum(s.negative_count))}
          ${card("Exclusions", fmtNum(s.exclusion_count))}
          ${card("Builds", fmtNum(detail.build_count))}
        </div>
        <p class="sci-checksum">Checksum : ${escapeHtml(s.checksum ?? "—")}</p>
        <section class="sci-section">
          <h2>Répartition par split</h2>
          ${barChart(detail.splits.map((r) => ({ label: `${r.split} · label ${r.label}`, count: r.count })))}
        </section>
        <section class="sci-section">
          <h2>Exclusions</h2>
          ${barChart(detail.exclusions.map((r) => ({ label: r.reason_category, count: r.count })))}
        </section>
        <p><a href="/science/datasets">← Retour à la liste des datasets</a></p>`;
    },

    async models() {
      const data = await fetchJSON("/api/science/models");
      const v1 = data.active_v1;
      const c = data.candidate;
      const cmp = data.comparison;
      return `
        <h1>Modèles</h1>
        <div class="sci-two-col">
          <section class="sci-section">
            <h2>Modèle actif v1</h2>
            ${v1 ? `<div class="sci-card-grid">
              ${card("ID", v1.id)}
              ${card("Statut", "actif")}
              ${card("Entraîné le", fmtDate(v1.trained_at))}
            </div><pre class="sci-checksum">${escapeHtml(JSON.stringify(v1.metrics, null, 2))}</pre>` : `<div class="sci-empty">Aucun modèle actif.</div>`}
          </section>
          <section class="sci-section">
            <h2>Modèle candidat v2</h2>
            ${c ? `<div class="sci-card-grid">
              ${card("Registry ID", c.id)}
              ${card("Statut", c.status)}
              ${card("Famille", c.model_family)}
              ${card("Nom", c.model_name)}
              ${card("Version artefact", c.artifact_version)}
              ${card("Seed", fmtNum(c.seed))}
              ${card("Commit", c.git_commit)}
              ${card("Dataset", c.dataset_logical_id)}
            </div>
            <p class="sci-checksum">Checksum artefact : ${escapeHtml(c.artifact_checksum)}</p>` : `<div class="sci-empty">Aucun candidat enregistré.</div>`}
          </section>
        </div>
        <section class="sci-section">
          <h2>Comparaison v1 / candidat (test 2025, population commune)</h2>
          <p class="sci-page-meta">Source : ${escapeHtml(cmp.source)}</p>
          <table class="sci-table">
            <thead><tr><th>Métrique</th><th>v1</th><th>Candidat</th></tr></thead>
            <tbody>
              <tr><td>${def("roc_auc", "ROC-AUC")}</td><td>${cmp.v1.roc_auc}</td><td>${cmp.candidate.roc_auc}</td></tr>
              <tr><td>${def("ap", "Average Precision")}</td><td>${cmp.v1.average_precision}</td><td>${cmp.candidate.average_precision}</td></tr>
              <tr><td>${def("lift", "Lift top 10%")}</td><td>${cmp.v1.lift_at_10pct}</td><td>${cmp.candidate.lift_at_10pct}</td></tr>
            </tbody>
          </table>
          <p>Gain AP candidat − v1 : <strong>+${cmp.ap_diff_candidate_minus_v1}</strong> (IC 95% [${cmp.ap_diff_95pct_ci[0]}, ${cmp.ap_diff_95pct_ci[1]}])</p>
          <p>Promotion : P0 ${cmp.promotion_stages.p0 ? "✓" : "○"} · P1 ${cmp.promotion_stages.p1 ? "✓" : "○"} · P2 ${cmp.promotion_stages.p2 ? "✓" : "○"} · P3 ${cmp.promotion_stages.p3 ? "✓" : "○ non commencé"}</p>
        </section>
        <section class="sci-section sci-warning-box">
          <h2>Sémantique du score</h2>
          <p>${escapeHtml(data.scientific_interpretation)}</p>
        </section>`;
    },

    async system() {
      const data = await fetchJSON("/api/science/system");
      return `
        <h1>Système et intégrité</h1>
        <div class="sci-card-grid">
          ${card("Migrations réussies", fmtNum(data.migrations_applied))}
          ${card("Migrations échouées", fmtNum(data.migrations_failed))}
          ${card("Modèles actifs", fmtNum(data.active_model_count), data.active_model_count === 1 ? "unique, comme attendu" : "⚠ anomalie")}
          ${card("Candidats enregistrés", fmtNum(data.candidate_registry_count))}
          ${card("Cellules cell_static", fmtNum(data.cell_static_total))}
          ${card("Événements d'ignition", fmtNum(data.ignition_events_total))}
          ${card("Versions de dataset", fmtNum(data.dataset_versions_total))}
          ${card("Dernier succès FIRMS", fmtDate(data.last_firms_success))}
          ${card("Dernier succès BDIFF", fmtDate(data.last_bdiff_success))}
        </div>
        <section class="sci-section sci-warning-box">
          <h2>Intégrité</h2>
          <ul>
            <li>Un seul modèle actif : ${data.active_model_count === 1 ? "confirmé" : "⚠ à vérifier manuellement"}.</li>
            <li>Shadow scoring : non déployé (aucune table shadow n'existe).</li>
          </ul>
        </section>`;
    },
  };

  async function render(path) {
    navLinks.forEach((a) => a.classList.toggle("is-active", path.startsWith(a.getAttribute("data-route"))));
    content.innerHTML = `<div class="sci-loading">Chargement…</div>`;
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
  document.addEventListener(
    "mouseover",
    (event) => {
      const term = event.target.closest(".sci-defterm");
      if (!term) return;
      tooltip.textContent = term.dataset.def;
      tooltip.hidden = false;
      const rect = term.getBoundingClientRect();
      tooltip.style.left = `${rect.left}px`;
      tooltip.style.top = `${rect.bottom + 6}px`;
    },
    true,
  );
  document.addEventListener(
    "mouseout",
    (event) => {
      if (event.target.closest(".sci-defterm")) tooltip.hidden = true;
    },
    true,
  );

  render(currentRoute());
})();
