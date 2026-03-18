### Bugfixes

- **Session runner re-reads pipeline channels after Emotiv auto-detection**: When Emotiv auto-detects the actual channel count (Insight 5-ch, MN8 2-ch vs assumed EPOC 14-ch), the session runner now picks up the updated `pipeline_channels` on the first EEG frame. Previously the snapshot was taken before any events arrived, so the DSP pipeline would process 14 channels even when only 5 were active.
