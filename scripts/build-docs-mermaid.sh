#!/usr/bin/env bash
# Render design docs to PDF with Mermaid diagrams:
# Markdown → styled HTML (marked) → rewrite ```mermaid blocks → Chrome headless
# renders Mermaid to SVG under a virtual-time budget → PDF.
# Usage: scripts/build-docs-mermaid.sh <doc-basename> [doc-basename ...]
set -euo pipefail

cd "$(dirname "$0")/.."
DOCS=("$@")
[ ${#DOCS[@]} -gt 0 ] || { echo "usage: $0 <doc-basename> [...]"; exit 1; }
CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"

read -r -d '' HEAD <<'CSS' || true
<style>
  body { font-family: -apple-system, "Segoe UI", Helvetica, Arial, sans-serif;
         font-size: 11pt; line-height: 1.5; color: #1a1a1a; max-width: 50rem; margin: 0 auto; }
  h1, h2, h3, h4 { font-weight: 650; line-height: 1.25; margin-top: 1.4em; }
  h1 { font-size: 1.9em; border-bottom: 2px solid #ddd; padding-bottom: .2em; }
  h2 { font-size: 1.45em; border-bottom: 1px solid #eee; padding-bottom: .15em; }
  code { font-family: "SF Mono", Menlo, Consolas, monospace; font-size: .88em;
         background: #f4f4f6; padding: .1em .35em; border-radius: 3px; }
  pre { background: #f4f4f6; padding: .9em 1.1em; border-radius: 6px; overflow-x: auto; }
  pre code { background: none; padding: 0; }
  pre.mermaid { background: none; padding: 0; text-align: center; margin: 1.4em 0; page-break-inside: avoid; }
  pre.mermaid svg { max-width: 100%; height: auto; }
  table { border-collapse: collapse; width: 100%; margin: 1em 0; font-size: .92em; }
  th, td { border: 1px solid #d0d0d5; padding: .4em .6em; text-align: left; vertical-align: top; }
  th { background: #f0f0f3; }
  blockquote { border-left: 3px solid #c8c8d0; margin: 1em 0; padding: .2em 1em; color: #444; background: #fafafb; }
  a { color: #1a56c4; }
  @page { margin: 1.6cm 1.4cm; }
</style>
<script type="module">
  import mermaid from "https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs";
  mermaid.initialize({ startOnLoad: false, securityLevel: "loose", theme: "neutral" });
  await mermaid.run({ querySelector: "pre.mermaid" });
</script>
CSS

for doc in "${DOCS[@]}"; do
  md="docs/${doc}.md"
  html="docs/.${doc}.gen.html"
  pdf="docs/${doc}.pdf"
  [ -f "$md" ] || { echo "skip: $md not found"; continue; }
  { printf '<!doctype html><html><head><meta charset="utf-8">%s</head><body>\n' "$HEAD";
    npx -y marked -i "$md";
    printf '\n</body></html>'; } > "$html"
  node scripts/mermaid-pre.mjs "$html"
  "$CHROME" --headless --disable-gpu --no-pdf-header-footer --virtual-time-budget=20000 \
    --print-to-pdf="$pdf" "file://$(pwd)/$html" 2>/dev/null
  rm -f "$html"
  echo "wrote $pdf"
done
