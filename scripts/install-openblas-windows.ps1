# install-openblas-windows.ps1
#
# Installs OpenBLAS (with LAPACK) for Windows MSVC builds and exports
# OPENBLAS_DIR + OPENBLAS_LIB_DIR so rlx-cpu, openblas-src, and any other
# `cargo:rustc-link-lib=openblas` consumer can find openblas.lib at link
# time and openblas.dll at runtime.
#
# Why upstream OpenBLAS rather than vcpkg:
#   vcpkg openblas:x64-windows omits LAPACK from openblas.dll. rlx-cpu's
#   `blas` feature calls dgesv_ / sgesv_ (LAPACK linear-system solvers), so
#   linking against the vcpkg DLL fails with LNK2019. The upstream
#   OpenBLAS Windows release archive bundles LAPACK into libopenblas.dll
#   along with the import lib, so a single download covers BLAS + LAPACK.
#
# Why the rename step:
#   Upstream ships `libopenblas.lib` / `libopenblas.dll` (MinGW-style
#   naming). The Rust ecosystem (rlx-cpu, openblas-src, blas-src, etc.)
#   emits `cargo:rustc-link-lib=openblas`, which MSVC's link.exe resolves
#   to `openblas.lib`. We create that name alongside the originals so both
#   conventions work.
#
# Outputs:
#   $env:OPENBLAS_DIR — root containing lib\openblas.lib + bin\openblas.dll
#   $env:OPENBLAS_LIB_DIR — convenience alias (== $OPENBLAS_DIR\lib)
#   $env:OPENBLAS_DLL — absolute path to openblas.dll (for staging)
#   $env:OPENBLAS_RUNTIME_DLLS — semicolon-separated list of MinGW runtime
#     DLLs (libgcc_s_seh-1.dll, libgfortran-5.dll, libquadmath-0.dll,
#     libwinpthread-1.dll) that need to ship alongside openblas.dll.
#
#   If $env:GITHUB_ENV / $env:GITHUB_PATH are set, the values are also
#   propagated to subsequent GitHub Actions steps.

$ErrorActionPreference = "Stop"

function Step ($msg) { Write-Host "`n>> $msg" -ForegroundColor Blue  }
function Ok   ($msg) { Write-Host "   $msg"   -ForegroundColor Green }
function Warn ($msg) { Write-Host "   $msg"   -ForegroundColor Yellow }
function Die  ($msg) { Write-Host "`nERROR: $msg" -ForegroundColor Red; exit 1 }

function Export-Env ($name, $value) {
    Set-Item -Path "Env:$name" -Value $value
    if ($env:GITHUB_ENV) {
        "$name=$value" | Out-File -FilePath $env:GITHUB_ENV -Encoding utf8 -Append
    }
}

function Export-Path ($dir) {
    if (-not (Test-Path $dir)) { return }
    $env:PATH = "$dir;$env:PATH"
    if ($env:GITHUB_PATH) {
        $dir | Out-File -FilePath $env:GITHUB_PATH -Encoding utf8 -Append
    }
}

$OpenBlasVersion = if ($env:OPENBLAS_VERSION) { $env:OPENBLAS_VERSION } else { "0.3.30" }
$Root = if ($env:OPENBLAS_INSTALL_DIR) {
    $env:OPENBLAS_INSTALL_DIR
} else {
    "C:\OpenBLAS"
}

function Validate-OpenBLAS ($root) {
    $lib = Join-Path $root "lib\openblas.lib"
    $dll = Join-Path $root "bin\openblas.dll"
    if ((Test-Path $lib) -and (Test-Path $dll)) {
        return [pscustomobject]@{
            Lib = $lib
            Dll = $dll
            Bin = (Join-Path $root "bin")
        }
    }
    return $null
}

# 1. Skip download if already installed and complete.
$found = Validate-OpenBLAS $Root
if (-not $found) {
    Step "Download OpenBLAS $OpenBlasVersion (with LAPACK) -> $Root"
    $zipName = "OpenBLAS-$OpenBlasVersion-x64.zip"
    $url = "https://github.com/OpenMathLib/OpenBLAS/releases/download/v$OpenBlasVersion/$zipName"
    $tempZip = Join-Path $env:TEMP $zipName

    if (-not (Test-Path $tempZip)) {
        $ok = $false
        for ($i = 1; $i -le 5; $i++) {
            try {
                Invoke-WebRequest -Uri $url -OutFile $tempZip -UseBasicParsing
                $ok = $true
                break
            } catch {
                Warn "download attempt $i failed: $($_.Exception.Message)"
                Start-Sleep -Seconds ($i * 2)
            }
        }
        if (-not $ok) { Die "failed to download $url after 5 attempts" }
    }

    if (Test-Path $Root) {
        Remove-Item -Recurse -Force -LiteralPath $Root -ErrorAction SilentlyContinue
    }
    New-Item -ItemType Directory -Force -Path $Root | Out-Null
    Expand-Archive -Path $tempZip -DestinationPath $Root -Force

    # Upstream ships `libopenblas.lib` + `libopenblas.dll`. Create
    # openblas.lib / openblas.dll aliases so the standard
    # `cargo:rustc-link-lib=openblas` directive resolves.
    $libDir = Join-Path $Root "lib"
    $binDir = Join-Path $Root "bin"
    foreach ($pair in @(@($libDir, "libopenblas.lib", "openblas.lib"),
                        @($binDir, "libopenblas.dll", "openblas.dll"))) {
        $dir = $pair[0]; $src = Join-Path $dir $pair[1]; $dst = Join-Path $dir $pair[2]
        if ((Test-Path $src) -and -not (Test-Path $dst)) {
            Copy-Item $src $dst -Force
            Ok "aliased $($pair[1]) -> $($pair[2])"
        }
    }
}

$found = Validate-OpenBLAS $Root
if (-not $found) {
    Die "openblas.lib / openblas.dll not found under $Root after install"
}

# Upstream OpenBLAS 0.3.30 ships libopenblas.dll statically linked against
# the MinGW runtime — `dumpbin /dependents` shows only KERNEL32.dll +
# msvcrt.dll, so we don't need to bundle libgcc/libgfortran/etc. If a
# future OpenBLAS release switches to a non-static build, look at the
# `OPENBLAS_RUNTIME_DLLS` history in git for the previous code path.
Export-Env "OPENBLAS_DIR" $Root
Export-Env "OPENBLAS_LIB_DIR" (Join-Path $Root "lib")
Export-Env "OPENBLAS_DLL" $found.Dll
Export-Path $found.Bin

Ok "OPENBLAS_DIR=$Root"
Ok "OPENBLAS_DLL=$($found.Dll)"
