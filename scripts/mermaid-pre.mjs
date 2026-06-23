// Post-process marked's HTML so Mermaid can render fenced ```mermaid blocks.
// marked emits <pre><code class="language-mermaid">…escaped…</code></pre>; Mermaid
// wants the raw diagram source inside <pre class="mermaid">. Rewrite + unescape.
import { readFileSync, writeFileSync } from "node:fs";

const file = process.argv[2];
let html = readFileSync(file, "utf8");

html = html.replace(
  /<pre><code class="language-mermaid">([\s\S]*?)<\/code><\/pre>/g,
  (_, code) => {
    const src = code
      .replace(/&lt;/g, "<")
      .replace(/&gt;/g, ">")
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&amp;/g, "&");
    return `<pre class="mermaid">${src}</pre>`;
  },
);

writeFileSync(file, html);
