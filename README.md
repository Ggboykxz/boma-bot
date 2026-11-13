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
Lance `./tools/sandbox.sh` (optionnel : Rust, Playwright et les outils qualité préinstallés) ou `opencode` sur ta machine après `./tools/install-rust-tools.sh`. Le provider **Coarena** (API Model gratuite) est déjà branché : pas de `/connect`, `/models` sert à vérifier les 18 modèles disponibles, défaut `coarena/claude-opus-5-5`.
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

## Démarrage rapide (état M0)
M0 est écrit, compilé, linté et testé : `make check` et `make ui-test` passent (résultats et commandes exactes dans docs/STATE.md). Le squelette a été éprouvé contre un vrai PostgreSQL 17 et un vrai Chromium.

Prérequis : `rustup` (le fichier `rust-toolchain.toml` épingle 1.99.1 ; la MSRV est 1.94, imposée par sqlx 0.9), PostgreSQL 17, Node 22 pour l'UI seulement, et les outils qualité `cargo-nextest`, `cargo-llvm-cov`, `cargo-deny`, `cargo-machete` (la CI les installe toute seule).

1. Verrous déjà générés et commités : `Cargo.lock` et `apps/web/package-lock.json` existent. Si tu les régénères, `make deps-lock` puis `git add`.
2. Base : `docker compose up -d db`, ou un PostgreSQL local avec `BOMA_DATABASE_URL` pointant dessus.
3. API en local :
   `cargo run --bin boma -- migrate`, `cargo run --bin boma -- serve`,
   puis `curl 127.0.0.1:3000/health` et `curl 127.0.0.1:3000/ready`.
4. UI en local (serveur de dev sur 127.0.0.1:3100) :
   `npm --prefix apps/web ci` puis `npm --prefix apps/web run dev`.
5. Portes qualité : `make check` (fmt, clippy, types, tests, couverture, deny, machete, trace, secrets) et `make ui-test` (Playwright, goldens UI, axe).
6. Docker : `docker compose up --build` ; le service `migrate` applique les migrations avant que `app` ne serve.
7. Ordinateur avec navigateur (observation et action, REQ-028) : `SANDBOX_PROVIDER=docker`, puis `BOMA_COMPUTER_IMAGE=zenika/alpine-chrome:latest`, `BOMA_COMPUTER_CMD="--headless --no-sandbox --remote-debugging-port=9222 --remote-debugging-address=0.0.0.0 about:blank"` et `BOMA_COMPUTER_CDP_PORT=9222`. Sans ces trois variables, l'ordinateur reste l'image alpine (shell et fichiers) : les cas d'usage navigateur répondent `Unavailable`, jamais de faux pixels. Ces cas d'usage ne sont pas encore exposés au fil RPC ni appelés par l'agent (voir docs/STATE.md, M4).

Le contrat de la spec reste `/health` et `/ready` ; `/healthz` et `/readyz` existent en plus comme alias exigés par AGENTS.md. L'écart est assumé et documenté dans docs/STATE.md (DEV-01).
La licence du projet est MIT (choix de l'agent pour débloquer `cargo deny`) : change-la dans `Cargo.toml` + `LICENSE` et relance `cargo deny check` si tu veux autre chose.

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
