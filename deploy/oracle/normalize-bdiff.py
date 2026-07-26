#!/usr/bin/env python3
import argparse
import csv
import hashlib
import io
import json
import re
import sys
import unicodedata
from datetime import datetime, timezone
from pathlib import Path
from zoneinfo import ZoneInfo


ALIASES = {
    "external_id": ("external_id", "identifiant", "identifiant_incendie", "id_incendie", "id_feu", "numero_incendie", "numero_feu", "numero"),
    "year": ("annee", "year"),
    "occurred_at": ("occurred_at", "alerte", "date_alerte", "date_et_heure_alerte", "date_de_premiere_alerte", "date_incendie", "date"),
    "alert_time": ("heure_alerte", "heure"),
    "municipality": ("municipality", "commune", "nom_commune", "nom_de_la_commune"),
    "municipality_code": ("code_insee", "code_commune", "code_commune_insee", "insee", "codcommune"),
    "department": ("departement", "code_departement", "dept", "dep"),
    "latitude": ("latitude", "lat", "y"),
    "longitude": ("longitude", "lon", "lng", "x"),
    "surface_ha": ("surface_ha", "surface_totale", "surface_parcourue", "surface", "surface_total_ha", "surface_parcourue_m2"),
    "cause": ("cause", "nature", "origine", "nature_cause", "cause_presumee"),
}


def normalized(value):
    value = unicodedata.normalize("NFKD", str(value)).encode("ascii", "ignore").decode("ascii")
    return re.sub(r"[^a-z0-9]+", "_", value.strip().lower()).strip("_")


def decode_csv(path):
    payload = Path(path).read_bytes()
    for encoding in ("utf-8-sig", "cp1252"):
        try:
            return payload.decode(encoding)
        except UnicodeDecodeError:
            continue
    raise ValueError("BDIFF export is neither UTF-8 nor Windows-1252")


def csv_rows(path):
    document = decode_csv(path)
    lines = document.splitlines()
    header_index = next(
        (
            index
            for index, line in enumerate(lines)
            if "code_insee" in normalized(line)
            and (
                "date_de_premiere_alerte" in normalized(line)
                or "occurred_at" in normalized(line)
            )
        ),
        None,
    )
    if header_index is None:
        raise ValueError("BDIFF export CSV header was not found")
    document = "\n".join(lines[header_index:])
    sample = document[:8192]
    try:
        dialect = csv.Sniffer().sniff(sample, delimiters=",;\t|")
    except csv.Error:
        dialect = csv.excel
    reader = csv.DictReader(io.StringIO(document), dialect=dialect)
    if not reader.fieldnames:
        raise ValueError("BDIFF export has no CSV header")
    for raw_row in reader:
        yield {normalized(key): value.strip() for key, value in raw_row.items() if key}


def first(row, key):
    for alias in ALIASES[key]:
        value = row.get(alias, "").strip()
        if value:
            return value
    return ""


def parse_number(value, default=None):
    match = re.search(r"-?\d+(?:[.,]\d+)?", value.replace("\u202f", "").replace(" ", ""))
    if not match:
        if default is not None:
            return default
        raise ValueError(f"invalid number: {value!r}")
    return float(match.group(0).replace(",", "."))


def parse_datetime(row):
    value = first(row, "occurred_at")
    alert_time = first(row, "alert_time")
    if alert_time and not re.search(r"\d{1,2}[:h]\d{2}", value):
        value = f"{value} {alert_time}"
    iso_value = value.replace("Z", "+00:00")
    try:
        parsed = datetime.fromisoformat(iso_value)
    except ValueError:
        parsed = None
    if parsed is None:
        for pattern in (
            "%d/%m/%Y %H:%M",
            "%d/%m/%Y %Hh%M",
            "%d/%m/%Y",
            "%Y-%m-%d %H:%M",
            "%Y-%m-%d",
        ):
            try:
                parsed = datetime.strptime(value, pattern)
                break
            except ValueError:
                continue
    if parsed is None:
        raise ValueError(f"invalid alert timestamp: {value!r}")
    if parsed.tzinfo is None:
        parsed = parsed.replace(tzinfo=ZoneInfo("Europe/Paris"))
    return parsed.astimezone(timezone.utc).isoformat(timespec="seconds").replace("+00:00", "Z")


def department_code(value):
    value = value.strip().upper()
    if value in {"2A", "2B"}:
        return value
    digits = re.sub(r"\D", "", value)
    return digits.zfill(2) if digits else ""


def load_communes(path):
    records = json.loads(Path(path).read_text(encoding="utf-8"))
    by_code = {}
    by_name_department = {}
    by_name = {}
    for record in records:
        geometry = record.get("centre") or record.get("center")
        coordinates = geometry.get("coordinates", []) if isinstance(geometry, dict) else []
        if len(coordinates) != 2:
            continue
        point = (float(coordinates[1]), float(coordinates[0]))
        code = str(record.get("code", "")).strip().upper()
        name = normalized(record.get("nom", ""))
        department = department_code(record.get("codeDepartement", "") or code[:2])
        if code:
            by_code[code] = point
        if name and department:
            by_name_department[(name, department)] = point
        if name:
            by_name.setdefault(name, []).append(point)
    return by_code, by_name_department, by_name


def coordinate(row, municipality, commune_indexes):
    latitude = first(row, "latitude")
    longitude = first(row, "longitude")
    if latitude and longitude:
        return parse_number(latitude), parse_number(longitude)
    by_code, by_name_department, by_name = commune_indexes
    code = first(row, "municipality_code").replace(" ", "").upper()
    if code in by_code:
        return by_code[code]
    key = (normalized(municipality), department_code(first(row, "department")))
    if key in by_name_department:
        return by_name_department[key]
    candidates = by_name.get(key[0], [])
    if len(candidates) == 1:
        return candidates[0]
    raise ValueError(f"no unambiguous municipality centre for {municipality!r} ({code or key[1]})")


def normalize_export(source, communes, destination, max_reject_ratio):
    commune_indexes = load_communes(communes)
    normalized_rows = []
    rejected = []
    for line_number, row in enumerate(csv_rows(source), start=2):
        try:
            municipality = first(row, "municipality")
            if not municipality:
                raise ValueError("missing municipality")
            occurred_at = parse_datetime(row)
            latitude, longitude = coordinate(row, municipality, commune_indexes)
            surface_ha = parse_number(first(row, "surface_ha"), default=0.0)
            if row.get("surface_parcourue_m2", "").strip():
                surface_ha /= 10_000.0
            cause = first(row, "cause") or "Inconnue"
            external_id = first(row, "external_id")
            if row.get("numero", "").strip() and first(row, "year"):
                external_id = f"{first(row, 'year')}-{row['numero'].strip()}"
            if not external_id:
                identity = f"{occurred_at}|{municipality}|{surface_ha:.4f}|{cause}"
                external_id = "bdiff-" + hashlib.sha256(identity.encode()).hexdigest()[:20]
            normalized_rows.append(
                (external_id, occurred_at, municipality, latitude, longitude, surface_ha, cause)
            )
        except ValueError as error:
            rejected.append(f"line {line_number}: {error}")
    total = len(normalized_rows) + len(rejected)
    reject_ratio = len(rejected) / total if total else 0.0
    if reject_ratio > max_reject_ratio:
        preview = "; ".join(rejected[:5])
        raise ValueError(
            f"rejected {len(rejected)}/{total} BDIFF rows ({preview})"
        )
    if not normalized_rows:
        raise ValueError("BDIFF export contains no usable rows")
    if rejected:
        preview = "; ".join(rejected[:5])
        print(
            f"Warning: excluded {len(rejected)}/{total} rows without a reliable current municipality centre ({preview})",
            file=sys.stderr,
        )
    with Path(destination).open("w", encoding="utf-8", newline="") as handle:
        writer = csv.writer(handle)
        writer.writerow(("external_id", "occurred_at", "municipality", "latitude", "longitude", "surface_ha", "cause"))
        writer.writerows(normalized_rows)
    return len(normalized_rows), len(rejected)


def main():
    parser = argparse.ArgumentParser(description="Normalize a public BDIFF CSV export")
    parser.add_argument("--input", required=True)
    parser.add_argument("--communes", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--max-reject-ratio", type=float, default=0.01)
    args = parser.parse_args()
    try:
        if not 0.0 <= args.max_reject_ratio < 1.0:
            raise ValueError("max reject ratio must be between zero and one")
        count, rejected = normalize_export(
            args.input, args.communes, args.output, args.max_reject_ratio
        )
    except (OSError, ValueError, json.JSONDecodeError) as error:
        print(f"BDIFF normalization failed: {error}", file=sys.stderr)
        return 1
    print(f"Normalized {count} BDIFF records ({rejected} excluded) into {args.output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
