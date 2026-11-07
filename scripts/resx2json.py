#!/usr/bin/env python3
"""One-off: Resources/Strings{,.fr}.resx -> crates/shared/i18n/{en,fr}.json (key order kept, values verbatim).

Rerun only if the resx files change (they stay the source of truth until the C# app is deleted).
Values are copied raw: .NET composite-format quirks (`{{base}}` in formatted strings) are handled by i18n::tf.
"""
import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

root = Path(__file__).resolve().parent.parent
for src, dst in (("Strings.resx", "en.json"), ("Strings.fr.resx", "fr.json")):
    tree = ET.parse(root / "Resources" / src)
    table = {}
    for d in tree.getroot().iter("data"):
        v = d.find("value")
        table[d.get("name")] = v.text if v is not None and v.text is not None else ""
    out = root / "crates" / "shared" / "i18n" / dst
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(json.dumps(table, ensure_ascii=False, indent=1) + "\n", encoding="utf-8")
    print(f"{dst}: {len(table)} keys", file=sys.stderr)
