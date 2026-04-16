#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-only
# Free disk space on GitHub Actions Linux runners by removing unused toolchains.

set -euo pipefail

sudo rm -rf /usr/local/lib/android
sudo rm -rf /usr/share/dotnet
sudo rm -rf /opt/ghc
sudo rm -rf /usr/local/.ghcup
sudo rm -rf /usr/local/share/powershell
sudo rm -rf /usr/local/share/chromium
sudo rm -rf /usr/share/swift
sudo rm -rf /opt/hostedtoolcache/CodeQL
sudo docker image prune -af 2>/dev/null || true
df -h /
