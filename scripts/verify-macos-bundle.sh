#!/bin/bash
# Runs only on the disposable GitHub macOS runner, using isolated application data.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BUNDLE="$ROOT/target/aarch64-apple-darwin/release/bundle"
APP="$BUNDLE/macos/忆涟千言.app"
DMG=("$BUNDLE"/dmg/*.dmg)
EVIDENCE="$RUNNER_TEMP/macos-smoke"
mkdir -p "$EVIDENCE/workspace" "$EVIDENCE/data" "$EVIDENCE/mount"

[[ "${#DMG[@]}" == 1 && -f "${DMG[0]}" ]]
hdiutil verify "${DMG[0]}" > "$EVIDENCE/dmg-verify.txt"
hdiutil attach -readonly -nobrowse -mountpoint "$EVIDENCE/mount" "${DMG[0]}"
pid=""
cleanup() {
  if [[ -n "$pid" ]] && kill -0 "$pid" 2>/dev/null; then
    kill "$pid"
    wait "$pid" || true
  fi
  hdiutil detach "$EVIDENCE/mount"
}
trap cleanup EXIT
ditto "$EVIDENCE/mount/忆涟千言.app" "$EVIDENCE/Installed.app"
APP="$EVIDENCE/Installed.app"
codesign --verify --deep --strict --verbose=2 "$APP" 2> "$EVIDENCE/codesign.txt"
EXECUTABLE=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleExecutable' "$APP/Contents/Info.plist")
VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
[[ "$VERSION" == "1.2.0" ]]
lipo -archs "$APP/Contents/MacOS/$EXECUTABLE" | tee "$EVIDENCE/architecture.txt"
[[ "$(lipo -archs "$APP/Contents/MacOS/$EXECUTABLE")" == "arm64" ]]
du -sk "$APP" > "$EVIDENCE/app-size-kib.txt"

export YILIAN_WORKSPACE="$EVIDENCE/workspace"
export YILIAN_DATA_DIR="$EVIDENCE/data"
"$APP/Contents/MacOS/$EXECUTABLE" > "$EVIDENCE/startup.txt" 2>&1 &
pid=$!
ready=false
for ((attempt=0; attempt<60; attempt++)); do
  kill -0 "$pid"
  if curl --fail --silent http://127.0.0.1:9420/api/health > "$EVIDENCE/health.json"; then
    ready=true
    break
  fi
  sleep 1
done
[[ "$ready" == true ]]
python3 - "$EVIDENCE/health.json" <<'PY'
import json, sys
health = json.load(open(sys.argv[1]))
assert health['status'] == 'healthy', health
assert health['version'] == '1.2.0', health
assert health['database'] == 'healthy', health
PY
echo 'elapsed_seconds,rss_kib,data_kib' > "$EVIDENCE/idle-resources.csv"
for ((sample=0; sample<12; sample++)); do
  kill -0 "$pid"
  rss=$(ps -o rss= -p "$pid" | tr -d ' ')
  data=$(du -sk "$EVIDENCE/data" | awk '{print $1}')
  echo "$((sample * 5)),$rss,$data" >> "$EVIDENCE/idle-resources.csv"
  sleep 5
done
kill "$pid"
wait "$pid" || true
pid=""
if curl --fail --silent --max-time 2 http://127.0.0.1:9420/api/health; then
  echo 'Backend remains available after application exit' >&2
  exit 1
fi
printf 'DMG integrity, arm64 architecture, ad-hoc signature, v1.2.0 startup and shutdown: PASS\n' | tee "$EVIDENCE/result.txt"
