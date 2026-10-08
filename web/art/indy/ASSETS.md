# INDY ASSET MANIFEST — art department

One character. Many expressions. One deterministic puppet engine.
Runtime never generates images; it only selects registered assets by stable ID.

## Canonical reference
- `web/art/media-generation-indy-scully-portrait-0-432992a8-963b-4146-a11e-6e0592a87c50.webp` — master portrait (Scully direction)
- `web/art/indy-portrait.webp` — downscaled field photo (neutral)

## Registered assets (WIRED — live in the renderer)

| Stable ID | File | Score | Wired as |
|---|---|---|---|
| `indy.expr.neutral.v001` | `web/art/indy-portrait.webp` | 9/10 — canonical | `EXPR_NEUTRAL`; default portrait |
| `indy.expr.skeptical.v001` | `web/art/indy/expr-skeptical.webp` | 9/10 — raised brow, sideways smirk, perfect Scully energy | `EXPR_PORTRAITS.Skeptical`; shown on `setEmotion('Skeptical')` |
| `indy.expr.thinking.v001` | `web/art/indy/expr-thinking.webp` | 9/10 — finger to chin, gaze up-left | `EXPR_PORTRAITS.Curious/Concerned/Surprised`; shown on those emotions |
| `indy.expr.discovery.v001` | `web/art/indy/expr-point.webp` | 8.5/10 — pointing at evidence, confident | `EXPR_PORTRAITS.Delighted`; shown on `setEmotion('Delighted')` |
| `indy.prop.notebook.v001` | `web/art/indy/prop-notebook.webp` | 9/10 — clean vector icon, transparent bg | static prop overlay in avatar panel (`#prop-notebook`) |

Wiring: `setEmotion()` in `web/app.js` → `setExpressionPortrait()` swaps
`#expr-portrait` src + `#expr-caption` text from the same AvatarState emotion
that drives brows/eyes/mouth. The SVG puppet is untouched and fully functional.

## Staged (next round — NOT wired)
- Raw 1MP masters: `web/art/indy/media-generation-*.png` (+ snapshot JSONs)
- Planned: `indy.head.*` full-head SVG layer variants, `indy.eyes.*` layer set,
  `indy.mouths.*` (A–X already parametric in renderer), more gestures
  (nod, lean-in, shrug), props (evidence card, magnifier)

## Scoring rubric used
character_consistency / expression_clarity / silhouette /
small_screen_readability / svg_traceability — keep bar ≥ 8/10.

## 95/5 law
Art department generates. Rust `AvatarState` + `setEmotion` own selection,
timing, and state. No image model in the runtime path, ever.
