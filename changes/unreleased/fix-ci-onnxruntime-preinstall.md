### Bugfixes

- **Stabilize CI ORT linking**: preinstall ONNX Runtime 1.23.2 from Microsoft releases on Linux/Windows CI jobs, then export `ORT_LIB_LOCATION` and `ORT_PREFER_DYNAMIC_LINK=1` so `ort-sys` does not depend on flaky `cdn.pyke.io` downloads during clippy/test runs.
