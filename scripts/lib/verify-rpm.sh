#!/usr/bin/env bash
# Checks run INSIDE the Fedora container against one NeuroSkill .rpm.
#
# Mounted rather than baked into Dockerfile.rpm-verify so a check can be edited
# without rebuilding the image. Driven by scripts/verify-rpm-docker.sh.
#
# Usage (inside the container):
#   verify-rpm.sh /work/pkg/neuroskill-<ver>.<arch>.rpm <expected-arch>
#
# Exits non-zero on the first failed check, with the reason on stderr.
set -uo pipefail

rpm_file="${1:?usage: verify-rpm.sh <rpm> <expected-arch>}"
expect_arch="${2:?usage: verify-rpm.sh <rpm> <expected-arch>}"

pass=0
fail=0

ok()   { printf '  \033[32mPASS\033[0m  %s\n' "$1"; pass=$((pass + 1)); }
bad()  { printf '  \033[31mFAIL\033[0m  %s\n' "$1" >&2; fail=$((fail + 1)); }
note() { printf '        %s\n' "$1"; }

echo "== metadata =="

q() { rpm -qp --queryformat "$1" "$rpm_file" 2>/dev/null; }

name="$(q '%{NAME}')"
version="$(q '%{VERSION}')"
release="$(q '%{RELEASE}')"
arch="$(q '%{ARCH}')"
license="$(q '%{LICENSE}')"

note "NAME=$name VERSION=$version RELEASE=$release ARCH=$arch LICENSE=$license"

[[ "$name" == "neuroskill" ]] && ok "package name is neuroskill" || bad "name is '$name', expected neuroskill"
[[ "$arch" == "$expect_arch" ]] && ok "arch is $expect_arch" || bad "arch is '$arch', expected '$expect_arch'"
[[ -n "$license" && "$license" != "(none)" ]] && ok "license present ($license)" || bad "no License in metadata"

# rpm forbids '-' in Version (it delimits Version from Release). A stray
# backslash means a quoting bug in the packaging script leaked through.
case "$version" in
  *-*)   bad "VERSION '$version' contains '-', which rpm forbids" ;;
  *'\'*) bad "VERSION '$version' contains a backslash (quoting bug)" ;;
  "")    bad "VERSION is empty" ;;
  *)     ok "VERSION is well-formed" ;;
esac

# A pre-release must sort BELOW the release it precedes, which in rpm means a
# tilde. Anything else (a dot, say) sorts ABOVE, so an RC would look newer than
# the final release and `dnf upgrade` would never offer the stable build.
if [[ "$version" == *rc* ]]; then
  if [[ "$version" == *'~'* ]]; then
    base="${version%%'~'*}"
    cmp="$(rpm --eval "%{lua:print(rpm.vercmp('$version','$base'))}" 2>/dev/null)"
    if [[ "$cmp" == "-1" ]]; then
      ok "RC sorts below its release ($version < $base)"
    else
      bad "RC does NOT sort below its release: vercmp($version,$base)=$cmp"
    fi
  else
    bad "VERSION '$version' looks like an RC but has no '~' — upgrade ordering inverts"
  fi
else
  ok "not an RC; no tilde ordering to check"
fi

echo "== dependencies =="

reqs="$(rpm -qp --requires "$rpm_file" 2>/dev/null)"
if grep -q 'libopenblas\.so\.0' <<<"$reqs"; then
  ok "auto-generated OpenBLAS soname requirement present"
else
  bad "no libopenblas.so.0 requirement — auto dep generation did not run"
fi

# A literal `Requires: openblas` is a portability trap: the package providing the
# library is named differently on Fedora/RHEL (openblas-serial / -threads /
# -openmp) and openSUSE (libopenblas0), so the bare name is unsatisfiable on some
# of them and blocks installation outright.
if grep -qx 'openblas' <<<"$reqs"; then
  bad "hard 'Requires: openblas' present — unsatisfiable on distros without that exact package"
else
  ok "no distro-specific bare 'openblas' requirement"
fi

echo "== install =="

if dnf -y install "$rpm_file" >/tmp/dnf.log 2>&1; then
  ok "dnf install resolved every dependency and installed"
else
  bad "dnf install FAILED"
  tail -25 /tmp/dnf.log >&2
  echo; echo "  $pass passed, $fail failed"; exit 1
fi

echo "== installed layout =="

for path in /usr/bin/neuroskill /opt/neuroskill /usr/share/applications/neuroskill.desktop; do
  [[ -e "$path" ]] && ok "exists: $path" || bad "missing: $path"
done

# rpm -V reports files whose size/mode/digest drift from the manifest. Config
# files legitimately differ; nothing here is marked %config, so any output is a
# real packaging problem.
verify_out="$(rpm -V neuroskill 2>&1)"
if [[ -z "$verify_out" ]]; then
  ok "rpm -V clean (every installed file matches the manifest)"
else
  bad "rpm -V reported drift:"
  sed 's/^/        /' <<<"$verify_out" >&2
fi

echo "== binaries =="

# /usr/bin/neuroskill is a bash launcher that sets NEUTTS_SAMPLES_DIR and friends
# before exec'ing the real app out of /opt/neuroskill. Checking *it* for an ELF
# architecture is meaningless, and so is running ldd against it — an earlier
# version of this script did both and passed for the wrong reason.
launcher="/usr/bin/neuroskill"
if [[ -f "$launcher" ]]; then
  if [[ -x "$launcher" ]]; then
    ok "launcher $launcher is executable"
  else
    bad "launcher $launcher is not executable"
  fi
  head -1 "$launcher" | grep -q '^#!' && ok "launcher has a shebang" || note "launcher is not a script (fine if it is the binary)"
  grep -q '/opt/neuroskill' "$launcher" 2>/dev/null \
    && ok "launcher points into /opt/neuroskill" \
    || bad "launcher does not reference /opt/neuroskill"
else
  bad "no launcher at $launcher"
fi

case "$expect_arch" in
  x86_64)  want_elf="x86-64" ;;
  aarch64) want_elf="aarch64" ;;
  *)       want_elf="$expect_arch" ;;
esac

# The shipped executables. `skill` is the Tauri app, `skill-daemon` the service,
# `skill-tty` the PTY proxy; all three are installed side by side and all three
# have to match the package's declared architecture and link cleanly.
found_elf=0
while IFS= read -r candidate; do
  desc="$(file -b "$candidate")"
  case "$desc" in
    *ELF*)
      found_elf=$((found_elf + 1))
      name="$(basename "$candidate")"
      if grep -q "$want_elf" <<<"$desc"; then
        ok "$name is $expect_arch"
      else
        bad "$name is not $expect_arch: $desc"
      fi
      # Every NEEDED library must resolve from what dnf just pulled in. This is
      # the check that only a real install on the target distro can make.
      if command -v ldd >/dev/null 2>&1; then
        missing="$(ldd "$candidate" 2>/dev/null | grep 'not found' || true)"
        if [[ -z "$missing" ]]; then
          ok "$name: all dynamic libraries resolve"
        else
          bad "$name: unresolved libraries:"
          sed 's/^/        /' <<<"$missing" >&2
        fi
      fi
      ;;
  esac
done < <(find /opt/neuroskill -maxdepth 1 -type f 2>/dev/null)

if [[ "$found_elf" -eq 0 ]]; then
  bad "no ELF executables found under /opt/neuroskill"
else
  ok "found $found_elf ELF executable(s) under /opt/neuroskill"
fi

echo "== erase =="

if dnf -y remove neuroskill >/tmp/dnf-rm.log 2>&1; then
  ok "package removes cleanly"
  [[ -e /usr/bin/neuroskill ]] && bad "binary survived removal" || ok "files cleaned up on removal"
else
  bad "dnf remove FAILED"
  tail -15 /tmp/dnf-rm.log >&2
fi

echo
echo "  $pass passed, $fail failed"
[[ "$fail" -eq 0 ]]
