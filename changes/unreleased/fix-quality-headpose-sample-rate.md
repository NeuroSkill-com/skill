### Bugfixes

- **QualityMonitor window size now matches device sample rate**: Added `QualityMonitor::with_window(channels, window)` and wired it to use the device's EEG sample rate (≈1 second window). Previously the window was hardcoded to 256 samples — only 0.51 s at 500 Hz (MW75) or 2 s at 128 Hz (Emotiv), causing quality to be assessed over inconsistent time windows.
- **HeadPoseTracker IMU rate now configurable**: Added `HeadPoseTracker::with_imu_rate(hz)`. Gyro integration (`dt`), stillness EMA, gesture window, and refractory period all used the hardcoded Muse IMU rate (52 Hz). At different rates, `dt = 1/52` would produce wrong pitch/roll/yaw accumulation and incorrect stillness scores.
