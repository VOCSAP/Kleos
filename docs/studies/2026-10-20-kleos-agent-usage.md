# Usage réel de Kleos par les agents et décisions du 2026-10-20

## 1. Méthode et portée

**MESURÉ, rapport de kleos-7 relu, sans relancer le scan :**
`~/.agent-forge/scratch/kleos7_usage.json` couvre **2 023 transcripts**, du
2026-09-20T20:27:05 au 2026-10-20T20:25:55, sur une fenêtre de 30 jours.
`jq '{period, files, cli_total: ([.cli[][3]]|add), mcp_total: ([.mcp[][3]]|add)}' ~/.agent-forge/scratch/kleos7_usage.json`
expose les bornes et les totaux : **4 698 occurrences CLI, 531 appels MCP**.

**DÉDUIT de `kleos7_usage.py:40-80,94-102,122-140` :** le scan parcourt les JSONL
principaux et sous-agents, extrait les commandes des outils et analyse leur syntaxe ;
il ne compte pas une simple mention de Kleos dans la prose. Un appel observé n'est pas
une réussite certifiée. Les clés parasites du parseur restent séparées ci-dessous.
Les journaux de hooks forment une mesure distincte ; leurs attachments tous hooks
ne sont pas un compteur d'appels réseau Kleos.

**Convention :** MESURÉ désigne soit une extraction exécutée ici, soit une mesure
attribuée explicitement à kleos-7/kleos-5, non réexécutée. Les décisions citées sont
consignées dans les mémoires lues par `memory_get(id)` et les cartes ; elles ne prouvent
pas une implémentation. DÉDUIT désigne une interprétation ; SUPPOSÉ reste à vérifier.

## 2. Appels choisis par les agents

**MESURÉ, agrégation du rapport :**
`jq '.cli | group_by(.[2]) | map({command: .[0][2], count: (map(.[3])|add)}) | sort_by(-.count)' ~/.agent-forge/scratch/kleos7_usage.json`.

| Sous-commandes CLI | Occurrences sur 30 jours |
|---|---|
| `store` ; `context` ; `search` | 2 959 ; 860 ; 267 |
| `recall` ; `list` ; `health` ; `delete` | 102 ; 73 ; 22 ; 17 |
| `handoff dump` ; `latest` ; `list` ; `get` | 150 ; 96 ; 18 ; 12 |
| `handoff restore` ; `search` ; `atoms` ; sans sous-commande | 9 ; 6 ; 3 ; 2 |
| `handoff gc` ; `show` | 1 chacun |
| `space resolve` ; `list` ; `ensure` ; sans sous-commande | 15 ; 10 ; 3 ; 1 |
| `admin vector-health` ; `vector-chunk-sync` ; sans sous-commande | 3 ; 1 ; 1 |
| `skill list` ; `stats` ; sans sous-commande | 2 ; 1 ; 1 |
| `forge` ; `forge get-spec` ; `ingest` ; `recall-due` | 1 chacun |
| Non classées ou clés parasites | `(none)` 42 ; `handoff 2>` 13 ; `admin 2>` 1 ; `space 2>` 1 ; clé `space resolve` mal découpée 1 |

**MESURÉ, même agrégation sur `.mcp` :**

| Outil MCP, préfixe `mcp__kleos__` | Appels |
|---|---|
| `memory_get` | 392 |
| `memory_search` | 99 |
| `handoffs_latest` ; `handoffs_search` | 16 ; 10 |
| `memory_list` | 4 |
| `toolbox_index` ; `toolbox_find` ; `toolbox_get` | 7 ; 2 ; 1 |

**DÉDUIT des comptes :** `context` + `search` + `memory_search` = 1 226 recherches
choisies, soit environ 1 230, sans confondre lectures par id et recherche. **MESURÉ par
kleos-7, carte `7d3836a5` :** une recherche choisie dans 414 des 1 050 sessions de son
périmètre de comparaison, soit 39 % ; ce dénominateur n'est pas les 2 023 fichiers.

### Routes effectivement utilisées

**DÉDUIT de l'audit code de kleos-7, Kleos #30932/#30933/#30939, non refait ici :**

| Entrée | Chemin et conséquence |
|---|---|
| CLI `search`, MCP `memory_search` | POST `/search`, `hybrid_search_reranked` |
| CLI `context` | POST `/recall` (`kleos-cli/src/main.rs:1509`), tiers sémantique sans reranker et tiers static/important/recent |
| Hook `UserPromptSubmit` | D'abord cache local POST `/v1/retrieve`, mode local, `top_k=3` |
| Repli sidecar `/recall` | Serveur POST `/search`, cosinus >= 0,55, catégories `general` et `state` exclues |
| Dernier repli du hook | CLI `context --limit 8`, donc POST `/recall` |

**Absence rapportée, non re-mesurée :** #30932 ne trouve aucun appelant de POST
`/context` dans CLI, sidecar, kleos-sh, hooks ou registre MCP. Ce périmètre ne constitue
pas une preuve d'absence de tout client externe. **DÉDUIT :** le canal dominant étant
local, un mécanisme serveur de supersession/redirection ne le rejoint que par la
réplique kleos-cache ; le serveur seul ne suffit pas. Voir l'[étude nightly](2026-10-20-kleos-nightly-pipeline.md), sections 9 et 13.

## 3. Récupération automatique et défauts du hook par prompt

**MESURÉ par kleos-7 dans les logs sur 30 jours**, communiqué le 2026-10-20 ; commandes
`python -I ~/.agent-forge/scratch/kleos7_logs2.py`, `kleos7_logs.py`, `kleos7_sc.py`
(même préfixe Python et répertoire pour les deux dernières), non relancées ici.

| Hook / mesure | Volume et preuve décisive rapportée |
|---|---|
| `user-prompt-lean`, déclenchements | 12 968 ; 272 prompts courts exclus, donc 12 696 tentatives de récupération, environ 12 700 |
| Cache local | 11 191 tentatives : 10 054 HIT, 1 124 TIMEOUT, 12 DOWN, 1 EMPTY ; HIT médian 159 ms, budget 250 ms |
| Prompts hors HIT cache | 2 642 ; 2 479 avec résultat de repli. Répartition sidecar/CLI du mois non mesurable dans le log |
| `session-start-kleos.sh` | 1 937 `SessionStart fired`, 1 909 rendus `/prompt/generate` ; POST `/activity`, CLI `list --limit 5` vers GET `/list` |
| `sidecar-session-start.sh` | 1 709 lignes : 1 164 codes 201, 540 `409000`, 5 `401000` |
| `session-end-kleos.sh` | 854 `task.completed` ; sidecar `/end` et serveur POST `/activity` |

**Limite mesurée :** 1 505 prompts non courts sans tentative cache dans ce relevé ;
**SUPPOSÉ par kleos-7 :** jeton cache absent, cause non vérifiée. Les ~12 700 sont donc
un volume de récupérations déclenchées, pas 12 700 injections réussies garanties.

**MESURÉ par kleos-7, #30936 et carte `4edeb1b3` :**

| Piège | Mesure et portée |
|---|---|
| Requête-enveloppe de peer | 8 664 sur environ 12 550, soit 69 %, avant nettoyage XML |
| Mémoire déjà injectée dans le transcript | 51,3 % des items, 56,5 % pour le top-1 |
| Texte tronqué | 180 caractères par item, pour une médiane mémoire de 633 caractères et de 732 sur les `memory_get` |
| Réouverture par l'agent | 36 % des `memory_get` rouvrent une mémoire déjà injectée |
| Catégorie `state` dans le cache | 6 377 / 30 126 items, soit 21 % ; retenir la correction de la carte, pas sa première note erronée |

**DÉDUIT de l'audit rapporté :** cache sans seuil ni exclusion, contrairement au
sidecar (`kleos-sidecar/src/routes.rs:728-760`). Le filtre du sidecar n'est pas une
garantie sur le premier chemin cache. **SUPPOSÉ :** un seuil et des exclusions alignés
amélioreraient la pertinence ; mesurer avant d'en faire une garantie.

## 4. Leviers et arbitrages de l'opérateur

**Décisions consignées, #30931/#30934 :** une commande peu utilisée est un **levier**,
pas la preuve de son inutilité. Mesurer les chemins et orienter les usages, sans
forcer un développement à partir d'un simple faible compteur.

| Levier | Décision du 2026-10-20 et suivi |
|---|---|
| Dernier handoff au SessionStart | **REFUSÉ** : serait injecté dans chaque peer d'une équipe |
| Ajouter des consignes Kleos aux CLAUDE.md de projet | **REFUSÉ** : le global suffit |
| Injection `UserPromptSubmit` agrandie et mieux sélectionnée | **ACCEPTÉ**, cinq réglages, carte `4edeb1b3` ; prochaine session, rien lancé aujourd'hui |
| Instructions du serveur MCP | **ACCEPTÉ**, carte `b656cd5a`, corriger notamment la référence à `context_build` absente du registre selon l'audit |
| Rappel avant premier Edit/Write | Principe non bloquant accepté, **message EN ATTENTE**, carte `7d3836a5`, `ready-for-human` |

Plan d'exécution séparé : dépôt claude-config, `plans/user-prompt-lean-injection.md`.
Le rappel n'appartient pas au lot des cinq réglages. Message proposé, **non validé** :

```text
[kleos] 3 mémoires mentionnent <fichier> (1 issue, 1 decision, 1 discovery). La plus pertinente : #<id> (<catégorie>, <date>) « <extrait> ». Avant d'éditer : kleos-cli recall <id>
```

**Contrat proposé, carte `7d3836a5` :** au premier Edit/Write d'un fichier dans la
session, requête à kleos-cache local ; rien si zéro résultat ou délai supérieur à 1 s ;
jamais bloquant, rien au deuxième Edit du même fichier. **DÉDUIT du contrat :** l'état
doit être isolé par session, pas partagé entre peers. La carte signale que la date
manque dans `LocalResult` (`kleos-cache/src/http.rs:57`) ; variante sans date à arbitrer,
non à présenter comme déjà disponible.

## 5. Porte, sidecar et transcripts : décisions distinctes

| Sujet | État documenté et décision |
|---|---|
| Porte kleos-sh | **MESURÉ par kleos-7, #30937 :** 120 313 annulations par timeout de 10 s du 2026-05-21 au 2026-09-29, dont **112 238 outils exécutés sans porte**. Fenêtre historique fermée par Patch 55/56 (`03603a9d`, `KLEOS_SH_APPROVAL_TIMEOUT_SECS=2`). |
| Risque résiduel | **DÉDUIT dans #30937** de `kleos-sh/src/main.rs:263-268`, `gate.rs:117-123` et du câblage : défaut fail-open, pas de `onFailure` bloquant ; erreur ou dépassement peut laisser passer. **Décision opérateur rapportée par le lead : comportement CONSERVÉ**, pistes could `77549485` et `062a5edf`, pas de durcissement dans ce lot. |
| Sidecar | **État rapporté #30939 :** actif ; ne lit plus les transcripts (watcher supprimé upstream `e5526e80`), n'écrit plus de mémoire ; registre de sessions, repli `/recall`, télémétrie. Audit sans appelant `/observe`, aucune mémoire source sidecar vivante, dernière le 2026-05-29. Ce n'est pas un pipeline de transcripts. |
| Codes sidecar | **MESURÉ par kleos-7 :** 540 `409000` bénins et 5 `401000` dans `sidecar-session-start.log` ; **DÉDUIT** de `sidecar-session-start.sh`, suffixe `000` produit par `curl -sf` puis `echo 000`, non un code HTTP à six chiffres. Les cinq 401 datent des 12 et 18 octobre ; **SUPPOSÉ :** jeton absent/périmé, non vérifié. |
| kleos-ingest | **État confirmé par l'opérateur dans #21290 : jamais déployé.** L'implémentation actuelle extrait lexicalement, envoie POST `/memories` et résume via Ollama local ; elle ne réalise pas la cible serveur ci-dessous. |

**MESURÉ par kleos-7 depuis le redémarrage du sidecar le 2026-10-20 à 06:59 :**
98 replis hors cache pour 98 au compteur `sidecar_code_context_total`, donc 100 % des
replis pris par le sidecar sur cette fenêtre seulement (`kleos7_sc2.py` et métriques
locales). Ne pas extrapoler cette répartition aux 30 jours.

**Décision opérateur #30938, complément à D6 et 4.3 de l'étude nightly :** la sécurité
des transcripts n'est pas un frein ; cible de stockage côté serveur, éventuellement
réduit, avec traitement en tâche de fond pendant l'inactivité. Première marche :
**must `3f45e0f8`, kleos-ingest sous Windows** ; suite cible sur **`f18dc3c4`**.
Cette décision ne certifie ni déploiement, ni transfert serveur, ni qualité de résumé.

## 6. Suites non confondues avec une autorisation d'implémenter

**Statut au 2026-10-20, sources cartes et brief du lead :** préparer le lot hooks pour
une prochaine session, attendre la validation du message avant Edit/Write et garder
le patch cache distinct. Sur la supersession, M-N16c manque les seuils du consultant ;
M-N16d mesure encore nature des échecs, cache et requête courte avant retour au consultant.
Aucune option de redirection n'est figée par cet état des lieux.
