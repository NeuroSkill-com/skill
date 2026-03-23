### Bugfixes

- **Windows app manifest for BLE access**: Added a custom Windows application manifest (`manifest.xml`) declaring Windows 10/11 compatibility via `supportedOS` and `maxversiontested`. Without this, Windows 11 may deny WinRT Bluetooth Low Energy API access to unpackaged desktop apps. Also includes Common Controls v6 and per-monitor DPI awareness v2.
