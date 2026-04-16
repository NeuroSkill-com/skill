#!/usr/bin/env node
// SPDX-License-Identifier: GPL-3.0-only
// Validate src-tauri/manifest.xml for Windows SxS safety and BLE compatibility.
// Cross-platform replacement for check_windows_manifest.py.

import { readFileSync } from "fs";

const NS_ASM_V1 = "urn:schemas-microsoft-com:asm.v1";
const NS_COMPAT_V1 = "urn:schemas-microsoft-com:compatibility.v1";
const NS_ASM_V3 = "urn:schemas-microsoft-com:asm.v3";
const NS_WIN_2005 = "http://schemas.microsoft.com/SMI/2005/WindowsSettings";
const NS_WIN_2016 = "http://schemas.microsoft.com/SMI/2016/WindowsSettings";
const WINDOWS_10_11_OS_ID = "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}";
const COMMON_CONTROLS_TOKEN = "6595b64144ccf1df";

const path = process.argv[2] || "src-tauri/manifest.xml";
let xml;
try {
  xml = readFileSync(path, "utf8");
} catch {
  fail(`Manifest not found: ${path}`);
}

function fail(msg) {
  console.error(`[error] ${msg}`);
  process.exit(1);
}

// Simple XML attribute extractor (avoids external deps)
function attr(tag, name) {
  const re = new RegExp(`${name}\\s*=\\s*["']([^"']*)["']`);
  const m = tag.match(re);
  return m ? m[1] : null;
}

function findTag(text, localName, ns) {
  // Match both prefixed and default-ns forms
  const patterns = [
    new RegExp(`<[^>]*?${localName}[^>]*xmlns(?::\\w+)?\\s*=\\s*["']${ns.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}["'][^>]*>`, "s"),
    new RegExp(`<(?:\\w+:)?${localName}[^>]*>`, "s"),
  ];
  for (const re of patterns) {
    const m = text.match(re);
    if (m) return m[0];
  }
  return null;
}

function textContent(text, localName, ns) {
  const re = new RegExp(`<(?:\\w+:)?${localName}[^>]*xmlns(?::\\w+)?\\s*=\\s*["']${ns.replace(/[.*+?^${}()|[\]\\]/g, "\\$&")}["'][^>]*>([^<]*)<`, "s");
  const m = text.match(re);
  return m ? m[1].trim() : null;
}

// Root assembly check
if (!xml.includes(NS_ASM_V1)) fail("Root element must reference urn:schemas-microsoft-com:asm.v1");

// assemblyIdentity
const asmId = findTag(xml, "assemblyIdentity", NS_ASM_V1);
if (!asmId) fail("Missing required element: assemblyIdentity");
if (attr(asmId, "type") !== "win32") fail("assemblyIdentity@type must be 'win32'");
if (!attr(asmId, "name")) fail("assemblyIdentity@name must be non-empty");
const version = attr(asmId, "version") || "";
if (!/^\d+\.\d+\.\d+\.\d+$/.test(version)) fail("assemblyIdentity@version must match 'A.B.C.D'");

// compatibility/supportedOS
const supportedOS = findTag(xml, "supportedOS", NS_COMPAT_V1);
if (!supportedOS) fail("Missing required element: compatibility/application/supportedOS");
if (attr(supportedOS, "Id") !== WINDOWS_10_11_OS_ID) fail("supportedOS@Id must be the Windows 10/11 GUID");

// maxversiontested
const maxVer = findTag(xml, "maxversiontested", NS_COMPAT_V1);
if (!maxVer) fail("Missing required element: compatibility/application/maxversiontested");
const maxVerId = attr(maxVer, "Id") || "";
if (!/^\d+\.\d+\.\d+\.\d+$/.test(maxVerId)) fail("maxversiontested@Id must match 'A.B.C.D'");

// Common Controls dependency
const depBlocks = xml.match(/<assemblyIdentity[^>]*>/g) || [];
const ccDep = depBlocks.find((b) => (attr(b, "name") || "").includes("Common-Controls"));
if (!ccDep) fail("Missing dependency assemblyIdentity for Microsoft.Windows.Common-Controls");
if (attr(ccDep, "name") !== "Microsoft.Windows.Common-Controls") fail("dependency assemblyIdentity@name must be 'Microsoft.Windows.Common-Controls'");
if (attr(ccDep, "version") !== "6.0.0.0") fail("dependency assemblyIdentity@version must be '6.0.0.0'");
if (attr(ccDep, "publicKeyToken") !== COMMON_CONTROLS_TOKEN) fail("dependency assemblyIdentity@publicKeyToken must match Microsoft Common-Controls token");

// DPI settings
const dpiAware = textContent(xml, "dpiAware", NS_WIN_2005);
if (!dpiAware || !["true", "true/pm"].includes(dpiAware.toLowerCase())) fail("dpiAware text must be 'true' or 'true/pm'");

const dpiAwareness = textContent(xml, "dpiAwareness", NS_WIN_2016);
if (dpiAwareness !== "PerMonitorV2") fail("dpiAwareness text must be 'PerMonitorV2'");

console.log(`[ok] Windows manifest validated: ${path}`);
