#!/bin/bash
# Check the latest per-day EEG store (YYYYMMDD/eeg.duckdb): row counts and
# how many epochs carry each key metric in metrics_json.
#
# Needs the DuckDB CLI (brew install duckdb). DuckDB lets one process write a
# database at a time, so quit NeuroSkill (and its daemon) first.
SKILL_DIR="${SKILL_DIR:-${HOME}/.skill}"
LATEST=$(ls -d "${SKILL_DIR}"/20* 2>/dev/null | sort -r | head -1)
if [ -z "$LATEST" ]; then echo "No skill data found"; exit 1; fi
DB="${LATEST}/eeg.duckdb"
if [ ! -f "$DB" ]; then echo "No eeg.duckdb in $LATEST"; exit 1; fi
command -v duckdb >/dev/null || { echo "duckdb CLI not found (brew install duckdb)"; exit 1; }

q() { duckdb -readonly "$DB" "$@"; }
echo "Checking: $DB"
echo
echo "=== Schema ==="
q -c "DESCRIBE embeddings"
echo
echo "=== Rows ==="
q -c "SELECT count(*) AS total,
             count(*) FILTER (WHERE octet_length(eeg_embedding) >= 4) AS embedded,
             count(*) FILTER (WHERE metrics_json IS NOT NULL) AS with_metrics,
             count(*) FILTER (WHERE unix_ms IS NULL) AS missing_unix_ms
      FROM embeddings"
echo
echo "=== Epochs carrying each metric ==="
for key in rel_delta rel_theta rel_alpha rel_beta rel_gamma relaxation_score engagement_score \
           faa tar bar dtr pse apf bps snr coherence mu_suppression mood migraine_index hr; do
  n=$(q -noheader -list -c "SELECT count(*) FROM embeddings
        WHERE TRY_CAST(json_extract_string(metrics_json, '\$.$key') AS DOUBLE) IS NOT NULL")
  printf "  %-20s %s\n" "$key" "$n"
done
echo
echo "=== Last 3 rows ==="
q -c "SELECT id, timestamp, unix_ms,
             TRY_CAST(json_extract_string(metrics_json, '\$.tar') AS DOUBLE) AS tar,
             TRY_CAST(json_extract_string(metrics_json, '\$.mood') AS DOUBLE) AS mood
      FROM embeddings ORDER BY id DESC LIMIT 3"
