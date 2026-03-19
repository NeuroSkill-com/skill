### Bugfixes

- **Paired Emotiv devices no longer appear in discovered list**: discovered Cortex devices (e.g. `cortex:EPOCPLUS-06F2DDBC`) now correctly match against the legacy `cortex:emotiv` paired entry, so paired headsets show in the "Paired" section instead of "Discovered". On first successful connection, the legacy ID is automatically migrated to the real headset ID.
