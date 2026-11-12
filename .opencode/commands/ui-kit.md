---
description: Install BoardUI through its MCP server, wire the design rules and the skill, map every UI REQ (usage /ui-kit)
---
Preconditions: spec/08-ui-ux.md exists; otherwise STOP and tell me to run /ui-spec in the analysis workspace, review it, then tools/export-spec.sh. Read spec/EXPORT.md, docs/PLAN.md and docs/UI.md first.
Read the design context: boardui_get_started, boardui_license_status, boardui_list_components, boardui_get_theme, boardui_get_skill. Summarize the foundations in ten lines (tokens, type scale, spacing, dark mode, cx()).
Pro license: if I have one, I paste the key in this conversation. Call boardui_activate_license once. Never write the key to a file, a commit message, a log, a note, docs/ or spec/. If you ever see it on disk, stop and tell me. One seat per machine: check boardui_license_status before activating, and never deactivate without my say-so.
boardui_init_boardui (Tailwind v4, runtime dependencies, theme and global styles, utils, boardui.json). Review each write. In a monorepo, put boardui.json next to the destination package's package.json and pass projectDir.
boardui_install_rules, then review the diff of AGENTS.md: the fenced BoardUI section must only add design conventions. If it changes anything else, stop and tell me. Never hand-edit that fenced section; re-run install_rules to refresh it.
boardui_install_skill: it writes into .claude/skills/, which opencode discovers as a project skill. Tell me to restart opencode at the end so the skill and the always-on rules load.
Install components screen by screen with boardui_install_components, smallest set first, never a whole template before the first screen works. You own the copied source: keep local changes minimal, and diff before re-running an install because it overwrites.
Write docs/UI.md: framework and rendering decision (or the ADR that already covers it), the semantic token map (role -> token -> where used), the component table (REQ id -> screen -> components -> states), copy conventions, i18n/RTL notes, and every local deviation from the vendored source.
Add an ADR only if /plan did not already decide the design system: alternatives (hand-rolled Tailwind, shadcn/Radix, MUI...), scoring, decision, consequences, license note.
Record licenses in docs/DEPENDENCIES.md. Check BoardUI's terms before the repository ever goes public: if Pro source may not be redistributed, keep it out of the public repo and install it at build time.
Build the app shell and one screen end to end, then run /ui-verify on that screen before widening. Commit. Keep the spec portable: do not let the spec depend on BoardUI. A design rule never overrides a spec requirement (the spec wins, a conflict is a decision for me), and there is only one design system.
