# Glass DESIGN.md

This file is the product's public-site brand contract. Keep it short and exact:
agents and humans should be able to update `site/` from this file without
inventing a second design system.

## Brand Voice

- Plain-spoken, concrete, and operator-facing.
- Lead with the user outcome, then the proof.
- Avoid marketing fog, mascot language, and decorative claims.

## Pitch One-Liner

`Watch your fleet work.`

## Locked Homepage

- Lock: operator lock-in 2026-07-07, `misty-step-936`.
- Layout: Split.
- Homepage H1: `Watch your fleet work.`
- Hero image: `site/assets/hero.jpg`, copied from the production
  `glass-hero.jpg` asset generated with `gpt-image-1` in the Misty Step fresco
  language.
- Hero image opacity: `0.85`.
- Homepage structure: one viewport only — header, left-aligned hero H1,
  `Get started` CTA, and footer. Existing feature rows and screenshots live on
  `features.html`.

## Lucide Mark

- Icon: `mirror-rectangular`
- Reason: reused from the live stage itself (the header mark shipped in
  glass-907) because it is already the product mark operators see when they
  open the running Glass viewer.
- Rule: the mark is an inline Lucide SVG inside `.ae-app-mark`. No bespoke
  marks, logo images, emoji marks, or colored wordmarks.

## Palette Hooks

Glass keeps its own live-viewer accent (a teal-green) rather than a named
Aesthetic preset, so the marketing site reads as the same product as the
running stage:

```css
:root {
  --ae-accent: #006b5b;
  --ae-accent-dark: #66c7b7;
}
```

## Screenshot Inventory

| File                                        | Surface                       | State                                              | Caption                                                     |
| -------------------------------------------- | ------------------------------ | --------------------------------------------------- | -------------------------------------------------------------- |
| `site/assets/screenshots/01-overview.png`   | Fleet wall + aggregate stream | Seeded instance, 3 concurrent lanes across 3 repos | The factory floor: every live lane, newest surface first.    |
| `site/assets/screenshots/02-narrow.png`     | Fleet wall + stream, narrow   | Same seeded instance at mobile width               | The same stage at 390px — no separate mobile app.             |
| `site/assets/screenshots/03-drilldown.png` | Session drill-down            | Fleet card clicked through to one scoped session   | Click a lane to see only its work, with a way back to all.   |

## Footer Links

- Misty Step: `https://mistystep.io`
- GitHub: `https://github.com/misty-step/glass`

Footer contract: the mode toggle sits on the left. The right side reads
`a Misty Step project`, with `Misty Step` linked to `https://mistystep.io`,
followed by the GitHub glyph linked to the public repo. No bare URL text, no
email, no copyright line, and no Weave links.

## Release Notes Rule

`site/changelog.html` is user-facing. Write entries as product outcomes, not
commit logs. Each entry needs a date, a version or release label, and one or two
plain-language bullets.

Glass has no tagged releases yet (no `git tag` history), so the Landmark
user-facing release-notes export (landmark-902) has nothing to diff against.
`site/changelog.html` ships an honest hand-written stub covering the shipped
milestones (MVP, verified-live deploy, branding + identity, fleet wall) instead
of a Landmark export. Switch to the Landmark export once Glass cuts its first
tagged release.
