#!/bin/bash
# Verify that all metric columns are populated in the latest eeg.sqlite
SKILL_DIR="${HOME}/.skill"
LATEST=$(ls -d ${SKILL_DIR}/20* 2>/dev/null | sort -r | head -1)
if [ -z "$LATEST" ]; then echo "No skill data found"; exit 1; fi
DB="${LATEST}/eeg.sqlite"
if [ ! -f "$DB" ]; then echo "No eeg.sqlite in $LATEST"; exit 1; fi
echo "Checking: $DB"
echo ""
echo "=== Schema ==="
sqlite3 "$DB" ".schema embeddings"
echo ""
echo "=== Row count ==="
sqlite3 "$DB" "SELECT COUNT(*) as total FROM embeddings;"
echo ""
echo "=== Non-NULL counts per metric column ==="
for col in rel_delta rel_theta rel_alpha rel_beta rel_gamma rel_high_gamma \
           focus_score relaxation_score engagement_score faa \
           tar bar dtr pse apf bps snr coherence mu_suppression mood \
           ppg_ambient ppg_infrared ppg_red band_channels_json; do
  count=$(sqlite3 "$DB" "SELECT COUNT(*) FROM embeddings WHERE $col IS NOT NULL;" 2>/dev/null || echo "N/A")
  printf "  %-22s %s\n" "$col" "$count"
done
echo ""
echo "=== Last 3 rows (key metrics) ==="
sqlite3 -header -column "$DB" \
  "SELECT id, timestamp, rel_delta, tar, bar, dtr, pse, mood, band_channels_json IS NOT NULL as has_channels
   FROM embeddings ORDER BY id DESC LIMIT 3;"
