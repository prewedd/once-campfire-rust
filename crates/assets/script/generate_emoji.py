"""Generates crates/assets/overrides/emoji/{data.json,prompt.html} from emojibase-data 15.3.2.

    npm pack emojibase-data@15.3.2 && tar xzf emojibase-data-15.3.2.tgz
    python3 generate_emoji.py package crates/assets/overrides
"""
import json, sys, html
src, out = sys.argv[1], sys.argv[2]
data = json.load(open(f"{src}/en/data.json"))
shortcodes = json.load(open(f"{src}/en/shortcodes/iamcal.json"))
GROUPS = [(0, "Smileys", "😀"), (1, "People", "👋"), (3, "Nature", "🐻"), (4, "Food", "🍔"), (5, "Travel", "✈️"), (6, "Activities", "⚽"), (7, "Objects", "💡"), (8, "Symbols", "❤️"), (9, "Flags", "🏳️")]
by_group = {g: [] for g, _, _ in GROUPS}
prompt = []
for e in sorted(data, key=lambda e: e.get("order", 1e9)):
    g = e.get("group")
    if g not in by_group or e.get("version", 99) > 14:
        continue
    codes = shortcodes.get(e["hexcode"], [])
    codes = [codes] if isinstance(codes, str) else codes
    label = e.get("label") or e.get("annotation") or ""
    words = " ".join(dict.fromkeys([label.lower()] + [c.replace("_", " ") for c in codes] + [t.lower() for t in e.get("tags", [])]))
    by_group[g].append([e["emoji"], label, " ".join(codes), words])
    if codes:
        prompt.append((e["emoji"], codes[0], " ".join(codes) + " " + label.lower()))
groups = [{"name": n, "icon": i, "emoji": by_group[g]} for g, n, i in GROUPS]
# The picker's emoji: [character, label, Slack shortcodes, search words], in emojibase 15.3.2's
# groups (Emoji 14 and earlier, so they render everywhere). Fetched when the picker first opens.
with open(f"{out}/emoji/data.json", "w") as f:
    json.dump(groups, f, ensure_ascii=False, separators=(",", ":"))
with open(f"{out}/emoji/prompt.html", "w") as f:
    for char, code, search in prompt:
        f.write(f'<lexxy-prompt-item search="{html.escape(search)}"><template type="menu"><b>{char}</b>:{html.escape(code)}:</template><template type="editor">{char}</template></lexxy-prompt-item>\n')
print(sum(len(g["emoji"]) for g in groups), "emoji;", len(prompt), "shortcodes")
