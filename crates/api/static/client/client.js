(() => {
  "use strict";

  const RISK_STOPS = [
    [0.0, [47, 122, 79]],
    [0.35, [217, 178, 60]],
    [0.65, [217, 98, 47]],
    [1.0, [179, 38, 30]],
  ];

  function colorForScore(score) {
    const value = Math.min(1, Math.max(0, Number(score) || 0));
    let lower = RISK_STOPS[0];
    let upper = RISK_STOPS[RISK_STOPS.length - 1];
    for (let index = 0; index < RISK_STOPS.length - 1; index += 1) {
      if (value >= RISK_STOPS[index][0] && value <= RISK_STOPS[index + 1][0]) {
        lower = RISK_STOPS[index];
        upper = RISK_STOPS[index + 1];
        break;
      }
    }
    const span = upper[0] - lower[0] || 1;
    const ratio = (value - lower[0]) / span;
    const rgb = lower[1].map((channel, index) =>
      Math.round(channel + (upper[1][index] - channel) * ratio),
    );
    return `rgb(${rgb.join(",")})`;
  }

  function inseeCodeFromPath() {
    const match = window.location.pathname.match(/\/client\/([^/]+)/);
    return match ? decodeURIComponent(match[1]) : null;
  }

  async function fetchJson(url) {
    const response = await fetch(url);
    if (!response.ok) {
      const body = await response.json().catch(() => null);
      const message = body && body.error ? body.error.message : response.statusText;
      throw new Error(message);
    }
    return response.json();
  }

  const HORIZON_LABELS = Object.freeze({
    nowcast: "Maintenant",
    hours_6: "+6 h",
    hours_24: "+24 h",
    hours_48: "+48 h",
  });

  function main() {
    const elements = {
      name: document.getElementById("cli-commune-name"),
      meta: document.getElementById("cli-commune-meta"),
      loading: document.getElementById("cli-loading"),
      error: document.getElementById("cli-error"),
      horizonButtons: [...document.querySelectorAll("[data-horizon]")],
      detail: document.getElementById("cli-detail"),
      detailClose: document.getElementById("cli-detail-close"),
      detailScore: document.getElementById("cli-detail-score"),
      detailTime: document.getElementById("cli-detail-time"),
    };

    function setLoading(visible) {
      elements.loading.hidden = !visible;
    }

    function showError(message) {
      elements.error.textContent = message;
      elements.error.hidden = false;
    }

    const inseeCode = inseeCodeFromPath();
    if (!inseeCode) {
      showError("Aucun code INSEE dans l'URL. Utilisez /client/{code_insee}.");
      setLoading(false);
      return;
    }

    const map = L.map("cli-map", { zoomControl: true, attributionControl: true });
    L.tileLayer("https://basemaps.cartocdn.com/light_all/{z}/{x}/{y}{r}.png", {
      attribution: "&copy; OpenStreetMap contributors &copy; CARTO",
      maxZoom: 19,
    }).addTo(map);
    map.setView([46.6, 2.2], 5);

    let riskLayer = null;
    let currentHorizon = "nowcast";

    function showDetail(properties) {
      elements.detailScore.textContent = `Score ${(Number(properties.score) * 100).toFixed(0)} %`;
      const horizonLabel = HORIZON_LABELS[properties.horizon] || properties.horizon;
      elements.detailTime.textContent = `${horizonLabel} · ${new Date(properties.valid_at).toLocaleString("fr-FR")}`;
      elements.detail.hidden = false;
    }

    async function loadRisk(horizon) {
      setLoading(true);
      try {
        const featureCollection = await fetchJson(
          `/api/client/communes/${encodeURIComponent(inseeCode)}/risk?horizon=${encodeURIComponent(horizon)}`,
        );
        if (riskLayer) {
          map.removeLayer(riskLayer);
        }
        riskLayer = L.geoJSON(featureCollection, {
          style: (feature) => {
            const color = colorForScore(feature.properties.score);
            return { color, weight: 1, fillColor: color, fillOpacity: 0.55 };
          },
          onEachFeature: (feature, layer) => {
            layer.on("click", () => showDetail(feature.properties));
          },
        }).addTo(map);
      } catch (error) {
        showError(`Impossible de charger le risque : ${error.message}`);
      } finally {
        setLoading(false);
      }
    }

    elements.horizonButtons.forEach((button) => {
      button.addEventListener("click", () => {
        elements.horizonButtons.forEach((other) => {
          other.setAttribute("aria-pressed", String(other === button));
        });
        currentHorizon = button.dataset.horizon;
        loadRisk(currentHorizon);
      });
    });

    elements.detailClose.addEventListener("click", () => {
      elements.detail.hidden = true;
    });

    (async () => {
      let commune;
      try {
        commune = await fetchJson(`/api/client/communes/${encodeURIComponent(inseeCode)}`);
      } catch (error) {
        showError(`Commune introuvable : ${error.message}`);
        setLoading(false);
        return;
      }

      elements.name.textContent = commune.name;
      elements.meta.textContent = commune.postal_codes.length
        ? `INSEE ${commune.insee_code} · ${commune.postal_codes.join(", ")}`
        : `INSEE ${commune.insee_code}`;
      document.title = `FireSift — ${commune.name}`;

      L.geoJSON(commune.boundary, {
        style: { color: "#315342", weight: 2, fill: false },
      }).addTo(map);

      const [west, south, east, north] = commune.bbox;
      const bounds = L.latLngBounds([south, west], [north, east]);
      map.fitBounds(bounds, { padding: [24, 24] });
      map.setMaxBounds(bounds.pad(0.15));
      map.setMinZoom(map.getBoundsZoom(bounds));

      await loadRisk(currentHorizon);
      setLoading(false);
    })();
  }

  document.addEventListener("DOMContentLoaded", main);
})();
