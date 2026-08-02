# Design — Skill Shelf (frontend)

A locked design system for this app. Every page redesign reads this file before
emitting code. Do not regenerate per page — extend or amend this file when the
system needs to grow. (Lives at `app/design.md` because the repo root `DESIGN.md`
is the API/architecture doc on a case-insensitive filesystem.)

## Genre
modern-minimal (dev-tool register)

## Macrostructure family
All pages are **app pages**: Workbench shape — sticky hairline nav, single
centered column (`max-w-3xl`), toolbar-above-content, cards as bordered panels.
No marketing pages, no content pages. **App pages MUST NOT use enrichment** —
function carries the page.

## Theme — Cobalt
Cool engineered paper, one electric cobalt signal, hairlines do the work.
Canonical values live in `src/index.css` as shadcn variables:

- paper (light): `oklch(98.5% 0.004 250)` · ink: `oklch(24% 0.02 258)`
- paper (dark, graphite): `oklch(17% 0.014 260)` · ink: `oklch(95.5% 0.006 250)`
- accent (signal): `oklch(55% 0.19 258)` light / `oklch(68% 0.17 256)` dark
- rule: `oklch(90.5% 0.008 253)` light / `oklch(100% 0.005 250 / 12%)` dark
- Accent budget ≤ 5% of any viewport: primary button, active nav pill, focus
  ring, dirty-state dot, link hover. Everything else is ink on paper.
- Depth = hairline borders. **No drop shadows** on cards or panels.

## Typography
- Display: Space Grotesk Variable 550–600, roman only, tracking −0.014 to −0.022em
  (`font-heading`; h1–h3 mapped globally in index.css). CJK falls back to system sans.
- Body: Geist Variable 400–500 (`font-sans`).
- Mono: JetBrains Mono Variable (`font-mono`) — ALL config keys, values, tokens,
  code, kbd hints, and meta labels.
- Machine-readout labels: class `.label-mono` (mono · 11px · uppercase ·
  +0.06em) for section eyebrows/meta. Use sparingly, never as heading substitute.

## Spacing
Tailwind 4-pt scale. Page rhythm: `py-8` main, `gap-4` between cards,
`p-3`–`p-6` inside panels. No raw pixel values in components.

## Radii
Ruler-drawn: `--radius: 0.625rem` → buttons/inputs ≈6px (`rounded-md`),
cards 10px (`rounded-lg`). No full-pill surfaces except the existing namespace
chips and nav pills (grandfathered interaction affordances).

## Motion
motion-cut. Keep only: the global 140ms press feedback, shadcn dialog/popover
transitions, sonner defaults. No reveals, no scroll animation, no new keyframes.
`prefers-reduced-motion` collapse already global in index.css.

## Microinteractions stance
- Silent success: toast, no celebration.
- Errors inline near the control; toasts only for async background failures.
- Focus: `:focus-visible` ring, instant, cobalt.

## i18n
- zh/en via `src/lib/i18n.tsx` (`useT()` → `t("domain.key")`, `{x}` placeholders).
- Dictionaries per domain in `src/lib/i18n/*.ts`; every UI string goes through
  `t()`. User data (skill names, config keys/values, feedback text) is NEVER
  translated. Language toggle lives in the header; persisted to localStorage.

## CTA voice
- Primary: solid cobalt, verb-first label ("Import 3 keys to draft", not "OK").
- Secondary: `variant="outline"` hairline. Destructive: shadcn destructive.
- One primary action per view region.

## What pages MUST share
- The header (wordmark + nav pills + language toggle + account menu).
- Cobalt tokens, Space Grotesk headings, mono-for-data discipline.
- Card-as-bordered-panel surface; no shadows.
- CTA voice and the 8-state discipline on interactive controls.

## What pages MAY differ on
- Internal panel layout (list vs form vs editor) as the function requires.
- Density: ConfigCenter runs denser than SkillList; both stay on the 4-pt scale.
