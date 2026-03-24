### Bugfixes

- **Ensure Windows release bundles ONNX Runtime DLL**: release workflow now installs ONNX Runtime 1.23.2 from Microsoft releases, exports `ORT_LIB_LOCATION` / `ORT_PREFER_DYNAMIC_LINK`, and stages `onnxruntime.dll` into release output directories before NSIS packaging.
