# Rebuild the mockups and capture every screen. THEME=<name> SCREENS="1 2" bash shoot.sh
D="$(cd "$(dirname "$0")" && pwd)"
cd "$D" && node build.js inputs/kawaii graphite.src.html graphite.html inputs/markdown
node export.js graphite.html ../../docs/mockups/index.html ${ARTIFACT_OUT:+"$ARTIFACT_OUT"}
CHROME="${CHROME:-/c/Program Files/Google/Chrome/Application/chrome.exe}"
mkdir -p ../review
for s in ${SCREENS:-1 2 3 4 5 6 7 8 9 10 11}; do "$CHROME" --headless=new --disable-gpu --hide-scrollbars --virtual-time-budget=5000 --window-size=1440,900 --screenshot="$D/../review/${THEME:-graphite}-s$s.png" "file:///$D/graphite.html?only=$s&theme=${THEME:-graphite}" >/dev/null 2>&1; done; echo done
