# skill-constants

Single source of truth for all NeuroSkill constants.

## Overview

Centralizes numeric, string, and path constants used across the entire workspace so that every crate references the same values.

## Constant groups

| Group | Examples |
|---|---|
| **EEG signal** | `EEG_CHANNELS`, `CHANNEL_NAMES`, `MUSE_SAMPLE_RATE`, `GANGLION_CHANNEL_NAMES` |
| **DSP filter** | `FILTER_WINDOW`, `FILTER_HOP`, `FILTER_OVERLAP`, `DEFAULT_LP_HZ`, `DEFAULT_HP_HZ`, `DEFAULT_NOTCH_BW_HZ` |
| **Frequency bands** | `NUM_BANDS`, `BANDS` (delta–high-gamma ranges), `BAND_COLORS`, `BAND_SYMBOLS` |
| **Spectrogram** | `SPEC_N_FREQ`, `BAND_WINDOW`, `BAND_HOP` |
| **Embedding model** | `EMBEDDING_EPOCH_SECS`, `EMBEDDING_OVERLAP_SECS`, `ZUNA_HF_REPO`, `ZUNA_WEIGHTS_FILE`, `ZUNA_DATA_NORM` |
| **HNSW index** | `HNSW_M`, `HNSW_EF_CONSTRUCTION` |
| **File layout** | `LABELS_FILE`, `SCREENSHOTS_SQLITE`, `SCREENSHOTS_DIR`, `MODEL_CONFIG_FILE`, `UMAP_CONFIG_FILE` |
| **Screenshots** | `SCREENSHOTS_HNSW`, `SCREENSHOTS_OCR_HNSW`, `SCREENSHOT_HNSW_SAVE_EVERY`, `OCR_DETECTION_MODEL_URL` |
| **Onboarding** | `ONBOARDING_MODEL_DOWNLOAD_ORDER` |

## Dependencies

None — pure constants, no external dependencies.
