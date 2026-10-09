# English screenshots for the site: translate the built mockup (en.json), then capture each screen
# exactly like shoot.sh does. Run after shoot.sh/build.js so graphite.html is current.
#   CHROME=... bash shoot-en.sh        (default: macOS Chrome)
D="$(cd "$(dirname "$0")" && pwd)"
CHROME="${CHROME:-/Applications/Google Chrome.app/Contents/MacOS/Google Chrome}"
OUT="$D/../../site/assets/screens/en"
cd "$D" && node translate-en.js graphite.html graphite.en.html || exit 1
mkdir -p "$OUT"
# the screens the site uses (see site/index.html, site/docs.html, site/site.js)
for s in 3 4 6 7 8 9 10 11; do
  "$CHROME" --headless=new --disable-gpu --hide-scrollbars --virtual-time-budget=5000 --window-size=1440,900 \
    --screenshot="$OUT/$(printf %02d "$s").png" "file://$D/graphite.en.html?only=$s&theme=graphite" >/dev/null 2>&1
done
rm -f graphite.en.html; echo done
