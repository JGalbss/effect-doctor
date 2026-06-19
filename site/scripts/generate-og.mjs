// Renders public/og.png (1200x630) from an inline SVG. Run: node scripts/generate-og.mjs
import { Resvg } from "@resvg/resvg-js"
import { writeFileSync } from "node:fs"

const svg = `<svg xmlns="http://www.w3.org/2000/svg" width="1200" height="630">
  <rect width="1200" height="630" fill="#0a0d13"/>
  <g stroke="#11161f" stroke-width="1">
    ${Array.from({ length: 30 }, (_, i) => `<line x1="${i * 40}" y1="0" x2="${i * 40}" y2="630"/>`).join("")}
    ${Array.from({ length: 16 }, (_, i) => `<line x1="0" y1="${i * 40}" x2="1200" y2="${i * 40}"/>`).join("")}
  </g>
  <path d="M80 330 L300 330 L350 230 L450 430 L500 330 L640 330" fill="none" stroke="#7ee2a8" stroke-width="10" stroke-linecap="round" stroke-linejoin="round" opacity="0.9"/>
  <text x="80" y="190" font-family="Menlo, monospace" font-size="84" font-weight="700" fill="#7ee2a8">effect <tspan fill="#d6dbe7">doctor</tspan></text>
  <text x="80" y="500" font-family="Helvetica, Arial, sans-serif" font-size="34" fill="#7d8799">Scan your Effect TS codebase. Get a score.</text>
  <text x="80" y="550" font-family="Helvetica, Arial, sans-serif" font-size="34" fill="#7d8799">89 rules, every one with the cleaner rewrite.</text>
  <rect x="80" y="575" width="330" height="0" fill="none"/>
  <text x="1120" y="560" text-anchor="end" font-family="Menlo, monospace" font-size="30" fill="#7ee2a8">npx @jgalbsss/agent-doctor</text>
</svg>`

const png = new Resvg(svg, { fitTo: { mode: "width", value: 1200 } }).render().asPng()
writeFileSync(new URL("../public/og.png", import.meta.url), png)
console.log(`og.png written (${png.length} bytes)`)
