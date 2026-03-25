#!/usr/bin/env python3
"""Validate src-tauri/manifest.xml for Windows SxS safety and BLE compatibility."""

from __future__ import annotations

import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

NS_ASM_V1 = "urn:schemas-microsoft-com:asm.v1"
NS_COMPAT_V1 = "urn:schemas-microsoft-com:compatibility.v1"
NS_ASM_V3 = "urn:schemas-microsoft-com:asm.v3"
NS_WIN_2005 = "http://schemas.microsoft.com/SMI/2005/WindowsSettings"
NS_WIN_2016 = "http://schemas.microsoft.com/SMI/2016/WindowsSettings"

WINDOWS_10_11_OS_ID = "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"
COMMON_CONTROLS_TOKEN = "6595b64144ccf1df"


def fail(msg: str) -> None:
    print(f"[error] {msg}", file=sys.stderr)
    raise SystemExit(1)


def find_required(elem: ET.Element, path: str, desc: str) -> ET.Element:
    found = elem.find(path)
    if found is None:
        fail(f"Missing required element: {desc}")
    return found


def main() -> int:
    manifest_path = Path(sys.argv[1]) if len(sys.argv) > 1 else Path("src-tauri/manifest.xml")
    if not manifest_path.is_file():
        fail(f"Manifest not found: {manifest_path}")

    try:
        tree = ET.parse(manifest_path)
    except ET.ParseError as exc:
        fail(f"XML parse error in {manifest_path}: {exc}")

    root = tree.getroot()
    if root.tag != f"{{{NS_ASM_V1}}}assembly":
        fail("Root element must be <assembly xmlns='urn:schemas-microsoft-com:asm.v1'>")

    assembly_identity = find_required(root, f"{{{NS_ASM_V1}}}assemblyIdentity", "assemblyIdentity")
    if assembly_identity.attrib.get("type") != "win32":
        fail("assemblyIdentity@type must be 'win32'")
    if assembly_identity.attrib.get("name", "").strip() == "":
        fail("assemblyIdentity@name must be non-empty")
    version = assembly_identity.attrib.get("version", "")
    if not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", version):
        fail("assemblyIdentity@version must match 'A.B.C.D'")

    compat_app = find_required(
        root,
        f"{{{NS_COMPAT_V1}}}compatibility/{{{NS_COMPAT_V1}}}application",
        "compatibility/application",
    )
    supported_os = find_required(
        compat_app,
        f"{{{NS_COMPAT_V1}}}supportedOS",
        "compatibility/application/supportedOS",
    )
    if supported_os.attrib.get("Id") != WINDOWS_10_11_OS_ID:
        fail("supportedOS@Id must be the Windows 10/11 GUID")

    max_version = find_required(
        compat_app,
        f"{{{NS_COMPAT_V1}}}maxversiontested",
        "compatibility/application/maxversiontested",
    )
    max_version_id = max_version.attrib.get("Id", "")
    if not re.fullmatch(r"\d+\.\d+\.\d+\.\d+", max_version_id):
        fail("maxversiontested@Id must match 'A.B.C.D'")

    dep_identity = find_required(
        root,
        f"{{{NS_ASM_V1}}}dependency/{{{NS_ASM_V1}}}dependentAssembly/{{{NS_ASM_V1}}}assemblyIdentity",
        "dependency/dependentAssembly/assemblyIdentity",
    )
    if dep_identity.attrib.get("name") != "Microsoft.Windows.Common-Controls":
        fail("dependency assemblyIdentity@name must be 'Microsoft.Windows.Common-Controls'")
    if dep_identity.attrib.get("version") != "6.0.0.0":
        fail("dependency assemblyIdentity@version must be '6.0.0.0'")
    if dep_identity.attrib.get("publicKeyToken") != COMMON_CONTROLS_TOKEN:
        fail("dependency assemblyIdentity@publicKeyToken must match Microsoft Common-Controls token")

    windows_settings = find_required(
        root,
        f"{{{NS_ASM_V3}}}application/{{{NS_ASM_V3}}}windowsSettings",
        "application/windowsSettings",
    )
    dpi_aware = find_required(windows_settings, f"{{{NS_WIN_2005}}}dpiAware", "windowsSettings/dpiAware")
    if dpi_aware.text is None or dpi_aware.text.strip().lower() not in {"true", "true/pm"}:
        fail("dpiAware text must be 'true' or 'true/pm'")

    dpi_awareness = find_required(
        windows_settings,
        f"{{{NS_WIN_2016}}}dpiAwareness",
        "windowsSettings/dpiAwareness",
    )
    if dpi_awareness.text is None or dpi_awareness.text.strip() != "PerMonitorV2":
        fail("dpiAwareness text must be 'PerMonitorV2'")

    print(f"[ok] Windows manifest validated: {manifest_path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
