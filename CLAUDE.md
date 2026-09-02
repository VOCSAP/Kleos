# CLAUDE.md -- conventions du fork VOCSAP

Ce depot est un **fork actif de `Ghost-Frame/Kleos`**, avec des merges upstream
reguliers. Tout ce qui suit existe pour une seule raison : garder le cout du
prochain rebase le plus bas possible. Les conventions ci-dessous ne sont pas
cosmetiques, ce sont les regles qui evitent de rouvrir des sites de conflit.

Ce fichier est **tracke**. Ce qui est propre a un poste ou a l'infrastructure
(hotes, IPs, procedure de deploiement, modele LLM cible) vit dans
`CLAUDE.local.md`, gitignored.

**Note sur les references** : sous `docs/dev-notes/`, seul `local-patches.md`
est tracke, le reste est gitignored (`.gitignore` ligne 65). Une reference a un
autre fichier de ce dossier ne resout que sur un poste qui l'a produit.

---

## Principe general : canaux overlay plutot que patch source

Avant d'editer un fichier suivi par upstream pour changer un comportement,
verifier si un canal overlay porte deja le reglage. Un overlay cree **zero delta
upstream**, un patch source en cree un pour toujours.

| Canal | Porte par | Objet |
|---|---|---|
| Prompts LLM | `prompts-overrides/` | personas, templates, rules blocks (Patch 15 + 16) |
| Lexicons i18n | `lexicon-overrides/` | vocabulaire TOML par langue (Patch 38) |
| Gate rules | `gate-rules/` | patterns du gate (Patch 19b) |
| Schema DB VOCSAP | `kleos-lib/src/db/vocsap/` | colonnes, index, tables (Patch 41) |

Les trois premiers sont des submodules, resolus en cascade avec hot reload
(cache TTL 5s) : aucun rebuild.

---

## Surcharge de prompts LLM (Patch 15 + Patch 16)

### Regle d'or

**NE JAMAIS modifier un prompt hardcode dans `kleos-lib/src/**/*.rs` ni les
fichiers embedded sous `kleos-lib/prompts/**/*.txt`** pour ajuster le
comportement local. Ces fichiers sont l'**alignement upstream** (zero delta
cosmetique avec Ghost-Frame/Kleos pour minimiser les rebases futurs). Passer par
le submodule `prompts-overrides`, qui couvre les quatre types de la table plus
bas.

Meme principe pour le raisonnement cote LLM : tout ajustement qui change la
maniere dont le modele *raisonne* sur une tache (formuler une question,
contraindre un format JSON, durcir une regle anti-fabrication) se fait via
overlay, pas via patch source.

### Procedure d'edition d'un prompt

1. Identifier l'ID canonique du prompt (voir `docs/dev-notes/llm-prompts-catalog.md`, 35 ids surchargeables au 2026-05-19).
2. Copier la version embedded depuis `kleos-lib/prompts/<id>.txt` (point de depart fidele a upstream + patches locaux).
3. Editer ou creer le fichier d'override dans `prompts-overrides/<id>.txt`.
4. Commit + push dans le submodule :
   ```bash
   git -C prompts-overrides add <chemin>
   git -C prompts-overrides commit -m "fix(<id>): <pourquoi>"
   git -C prompts-overrides push origin main
   ```
5. Cote serveur : pull le clone du canal (chemins dans `CLAUDE.local.md`). Le cache TTL 5s prend l'edit en compte sans restart de `kleos-server`.
6. Optionnel : bump le pointer submodule dans le main repo si on veut tracer la version.

### Resolution cascade

1. `KLEOS_LLM_PROMPT_REPOSITORY` (explicit) si set
2. `${KLEOS_DATA_DIR}/prompts` si le dossier existe (convention par defaut)
3. Embedded only (upstream behavior)

Cache TTL : 5 secondes (`kleos-lib/src/llm/prompts.rs::TTL_SECS`).

### Types de fichiers et semantique

| Suffixe fichier | Role | Concat cote caller | Exemples ids |
|---|---|---|---|
| `system.txt` | Persona system prompt | tel quel | `broca/ask_plan/system`, `growth/kleos_reflection/system` |
| `user.txt` | Template user avec `{{var}}` | `interpolate(template, vars)` | `broca/ask_plan/user`, `growth/kleos_reflection/user` |
| `system_suffix.txt` | Rules block ajoute au system | `format!("{}\n{}", system.trim_end(), suffix.trim_end())` | `growth/*/system_suffix` |
| `<phase>_user_suffix.txt` | Shot rules appendees au user (phase = `name`/`desc`/`code` pour skills) | `format!("...\n\n{}", suffix.trim())` | `skills/fix_prompt/name_user_suffix` etc. |

Pour les `<phase>_user_suffix.txt`, la phase est embarquee dans le nom de
fichier afin de garder Option C (tout colocalise par `<service>/<purpose>/`,
pas de dossier `rules/` separe).

---

## Lexicons TOML overlay vs modif code (Patch 38)

Cascade `KLEOS_LEXICON_REPOSITORY > ${KLEOS_DATA_DIR}/lexicon/ > embedded
baselines`, hot reload TTL 5s.

**Regle de decision** : si le symptome peut etre exprime comme "il manque ce mot
ou cette forme dans la classe X de la langue Y", c'est **TOML**. Si le symptome
demande "il faut changer COMMENT le regex est construit ou COMMENT le pipeline
appelle la classe", c'est **code**.

- Cas **lexicalement trivial** (mot manquant, accent, conjugue, classe a etendre, metadata valence/arousal) -> **TOML overlay**. Aucun rebuild, hot reload sous 5s. Editer `lexicon-overrides/<lang>.toml`, commit + push, pull cote serveur.
- Cas **bug structural** (priorite regex, capture groups, pipeline d'extraction, mecanisme stem/fold, nouvelle classe necessitant un site Rust) -> **modif code + rebuild**.

**Anti-pattern** : contourner un bug code en listant manuellement toutes les
inflexions dans le TOML. Cela multiplie la maintenance par N et casse
l'intention du stem+wildcard. Preferer le fix code propre une fois le
diagnostic confirme.

---

## Ajouts de schema DB VOCSAP (Patch 41) -- canal overlay, PAS de migration numerotee

**Regle d'or** : tout ajout ou modif de schema DB cote VOCSAP (nouvelle colonne,
index, table) passe par le **canal overlay** `kleos-lib/src/db/vocsap/mod.rs`,
**jamais** par une nouvelle entree dans `MIGRATIONS` / `TENANT_MIGRATIONS`. Ces
arrays sont upstream pur et le restent.

**Statut de la justification** : le canal protegeait a l'origine d'une collision
de slot au merge (renumber -> skip silencieux d'un body upstream -> crash `no
such column`). Cette classe de bug est **morte cote upstream** depuis que le
dispatch des migrations est passe de `MAX(version)` a un applied-set (commit
`79b4efe5`, PR #206, absorbe au merge `7ce95482`). Le canal reste, mais comme
canal anti-conflit, plus comme protection anti-skip.

### Procedure pour ajouter un changement de schema VOCSAP

1. Ajouter une entree dans `VOCSAP_MONOLITH_OVERLAYS` (monolith) ou `VOCSAP_TENANT_OVERLAYS` (tenant), dans `kleos-lib/src/db/vocsap/mod.rs`.
2. Definir `needs` : un guard idempotent qui retourne `true` SSI l'ajout n'est pas encore present (`table_has_column` / `index_exists` / `table_exists`). SQLite n'a pas d'`ADD COLUMN IF NOT EXISTS`, c'est le guard `needs` qui assure l'idempotence.
3. Definir `apply` : le DDL (inline `execute_batch`, ou `include_str!("vocsap/<sujet>.sql")` pour un gros DDL). Cote tenant, `apply` recoit `owner_user_id: Option<i64>` pour un eventuel backfill owner-scoped.
4. Nommer l'overlay par un nom semantique stable, jamais un numero de version.
5. Tester : apply puis no-op (run x2), no-op sur schema deja complet, et le cas fresh.

### Ce qui est OBSOLETE (ne plus faire)

- Ajouter `migration!(N, ...)` / `tenant_migration!(N, ...)` pour un besoin VOCSAP.
- Prendre "le prochain numero libre >= max+1" pour une migration VOCSAP.
- Maintenir un fichier `.manifest` ou un test `*_obey_append_only_manifest` (supprimes par Patch 41).
- Ecrire un self-heal pre-dispatch type Patch 40.

---

## Submodules

| Path | Repo | Branche |
|---|---|---|
| `wiki/` | `VOCSAP/Kleos.wiki.git` (fork du wiki upstream) | `master` |
| `prompts-overrides/` | `VOCSAP/Kleos.prompts` | `main` |
| `lexicon-overrides/` | `VOCSAP/Kleos.lexicon` | `main` |
| `gate-rules/` | `VOCSAP/Kleos.gates-rules` | `main` |

Editer un submodule = `cd <sub> && git add/commit/push`. Bump le pointer cote
main repo via `git add <sub-path> && git commit`. Les trois canaux overlay sont
montes **separement** cote serveur, pas via `git submodule update` (chemins dans
`CLAUDE.local.md`).

---

## Build et tests

- **Windows MSVC ne peut PAS cross-compiler vers `x86_64-unknown-linux-gnu`** (libstd Linux absente). Le build du binaire Linux passe par WSL.
- `cargo test -p kleos-lib` cote Windows exige `--features bundled-sqlite` (pas de `sqlite3.lib` systeme).
- Une suite complete se delegue au sous-agent `test-runner` ; un run cible (`-p <crate> --lib <module>`) reste direct.

---

## Politique d'ecart avec upstream

Chaque ligne modifiee ou ajoutee dans un fichier suivi par upstream est un
**conflit potentiel au prochain rebase**. Avant d'editer un tel fichier :
`git log -p <fichier>` pour voir si la zone est upstream pure ou deja patchee
(etendre un patch local coute moins cher qu'ouvrir un site neuf), puis verifier
`docs/dev-notes/local-patches.md` pour s'inscrire dans un patch numerote
existant. Eviter les **attributs cosmetiques** (`#[cfg(test)]`,
`#[allow(dead_code)]`, suppression d'imports "inutiles") sur du code upstream :
invisibles a la review locale, ils explosent au merge.

**Choisir toujours le niveau le plus bas qui resout le probleme** :

| Niveau | Exemple | Cout de rebase |
|---|---|---|
| Zero delta upstream | Canal overlay, env var, config externe | nul |
| Additif pur | Nouveau module ou fonction publique, appele depuis un site upstream non touche | tres faible |
| Patch chirurgical | 3 a 5 lignes dans une fonction upstream sans rien renommer | faible |
| Refactor local | Renommer un symbole, deplacer une fonction, changer une signature | moyen, releve d'un patch numerote |
| Reecriture | Substituer une implementation complete | lourd, a justifier dans local-patches.md |

Tout patch numerote doit avoir une section dans
`docs/dev-notes/local-patches.md` : symptome sous upstream pur, approche (dont
**pourquoi pas le niveau d'en dessous**), fichiers touches, tests, conditions de
retrait. Et le fait se stocke dans Kleos avec le tag `delta-upstream`, ce qui
construit une carte des divergences interrogeable avant le prochain rebase.

Le complement pratique (grep cross-call-site avant patch, quand deleguer a un
sous-agent pour un audit independant) vit dans
`.claude/rules/kleos-patching-discipline.md`.

---

## Reference des patches locaux

`docs/dev-notes/local-patches.md` est la source de verite : un patch numerote
par section, plus les statuts de merge upstream (patches absorbes, re-accroches,
abandonnes). A consulter **avant tout merge ou rebase depuis Ghost-Frame/Kleos**.
