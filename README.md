# rebuild-workspace : reconstruire le logiciel analysé, en Rust et en mieux
Uniquement pour un logiciel que tu possèdes ou que tu es autorisé à reproduire ou remplacer. La spec est le contrat ; l'original reste une référence disponible. Ce n'est pas un avis juridique : vérifie licences, CLUF et droits sur les assets avant de publier quoi que ce soit.

## Le produit
Le logiciel reconstruit s'appelle **Boma Bot**. C'est la cible de `re-workspace` (Rakazo) réécrite : même comportement attendu, nom, marque, scope de paquets (`@boma/*`), variables d'environnement (`BOMA_*`) et identifiants d'applis (`com.boma.app`) à toi. Aucun asset, logo, domaine ou texte de la marque d'origine n'est repris sans vérification de licence.

## Le principe
`re-workspace` produit une spec de comportement (notes/spec/) et des captures de référence (notes/ui/). Tu relis, puis `rebuild-workspace` reconstruit à partir de cette spec. L'agent peut consulter l'original et l'exécuter à côté du nouveau pour comparer ; il note ce qu'il a consulté dans docs/STATE.md, et préfère réimplémenter plutôt que copier. Backend en Rust ; pour tout le reste (base de données, UI, mobile, CLI, déploiement...) l'agent choisit lui-même et justifie chaque choix dans un ADR (docs/adr/). Tu n'as qu'à poser ton veto.

## Pipeline
Dans `re-workspace` : finis l'analyse (étapes 2 à 5), lance `/ui-probe` si l'interface compte, puis `/spec` et `/ui-spec`.
Relis `re-workspace/notes/spec/`, surtout 05-known-defects.md, 06-decisions-needed.md, 08-ui-ux.md, golden/ et les captures de notes/ui/.
Ici : `./tools/export-spec.sh ../re-workspace` (ajoute `--review` pour relire avant de copier). Aucune porte bloquante : les fichiers manquants, le scan de secrets et les exemples golden sont des avertissements. spec/EXPORT.md garde la provenance (horodatage UTC, sha256, compteurs) ; l'ancien import part dans `spec.bak-<horodatage>`.
Une seule fois : `git config user.name "..."` et `git config user.email "..."` dans ce dossier.
Lance `./tools/sandbox.sh` (optionnel : Rust, Playwright et les outils qualité préinstallés) ou `opencode` sur ta machine après `./tools/install-rust-tools.sh`. Puis `/connect` et `/models`.
`/plan` : l'agent décide la stack, écrit les ADR et le plan, puis s'arrête. Réponds aux décisions, pose tes vetos, écris « go ».
`/scaffold`, puis `/implement M1`, `/implement M2`... Après chaque jalon : `/parity`. Ensuite `/improve`, `/audit`, corrige, puis `/release 0.1.0`.
Si la spec a une UI : `/ui-kit` une fois, `/ui-verify` par écran, et `/ui-parity` pour comparer avec l'original.
Nouvelle session : `/resume` relit docs/STATE.md et continue.

## Commandes
| Commande | Rôle |
| --- | --- |
| /plan | Choix de stack (ADR), architecture, jalons, budget perf, carte des améliorations ; s'arrête pour ton « go » |
| /adr <sujet> | Écrit un ADR pour une décision ultérieure (ex. file de jobs) |
| /scaffold | Workspace Cargo, lints, deny.toml, CI, harnais de tests (golden API et UI, contrat OpenAPI, Playwright + axe) |
| /implement <M1 ou REQ-001 ...> | Test d'abord, puis code, un commit par exigence |
| /parity | Parité avec la spec et l'original, exemples golden, contrat OpenAPI, UI et accessibilité (docs/PARITY.md, docs/UI-PARITY.md) |
| /improve | Passe d'amélioration mesurée avant/après (docs/IMPROVEMENTS.md) |
| /audit | Revue sceptique indépendante (docs/AUDIT.md), ne corrige rien |
| /release <version> | Release si les portes passent : CHANGELOG, migration, build reproductible, checksums |
| /resume | Reprend depuis docs/STATE.md |
| /ui-kit | Installe BoardUI, le thème, les règles et le skill, mappe les REQ UI |
| /ui-verify | Vérifie l'UI reconstruite avec Playwright MCP + tests Playwright + axe |
| /ui-parity | Compare l'UI reconstruite et l'originale écran par écran (docs/UI-PARITY.md) |

## UI/UX : BoardUI, Playwright, accessibilité
BoardUI fournit les composants, les tokens sémantiques, les règles de design et un skill. `boardui_get_started` confirme le framework (Tailwind v4 / shadcn) avant de s'engager. La clé Pro se colle dans la conversation, jamais dans un fichier ni un commit.
Deux directions valides : parité visuelle fidèle (palette, typo, espacements, wording, assets — si la licence le permet) ou design original qui répond au même comportement. Le choix est un ADR ; les captures de `../re-workspace/notes/ui/` servent de références.
Playwright pilote les deux applis, sur localhost ou une machine que tu contrôles : tests e2e, axe, parcours clavier, screenshots, diff visuel. Jamais la production.
Accessibilité = porte de qualité. Zéro critique ou sérieux dans axe, parcours clavier complet, focus visible, 360 px, clair/sombre, prefers-reduced-motion. Ça se mesure dans /parity, /improve, /audit et /ui-parity.
Docker. Chromium tourne mal en root dans un conteneur : si le navigateur refuse de démarrer, ajoute `-e PLAYWRIGHT_MCP_NO_SANDBOX=1` à sandbox.sh. Si la version de Playwright du projet diffère de celle de l'image, relance `npx playwright install --with-deps chromium` dans le projet.
Skill Playwright (optionnel, non câblé). Microsoft recommande `@playwright/cli` + skills pour les agents de code (plus économe en tokens que le MCP). Si tu veux l'essayer : `npm install -g @playwright/cli@latest`, `playwright-cli install --skills`, relis le skill, puis charge-le. Ne fais pas tourner les deux sur la même app en même temps.

## Ce que « mieux » veut dire
Voir docs/QUALITY_BAR.md : parité complète des MUST, correction de tous les défauts connus, Rust strict (zéro unsafe, zéro unwrap hors tests, clippy et cargo-deny propres), sécurité, performance mesurée, couverture, DX, observabilité, accessibilité WCAG 2.2 AA. Tout changement visible de l'extérieur (API, formats) est une DEVIATION que tu approuves. Une porte de qualité peut être améliorée, jamais desserrée en silence.

## Garanties et limites
Deux workspaces séparés pour la clarté et la provenance, mais aucune barrière technique : l'agent peut lire la cible, l'exécuter et comparer les deux applis. La spec reste le contrat.
Une seule règle permissions (* -> allow), aucune policies : tout est autorisé (écriture, shell, réseau, MCP, skills, Code Mode, push). C'est volontaire ; la contrepartie est ta vigilance (clé API dédiée, VM pour une cible douteuse, licences des assets).
`trace_check.py` exige l'id dans le nom du test : un commentaire ne compte pas. Il signale aussi les ORPHAN (id testé mais absent de la spec).
`cargo build` et `cargo test` exécutent les build scripts et macros procédurales des dépendances : du code tiers qui tourne. Fais-le dans le conteneur ou une VM si tu n'as pas confiance.
`/ui-parity` compare des captures : les polices, l'antialiasing et les animations peuvent créer des écarts ; fixe une tolérance et documente-la.
Les GUESS non tranchés sont des risques : relis 06-decisions-needed.md avant de lancer /plan.
