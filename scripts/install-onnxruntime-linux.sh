#!/usr/bin/env bash
set -euo pipefail

# Installs ONNX Runtime Linux x64 into the shared cache location and exports
# environment variables for subsequent GitHub Actions steps.

ORT_VERSION="${ORT_VERSION:-1.23.2}"
ORT_ROOT="${HOME}/.cache/onnxruntime/onnxruntime-linux-x64-${ORT_VERSION}"

if [[ ! -f "${ORT_ROOT}/lib/libonnxruntime.so" ]]; then
  TMP_DIR="$(mktemp -d)"
  curl -fL --retry 8 --retry-delay 2 --retry-all-errors \
    "https://github.com/microsoft/onnxruntime/releases/download/v${ORT_VERSION}/onnxruntime-linux-x64-${ORT_VERSION}.tgz" \
    -o "${TMP_DIR}/onnxruntime.tgz"

  tar -xzf "${TMP_DIR}/onnxruntime.tgz" -C "${TMP_DIR}"
  EXTRACTED_DIR="$(find "${TMP_DIR}" -maxdepth 1 -type d -name 'onnxruntime-linux-x64-*' | head -1)"

  if [[ -z "${EXTRACTED_DIR}" ]]; then
    echo "Failed to find extracted ONNX Runtime directory" >&2
    exit 1
  fi

  mkdir -p "${HOME}/.cache/onnxruntime"
  rm -rf "${ORT_ROOT}"
  mv "${EXTRACTED_DIR}" "${ORT_ROOT}"
  rm -rf "${TMP_DIR}"
fi

test -f "${ORT_ROOT}/lib/libonnxruntime.so"

if [[ -n "${GITHUB_ENV:-}" ]]; then
  echo "ORT_LIB_LOCATION=${ORT_ROOT}/lib" >> "${GITHUB_ENV}"
  echo "ORT_PREFER_DYNAMIC_LINK=1" >> "${GITHUB_ENV}"
  echo "LD_LIBRARY_PATH=${ORT_ROOT}/lib:${LD_LIBRARY_PATH:-}" >> "${GITHUB_ENV}"
else
  export ORT_LIB_LOCATION="${ORT_ROOT}/lib"
  export ORT_PREFER_DYNAMIC_LINK=1
  export LD_LIBRARY_PATH="${ORT_ROOT}/lib:${LD_LIBRARY_PATH:-}"
fi

echo "[ok] ONNX Runtime ready at ${ORT_ROOT}/lib"
