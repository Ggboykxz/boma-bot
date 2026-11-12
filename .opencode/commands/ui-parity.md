---
description: Compare the rebuilt UI with the original, side by side (writes docs/UI-PARITY.md)
---
Compare the rebuilt UI with the original. Both must run on localhost or a host I control; never point this at production.
Start both apps and record the URLs, builds, accounts and seed data in docs/UI-PARITY.md. If the original is a desktop or mobile app, capture OS-level screenshots and video instead and say so; Playwright only covers web targets.
For each screen in spec/08-ui-ux.md, capture the same state on both sides at 360x800 and 1440x900, light and dark, reduced motion: full page, plus the meaningful components. Store them under docs/ui-parity/<screen>/.
Compare: layout and spacing (allow a documented tolerance), color and type tokens, copy (flag every wording difference), assets, state coverage (loading, empty, partial, error, permission-denied), keyboard order, focus, and the accessibility tree. Use Playwright screenshot assertions for the stable ones and a visual diff for the rest.
Report in docs/UI-PARITY.md: a table per screen (state, original capture, rebuild capture, verdict), a list of intentional differences (design direction, accessibility fixes, better UX) and unintentional ones (defects), and the axe result on both sides. Link every capture; label its origin and license.
Fix the unintentional gaps, one commit per screen, and re-run. Do not edit the original's captures to make a diff pass.
