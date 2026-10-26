# Étude d'architecture : cible de l'ingestion des transcripts Claude Code

Statut : **conception seulement, aucun code livré**. **Révision 7 du 2026-10-24 :
Kleos devient consommateur d'hypomnema.** Décision opérateur (Kleos #33942, #33968,
#33978) : un outil autonome, **hypomnema** (dépôt `VOCSAP/hypomnema`, Rust, conception
`docs/superpowers/specs/2026-10-24-hypomnema-design.md`, commit `009d6b6`), devient la
seule source d'extraction, de rédaction, de stockage et de map des transcripts. Le démon
poste et le relais de la révision 6 ne seront pas construits dans ce dépôt : leur
contenu est repris, amendé, par la conception hypomnema. **Ce qui reste à Kleos est
décrit en section 11** (consommateur), qui fait foi ; les sections 1 à 6 et 8 portent
chacune en tête la mention de ce qui est remplacé et de ce qui reste valide. Rien n'est
effacé : l'historique des révisions 1 à 6 reste lisible tel quel.
Historique : la
révision 2 répondait à un challenge au verdict « tombe » et à trois mesures (T1 à T3,
section 0) ; la révision 3 intègre le verdict du consultant sur l'embranchement de la
section 1.3 ; la révision 4 retire toute dépendance à la porte de revue ; la révision 5
pose la dédup de fond, le space par session et les juges cloud ; la révision 6 répond à
une attaque de challenger sur la rédaction des secrets et sur les injections du harnais,
validée par deux mesures sur le corpus du poste (Kleos #32359, #32381), et à la décision
opérateur du 2026-10-22 de calculer la vue A+ sur le poste. Décisions
opérateur du 2026-10-21 : R et règle de bump validées (Q5) ; **la porte de revue
(`KLEOS_REVIEW_GATE_*`, statut `pending`, inbox) ne sera jamais activée** (21:57, Kleos
#31722), donc la variante (a') et la carte 6ee2297f sont abandonnées. La protection
contre les faits faux passe par le relais et par le juge (section 4.4). Cette étude détaille la section 4.3 de
l'[étude nightly](2026-10-20-kleos-nightly-pipeline.md) (« Transcripts en attente :
extension distincte ») et en **amende un point** : la zone `transcript_staging` ne vit
pas dans Kleos (overlay Patch 41) mais dans un relais à côté du serveur, dépôt séparé
dépendant de kleos-lib par un rev épinglé (tranché par le consultant, section 1.3). Le reste
de l'étude nightly (juge de voisinage, état de traitement, principe « un faux retrait
est pire qu'un retrait manqué ») s'applique tel quel aux mémoires issues des
transcripts.

Cartes liées : `f18dc3c4` (mise en service), `20d18577` (reprise après arrêt brutal),
`3f45e0f8` (portage Windows, livré), `19671bc8` (dry-run du watcher), `6ee2297f`
(dédup et porte d'attente, archivée en wontfix le 2026-10-21).

### Historique des révisions

| Révision | Date | Objet |
|---|---|---|
| 1 | 2026-10-21 | Cible initiale : démon poste, relais, publication à 45 min |
| 2 | 2026-10-21 | Challenge « tombe » et mesures T1 à T3 ; B stocké au relais, A+ présenté au modèle ; publication à la stabilisation |
| 3 | 2026-10-21 | Verdict du consultant : R, relais séparé sur kleos-lib épinglé |
| 4 | 2026-10-21 | Porte de revue jamais activée ; protection par le juge amont |
| 5 | 2026-10-21 | Dédup de fond, space par session, juge amont Haiku, bande grise Sonnet, gel ELv2 |
| 6 | 2026-10-22 | A+ calculé au démon, plus de B au relais ; détecteur de secrets avant publication ; injections repérées par la structure ; « extraire là où c'est écrit » généralisé ; renvoi motivé (`rewrite`, `view_upgrade`) ; points secondaires du challenger (erreurs Windows en français, ligne illisible, `/rewind`, enregistrement énorme) |
| 7 | 2026-10-24 | Décisions #33942, #33968, #33978 ; conception hypomnema (commit `009d6b6`). Topologie client poste → serveur hypomnema → consommateur Kleos séparé ; démon, protocole, vue A+, rédaction, stockage et map transférés à hypomnema ; space transmis par une étiquette de hook ; map de toutes les sessions dès la mise en service ; rétention sans limite côté hypomnema ; mesures du lot 0 d'hypomnema intégrées (F21 à F26) ; section 11 « Consommateur Kleos » |
| 7.1 | 2026-10-26 | Contrat du §10 d'hypomnema épinglé à `ef02cee` : `secret_patterns_version` remplacée par `secret_patterns_rev` (ordonnée, garde du consommateur) et `secret_patterns_hash` (égalité seule) ; annexe A d'hypomnema, qui fait foi sur les sections reprises ; quatre divergences relevées en section 11 |
| 7.2 | 2026-10-26 | Contrat épinglé à `e99b9a2` (lot 3 d'hypomnema, `0b069bb`, puis réponses à Q11 et Q12) : `source` de chaque étiquette (`hook`, `marker_file`) ; `branch_root_changed` ajouté au flux ; étiquette lue dans `hook_success` seulement ; toute copie ouvre sa propre branche ; purge des époques remplacées conditionnée à l'enregistrement des consommateurs ; fichier de motifs renommé `redaction-patterns.toml` ; Q11 (inscription d'un consommateur) et Q12 (identité d'un fait `(lineage_id, fact_id)`, branche en attribut) tranchées par hypomnema |

### Ce que les révisions 2 à 7 changent

| Point du challenge, du consultant ou de l'opérateur | Réponse | Section |
|---|---|---|
| Opérateur (révision 7, 2026-10-24) : un outil autonome de transcripts, hypomnema, sert l'humain (UI), les agents (CLI, MCP) et Kleos ; le résumé se fait sur un serveur distant, PC source éteint ; la rédaction se fait sur le PC source ; seul le delta est envoyé (Kleos #33942) | Le démon poste devient `hypomnema-agent`, le relais devient `hypomnema-server`, propriétaire unique du stockage et du map (un seul passage produit le résumé et les faits candidats). Kleos garde un **consommateur séparé** qui lit le flux d'événements d'hypomnema et ne partage pas sa base | 1.1, 11 |
| Opérateur (révision 7) : portée du map (Kleos #33968) | Toutes les sessions dès la mise en service, sous-agents compris ; historique limité aux sessions team-lead et sans agent, du plus récent au plus ancien, profondeur `backfill_days` | 11.2, Q4 |
| Opérateur (révision 7) : rétention et secrets (#33942, #33978) | Rétention sans limite de tout l'extrait côté hypomnema, sans niveaux ; pas de rotation des secrets de F17 ; le fichier de motifs vit dans le dépôt hypomnema, le consommateur en épingle une copie | 1.4, 11.4, Q6 |
| Revue Kleos de la conception hypomnema (14 points, intégrés au commit `009d6b6`) | Space et racine git résolus sur le poste, étiquettes de session lues par égalité stricte d'un `additionalContext` entier, préfixe forké jamais remappé, schéma de fait avec `fact_id` stable, flux typé avec acquittement, `same_view` sur la paire de versions | 11 |
| Lot 0 d'hypomnema (Kleos #33981) : T3-a contredit, rapport des sous-agents non confirmé, seuil des préfixes recalibré | F21 à F26 ; lignées à branches sœurs gérées par hypomnema ; `tool_result` d'agent du parent gardé entier | 0, 3.4, 4.3 |
| Opérateur (révision 6) : A+ au démon, le relais ne stocke plus B. Motif mesuré : des secrets réels sont dans les sorties d'outils (Kleos #32381, F17) et le poste garde les transcripts 180 jours (F20) | La décision « B stocké, A+ présenté » de la révision 2 est **remplacée** : le démon calcule la vue A+ et n'envoie qu'elle ; la seconde copie en B disparaît ; un retraitement avec une vue plus large passe par un renvoi depuis la source sous une nouvelle époque motivée (`view_upgrade`), borné aux fichiers encore sur le poste, poste allumé | 1.2, 1.4, 2.8, 3.2, 5, 6, Q1 |
| Challenger (révision 6, a) : la rédaction prévue laisse passer les secrets propres à Kleos et efface des faits (Kleos #32359, F19) | Rédaction **au niveau du jeton** (jamais la ligne), formats ajoutés (`kleos_`, `gh[opsur]_`, valeurs après `*_KEY`, `*_SECRET`, `*_PEPPER`, `*_TOKEN`, URL à identifiants, `PrivateKey =`), fixtures `env`, `Get-ChildItem Env:`, `docker inspect`, `.env`, `kleos.env`, `sqlite3` en `|` ; **détecteur de secrets côté relais avant toute publication Kleos**, en échec fermé, parce qu'une mémoire n'expire pas | 1.4, 4.4 (couche 0), 8 |
| Challenger (révision 6, b) : les injections du harnais passent pour du texte user (F18) | Texte user défini **par la structure** et en échec fermé ; corps de skill, `<task-notification>`, blocs `=== … ===`, refus de hook dans les `tool_result` classés harnais ; tout préfixe de 40 caractères répété dans au moins 5 sessions classé injection ; alerte si la part de texte user gardé double après une mise à jour de Claude Code | 3.3, 6, 8 |
| Challenger (révision 6, c) : « extraire là où c'est écrit » n'était appliqué qu'aux `<channel>` | Généralisé : un store Kleos de l'agent devient un marqueur `{stored: id}` qui sert de liste négative au juge ; un `send_message` identique envoyé à plusieurs pairs est dédupliqué dans la session ; un `<channel>` sans original transmis (émetteur `operator`, Deck, pair hors parc des démons) reste en entier ; `subagents/` envoyés dès la v1 | 3.3, 3.4, 4.2, 4.6 |
| Challenger (révision 6, points secondaires) | Erreurs Windows en français dans le filtre des lignes d'erreur ; ligne complète illisible traitée explicitement ; branches abandonnées par `/rewind` non extraites comme valides ; un enregistrement énorme ne bloque pas le fichier | 2.3, 3.3, 4.3, 6 |
| (1) Une ligne `pending` absorbe les stores ultérieurs de l'agent | **Sans objet depuis la révision 4** : la porte ne sera jamais activée, aucune ligne `pending` n'est écrite | 4.4 |
| Opérateur (révision 5) : dédup de fond comme couche principale contre le transcript publié avant l'agent ; space par session ; juge cloud admis ; licence upstream | Niveau déterministe déjà assuré par le Patch 60 à 0,9, bande grise [0,80 ; 0,90[ jugée par Sonnet dans le relais (décision opérateur du 2026-10-22, 4.8), file `to_dedup` en table du relais ; résiduel réécrit (le mauvais ordre n'existe qu'au-delà de 8 h) ; space résolu sur le poste depuis le fichier de session du hook ; juge amont Haiku ; rev épinglé conditionné au gel ELv2 | 1.3, 4.4, 4.5, 4.8, 8, Q5 |
| Opérateur (révision 4) : pas de porte ; des faits de Qwen peut-être faux seraient servis dès publication ; aucune relecture humaine possible | Protection autonome à zéro delta en quatre couches : juge amont dans le relais (fidélité et voisinage) avant toute publication, importance plafonnée à 5, provenance par source et tags, juge nightly qui archive a posteriori ses seules mémoires de transcript. Les couches 1 à 3 bloquent la mise en service, la couche 4 et le nettoyage des mémoires d'agents non | 4.4, 4.6, 4.7 |
| (2) La règle « non révisé après 45 min » ne tient pas à la reprise | Parade : état par lignée hérité à la reprise, publication à la stabilisation (8 h), révision des faits publiés par la chaîne de versions de Kleos | 4.2, 4.3 |
| (3) Coût du relais sous-estimé : secrets, chiffrement, authentification | Parade pour chacun, coût chiffré ; limite nommée : la rédaction des secrets est incomplète par nature. **Révision 6** : la seconde copie en B disparaît, un détecteur avant publication s'ajoute | 1.4 |
| (4) Le budget Ollama ignore `KLEOS_LLM_THINK` | Parade : `think: false` explicite et rejet des réponses tronquées ; budget recalculé sur T2 | 3.2, 4.2 |
| Idée du challenger : stocker B, ne présenter que A+ au modèle | Retenue en révision 2 ; **remplacée en révision 6** par A+ calculé au démon (voir la première ligne de cette table) | 3.2 |
| Consultant (révision 3) : R, mais le relais dépend de kleos-lib par un rev épinglé | Intégré : E3 et E7 neutres, effort de 1.4 ramené à 0,5 à 1 jour, lot F de gel avant tout code | 1.3, 1.4, 8, Q5 |

### Légende des étiquettes

- **MESURÉ (rapporté, #id)** : mesure faite par l'opérateur ou un autre peer, consultée
  dans Kleos pour cette étude, non réexécutée ici.
- **MESURÉ** sans mention : commande lancée pour cette étude.
- **DÉDUIT** : raisonnement sur du code lu, avec `fichier:ligne`.
- **SUPPOSÉ** : non vérifié. Une porte de lot (section 8) le transforme en mesure quand
  il porte une décision.

---

## 0. Faits d'entrée

| # | Fait | Étiquette |
|---|---|---|
| F1 | 4 751 fichiers `.jsonl`, 7,0 Go ; fichier max 60 Mo, p90 3,5 Mo ; flux moyen environ 300 Mo par jour sur 8 jours, pic 562 Mo ; sous-agents 10 % du stock | MESURÉ (rapporté, #31675) |
| F2 | La conversation utile (texte user + texte assistant) pèse 5 à 9 % des octets ; `attachment` 29 %, `api-request*` 16,7 %, `thinking` environ 7 % | MESURÉ (rapporté, #31675) |
| F3 | Purge complète : environ 14,7 % restent avec `tool_use`/`tool_result`, 2,0 % sans eux (texte assistant 1,89 %, texte user tapé 0,13 %) ; soit environ 44 Mo ou 6 Mo par jour. 73 % du texte user est injecté par le harnais | MESURÉ (rapporté, #31699) |
| F4 | Curseur = offset d'octets : 9,8 % des lignes datées ont un timestamp non monotone, 29 % des lignes n'ont pas d'uuid | MESURÉ (rapporté, #31676) |
| F5 | `/compact` écrit `compact_boundary` et une ligne user `isCompactSummary` dans le MÊME fichier ; reprise ou fork recopie l'historique dans un nouveau fichier avec les MÊMES uuid sous un nouveau `sessionId` (100 paires de fichiers) ; sous-agents sous `<projet>/<sessionId>/subagents/` | MESURÉ (rapporté, #31676) |
| F6 | Le caractère « ajout seul » des fichiers n'est pas prouvé : second passage de comparaison à faire | DÉDUIT (rapporté, #31676) |
| F7 | kleos-ingest sur 20 vrais transcripts : 541 candidates, environ 2 utiles sur 20, injections du harnais prises pour du texte user ; le résumé couvre 1,7 % de la conversation en moyenne, 0,4 % sur 59 Mo | MESURÉ (rapporté, #31682) |
| F8 | Le résumé lit depuis la première ligne et s'arrête à 3 000 caractères, `max_tokens` 300 | DÉDUIT (rapporté, #31653, `kleos-ingest/src/summarizer.rs:16-67`) |
| F9 | Le classifieur du mode auto refuse aux agents la lecture analytique des transcripts : toute mesure sur les transcripts réels est un geste opérateur | MESURÉ (rapporté, #31658) |
| F10 | Cible LLM serveur : `qwen3:8b-ctx16k`, contexte 16 384 tokens, sur Ollama 192.168.10.50 | DÉDUIT (`CLAUDE.local.md`, section Cible LLM) |
| **T1** (historique, sans objet depuis la révision 4) | Porte active, `KLEOS_REVIEW_GATE_SOURCES=kleos-transcript` : un store `claude-session` quasi identique à une ligne `pending` rend `created=false, duplicate_of=<id pending>` ; aucune ligne approuvée n'est créée | MESURÉ (rapporté, #31707, test d'intégration jetable) |
| T1-a | Cause : la requête de dédup exclut `is_archived = 1` et `status = 'rejected'`, pas `pending` ; le chemin doublon rend l'id existant sans toucher la ligne | DÉDUIT (`kleos-lib/src/memory/mod.rs:586-590`, `:607-616`) |
| T1-b | En production, `/etc/kleos/kleos.env` ne contient aucune variable `REVIEW_GATE` ; `KLEOS_REVIEW_GATE_ENABLED` vaut `false` par défaut : **la porte est éteinte aujourd'hui** | MESURÉ (rapporté, brief du lead) ; défaut DÉDUIT (`mod.rs:777-781`) |
| **T2** | `qwen3:8b-ctx16k` par `/api/chat`, fenêtre de 36 000 caractères = 9 084 tokens, raisonnement coupé : ingestion du prompt à froid 6,2 s (environ 1 470 tokens/s), génération environ 49 tokens/s ; environ 12 s pour 300 tokens de sortie, 22 s pour 800. Un préfixe identique est servi par le cache KV (18 à 46 ms), donc un appel répété sous-estime le coût réel. Avec `think: true` et `num_predict` 800, le budget part dans le raisonnement et l'appel s'arrête en `length` | MESURÉ (rapporté, #31706) |
| T2-a | La production pose `KLEOS_LLM_THINK=0`, mais cette variable est lue par kleos-server (Patch 14) ; un processus séparé n'en hérite pas | MESURÉ (rapporté, brief du lead) ; héritage DÉDUIT (`CLAUDE.local.md`, section Cible LLM) |
| **T3** | 687 sessions actives sur 14 jours : 28,97 % ont au moins un écart de plus de 45 min suivi d'autres tours ; 18,05 % à 60 min, 7,71 % à 2 h, 0,73 % à 8 h. En ne comptant que les prompts tapés par un humain : 14,41 % à 45 min, 0,58 % à 2 h. La moitié des reprises sont déclenchées par un message injecté (canal peer, notifications). Les 355 écarts de plus de 45 min : médiane 55 min, p90 170 min | MESURÉ (rapporté, #31716) |
| T3-a | Lignée resume/fork : 6 copies sur 687 sessions ; la copie démarre p50 16 min après la source ; 2 cas sur 7 au-delà de 45 min ; aucune source n'a continué après le fork. **Contredit en révision 7 par F21** sur un corpus plus large | MESURÉ (rapporté, brief du lead ; le brief donne 6 copies et 7 cas, écart non résolu ici) |
| F11 | `POST /memory` désérialise `StoreRequest` tel quel : `parent_memory_id`, `created_at`, `space`, `session_id`, `tags` et `sync_id` passent par l'API publique ; corps limité à 256 Kio | DÉDUIT (`kleos-server/src/routes/memory/mod.rs:74`, `:110-136` ; `kleos-lib/src/memory/types.rs:306-342`) |
| F12 | Un store avec `parent_memory_id` crée la version suivante, passe l'ancienne à `is_latest = 0`, saute la dédup, et **refuse** (`Conflict`) si le parent n'est plus la tête de chaîne | DÉDUIT (`mod.rs:578`, `:855-890`) |
| F13 | La porte de revue ne sera jamais activée ; l'inbox d'approbation existe et se consulte depuis l'interface web GUI de Kleos, pas depuis kleos-cli ni le serveur MCP | Décision opérateur du 2026-10-21 (Kleos #31722) |
| F14 | Tiers servis sans requête : le tier « important » de `/recall` prend `importance >= 7` (`RECALL_IMPORTANT_MIN`, `kleos-server/src/routes/memory/mod.rs:681`, appel `:770`) ; `/context` ne remonte d'office que les statiques d'importance au moins 9 (`kleos-lib/src/context/mod.rs:601`) et les insights d'importance au moins 8 (`kleos-lib/src/context/deps.rs:117`) | DÉDUIT (lignes lues) |
| F15 | Filtres de lecture existants : `source_filter` et `tags` sont des filtres **positifs** (égalité de source, `kleos-lib/src/memory/search.rs:1543-1575`) ; aucun filtre d'**exclusion** par source n'existe. Seules les catégories `activity` et `growth` sont exclues par défaut de la recherche (`search.rs:1101-1107`) | DÉDUIT (lignes lues) ; absence d'un filtre d'exclusion SUPPOSÉE (lecture de `search.rs` seulement) |
| F16 | La dédup de fond (Patch 60) garde l'importance la plus haute, et à égalité l'id le plus haut | DÉDUIT (`docs/dev-notes/local-patches.md:6606-6613`) |
| **F17** | Secrets réels dans le corpus du poste (4 777 fichiers, 7,1 Go le 2026-10-22 ; comptes de valeurs distinctes, aucune valeur affichée) : 3 clés `kleos_<hex32>`, 4 valeurs `KLEOS_*_KEY` suivies de 32 hex ou plus, 12 jetons GitHub `gh[ousr]_` (les formats `gho`/`ghu`/`ghs`/`ghr` manquaient à la passe prévue, qui ne connaissait que `ghp_` et `github_pat_`), 51 URL de forme `scheme://user:secret@` (candidats, faux positifs possibles), 0 clé WireGuard | MESURÉ (rapporté, #32381) |
| **F18** | Injections du harnais dans le corpus : `=== RECENT MEMORIES ===` 7 948 fois en lignes `attachment` et 37 fois en lignes `user` ; corps de skill « Base directory for this skill » 2 673 fois en lignes `user` ; `<task-notification>` 1 037 fois en lignes `user` ; refus de hook `PreToolUse:… hook error` 6 285 fois dans des `tool_result` de lignes `user` | MESURÉ (rapporté, #32381) |
| **F19** | `scrub_credentials` : le lexique `credential_keywords` (`en.toml`) contient `password`, `passwd`, `passphrase`, `secret`, `api_key`, `apikey`, `token`, `bearer`, `authorization`, `credential`, `private_key`, `ssh_key`, `client_secret`, mais **ni `key` seul, ni `privatekey`, ni `api-key`** ; la passe exige `=` ou `:` sans `://` sur la ligne et **remplace la ligne entière**. Donc `KLEOS_DB_KEY`, `KLEOS_IDENTITY_KEY`, `KLEOS_SESSION_KEY` et les clés `kleos_<hex>` passent, et une ligne de fait contenant `token` (« 9 084 tokens ») est effacée | DÉDUIT (rapporté, #32359, `kleos-lib/src/prompts.rs:262-289` et lexique `en.toml`) |
| **F20** | Le poste garde les transcripts Claude Code 180 jours : `"cleanupPeriodDays": 180,` dans `~/.claude/settings.json` | MESURÉ (`grep cleanupPeriodDays ~/.claude/settings.json`, ligne 3) |
| **F21** (révision 7) | **T3-a est contredit.** 65 familles de fichiers partagent leur premier uuid (148 fichiers) ; dans 50 d'entre elles, les deux fichiers ont continué (87 couples divergents contre 17 inclusions strictes). Les lignées ont des branches sœurs, pas seulement des suites linéaires | MESURÉ (rapporté, lot 0 hypomnema, 3 790 sessions, #33981) |
| **F22** (révision 7) | Sur un fork, l'historique recopié forme un préfixe contigu, et les lignes recopiées portent le `sessionId` de la nouvelle session, sans champ qui nomme la source ; 4 lignes `continued-in` concordent avec la règle d'inclusion | MESURÉ (rapporté, #33908, #33981) |
| **F23** (révision 7) | Le dernier texte assistant d'un sous-agent n'est retrouvé dans le `tool_result` du parent que 67 fois sur 1 335 (comparateur de 80 caractères, faux négatifs possibles) ; 329 sous-agents n'ont aucun texte final. L'hypothèse « le rapport est le dernier message » n'est pas confirmée | MESURÉ (rapporté, #33981) |
| **F24** (révision 7) | Préfixes de 40 caractères vus dans au moins 5 sessions : 76. Le seuil seul retirerait 30,4 M caractères (dont 1 152 `isCompactSummary` et des missions de pairs) ; le seuil combiné à un ratio textes distincts sur blocs au plus 0,2 retient 1 092 blocs pour 47 k caractères, de vraies injections | MESURÉ (rapporté, #33981) |
| **F25** (révision 7) | `/rewind` ou réédition : 166 sessions (4,4 %), 285 points. `entrypoint` : `cli` 3 434, `sdk-cli` 351, `claude-desktop` 1 | MESURÉ (rapporté, #33981) |
| **F26** (révision 7) | La sortie d'un hook SessionStart est écrite deux fois dans le transcript : dans `hook_success.stdout` et comme élément de `hook_additional_context` ; 9 272 éléments sur 9 348 égalent l'`additionalContext` entier d'un hook du même fichier | MESURÉ (rapporté, #33981) |

---

## 1. Composants et frontières

> **Révision 7.** Topologie remplacée : client `hypomnema-agent` sur le poste, serveur
> `hypomnema-server` (propriétaire unique du stockage et du map), consommateur Kleos
> séparé qui lit l'API du serveur (conception hypomnema §3, §7.1, §10 ; ici section 11).
> Restent valides pour Kleos : le verdict R en 1.3, relu en 11.1 (le consommateur reste
> un processus séparé de kleos-server, pour l'isolement des pannes, puisque le gel
> upstream annule E9), et la couche 0 de 1.4, désormais appliquée par le consommateur.
> La rédaction au démon de 1.4 est remplacée par la conception hypomnema §6 (fichier de
> motifs versionné dans le dépôt hypomnema, mêmes trois passes au jeton). Le schéma 1.1
> et les tables 1.2 et 1.4 décrivent la révision 6.

### 1.1 Vue d'ensemble

Le schéma montre l'option R (relais séparé). Dans l'option S (section 1.3), le bloc
« relais » devient un module de kleos-server et son stockage des tables overlay ; le
démon et son protocole ne changent pas.

```mermaid
flowchart LR
  subgraph Poste["Poste Windows"]
    CC["Claude Code<br/>~/.claude/projects/**/*.jsonl"]
    D["kleos-transcript-agent<br/>(démon poste)"]
    DS[("état poste<br/>SQLite")]
    CC -- lecture seule --> D
    D <--> DS
  end
  subgraph LXC121["LXC 121"]
    R["kleos-transcript-relay<br/>(réception + traitement)"]
    RS[("stockage relais<br/>SQLCipher")]
    K["kleos-server<br/>(upstream + patches)"]
    R <--> RS
    R -- "POST /memory, Bearer<br/>source=kleos-transcript" --> K
  end
  O["Ollama 192.168.10.50<br/>qwen3:8b-ctx16k"]
  D -- "segments A+ rédigés<br/>Bearer par poste" --> R
  R -- "fenêtres A+ <= 16k<br/>think: false" --> O
  K -- "juge nightly<br/>(étude 4.6)" --> K
```

Révision 6 : le démon calcule la vue A+ (3.3) et la rédaction (1.4) ; le relais ne
reçoit ni ne stocke B. Avant `POST /memory`, le relais passe chaque fait candidat par
le détecteur de secrets (1.4, couche 0 de 4.4) puis par le juge amont (4.4, couche 1).

### 1.2 Qui détient quoi

| Composant | Détient | Ne détient jamais | Écart upstream (table CLAUDE.md) |
|---|---|---|---|
| Claude Code | Les transcripts bruts, seule copie complète, gardée 180 jours sur le poste (F20) | | hors périmètre |
| Démon poste `kleos-transcript-agent` | Curseur par fichier (offset acquitté, époque et son motif, empreintes), version de purge du fil (`wire_purge_version`, qui couvre depuis la révision 6 la vue A+ et la rédaction), table des préfixes répétés (3.3). **Aucune copie** du transcript : le fichier source sert de tampon et de source de tout retraitement (2.8) | Contenu purgé persistant, mémoires, appel LLM | **Zéro delta** si dépôt séparé (Q5) |
| Relais `kleos-transcript-relay` | Enregistrements **A+ rédigés** (rétention 14 jours ; révision 6, plus de B), index des uuid (longue durée), état de lignée, résumés de fenêtre et de session, faits candidats, correspondance fait vers id et version Kleos, index des `send_message` émis (3.4), file de dédup (4.8) | Les mémoires canoniques, l'autorité de retrait sur une mémoire qu'il n'a pas écrite ; **B, les `tool_result` entiers, les contenus de fichiers lus** (révision 6) | Option R : **zéro delta**. Option S : voir 1.3 |
| kleos-server | Mémoires canoniques, chaîne de versions, archivage réversible, voisinage et annotations de l'étude nightly | Le transcript, même réduit (option R) ; aucune mémoire de transcript en statut `pending` (F13) ; **aucun secret** : le détecteur de 1.4 bloque la publication (révision 6) | **Zéro delta** pour la cible : publication, révision, recherche des voisins et archivage passent par l'API existante (4.4) |
| Ollama | Rien de persistant | | hors périmètre |

### 1.3 Embranchement pour le consultant : relais séparé (R) contre staging dans Kleos (S), tranché R

> Ce passage est autonome : il peut être lu sans le reste de l'étude.

**La question.** Les segments de transcript envoyés par le démon poste, leur état de
traitement et les faits candidats doivent vivre quelque part sur LXC 121, pendant 14
jours, avant que des faits n'en sortent vers les mémoires Kleos. Deux emplacements :

- **R, relais séparé** : un binaire et un service systemd distincts, dans un dépôt
  séparé, avec une base propre. Il publie dans Kleos par l'API HTTP publique et
  **dépend de kleos-lib par une dépendance git épinglée sur le fork VOCSAP** (prémisse
  corrigée par le consultant, voir « Verdict » ci-dessous), sans entrer dans les
  `members` du `Cargo.toml` racine.
- **S, staging dans Kleos** : des tables ajoutées par le canal overlay Patch 41
  (`kleos-lib/src/db/vocsap/mod.rs`), des routes de réception et un job de traitement
  dans kleos-server, déclenché par le dreamer.

**Ce qui ne dépend pas du choix.** Le démon poste, son protocole (curseur, époque,
idempotence par `(file_key, epoch, start)`, réponses 409, section 2), la règle de purge,
le fenêtrage et la règle de publication sont identiques dans R et S. Le démon est
la partie la plus délicate (Windows, arrêts brutaux, F4 à F6) : **si le contrat HTTP
démon vers serveur est figé d'abord, l'embranchement n'engage que le côté serveur.**

**Faits qui pèsent.**

| # | Fait | Étiquette | Penche vers |
|---|---|---|---|
| E1 | L'API publique accepte tout ce que la publication demande : `parent_memory_id` (révision), `created_at` (date du tour), `space`, `session_id`, `tags` | DÉDUIT (`routes/memory/mod.rs:110-136`, `types.rs:306-342`) | R : aucun accès interne requis |
| E2 | La révision d'un fait publié passe par la chaîne de versions, qui refuse un parent périmé | DÉDUIT (`memory/mod.rs:855-890`) | neutre : disponible en R par l'API, en S par appel direct |
| E3 | Le réglage du raisonnement et la résolution `prompts-overrides` sont des fonctions publiques de kleos-lib : `llm::think_setting` lit `KLEOS_LLM_THINK` (`kleos-lib/src/llm/mod.rs:45`), `llm::prompts::load_prompt` applique la cascade et le cache (`kleos-lib/src/llm/prompts.rs:97`). Avec le raisonnement actif, l'appel s'arrête en `length` (T2) | DÉDUIT (lignes lues) ; T2 MESURÉ (rapporté, #31706) | **Neutre** avec la prémisse corrigée : R les appelle via la dépendance épinglée ; il doit seulement porter `KLEOS_LLM_THINK=0` dans son propre environnement, puisque un processus séparé n'hérite pas de celui de kleos-server (T2-a) |
| E4 | La garde d'inactivité du dreamer mesure le trafic HTTP de kleos-server (`dreamer.rs:193-206`) | DÉDUIT par l'étude nightly 4.4, non relu ici | S : garde gratuite ; R : la remplace par `/api/ps` d'Ollama (SUPPOSÉ disponible) |
| E5 | La contention de calcul porte sur l'hôte Ollama 192.168.10.50, partagé par les deux options ; LXC 121 ne fait que du I/O et du JSON | DÉDUIT (F10, 1.1) | neutre : l'isolement de R protège le processus et la base de Kleos, pas le GPU |
| E6 | Volume stocké en B : environ 44 Mo par jour, soit environ 620 Mo sur 14 jours, plus l'index des uuid. **Révision 6** : le relais ne stocke plus que A+, au plus 9 Mo par jour (cible 3.2), soit environ 130 Mo sur 14 jours ; le chiffre B reste comme borne haute | DÉDUIT (F1, F3) | R : hors de `kleos.db` ; S : dans la base principale, sa WAL et ses sauvegardes ; l'écart se réduit avec A+ mais ne change pas le sens |
| E7 | Chiffrement au repos et rédaction : `Database::connect_encrypted` applique une clé de 32 octets par `PRAGMA key` (`kleos-lib/src/db/mod.rs:57-65`), avec la feature `sqlcipher` de kleos-lib (`kleos-lib/Cargo.toml:30`) ; `prompts::scrub_credentials` est public (`kleos-lib/src/prompts.rs:262`, module `pub mod prompts`, `lib.rs:48`). L'authentification démon vers relais reste à fournir par R (1.4). **Révision 6** : `scrub_credentials` n'est plus réutilisé tel quel, parce qu'il remplace la ligne entière (F19) ; seul son lexique `credential_keywords` est repris (1.4). La rédaction redevient du code propre au démon, dans R comme dans S | DÉDUIT (lignes lues). Chiffrement de `kleos.db` en production SUPPOSÉ, non vérifié non plus par le consultant | **Neutre** pour le chiffrement ; neutre aussi pour la rédaction depuis la révision 6, puisqu'aucune option ne la réutilise ; S garde l'avantage sur l'authentification seulement |
| E8 | Les routes mémoire limitent le corps à 256 Kio ; la réception de segments demande sa propre route et sa propre limite | DÉDUIT (`routes/memory/mod.rs:74`) | neutre : route neuve dans les deux options |
| E9 | **Force décisive.** Upstream a touché `kleos-server/src/dreamer.rs` dans chacun des 4 merges de juin à août (`50e03774`, `61fd2acf`, `cb1bff54`, `733a9466`), et `docs/dev-notes/local-patches.md:125` classe déjà ce fichier « ELEVE (2 blocs) ». L'API publique de kleos-lib a bougé aussi (`llm/mod.rs` : `f857f7cd`, `2a6f0f9a`, `7e207b90`), mais bien moins que l'intérieur du dreamer | Ligne `local-patches.md:125` DÉDUITE (lue) ; liste des commits MESURÉE (rapporté, consultant) | R : dépend de l'API publique de kleos-lib, pas de l'intérieur du dreamer |
| E10 | La ligne `members` du `Cargo.toml` racine liste 26 crates sur une seule ligne ; upstream l'a modifiée 8 fois en deux mois | Forme DÉDUITE (`Cargo.toml:3`) ; fréquence MESURÉE (rapporté, consultant) | R hors workspace : un crate ajouté à cette ligne serait un conflit à presque chaque pull |

**Coût de rebase.**

| | R, relais séparé | S, staging dans Kleos |
|---|---|---|
| Fichiers upstream touchés | Aucun ; pas d'entrée dans `members` (E10) | Enregistrement des routes dans le routeur serveur et branchement d'un job dans le dreamer : deux patchs chirurgicaux, dont un dans le fichier classé « ELEVE » (E9) |
| Tables | Base propre | Overlay Patch 41 : zéro delta par construction |
| Coût récurrent | Un bump du rev épinglé de kleos-lib après chaque merge upstream du fork ; ne casse que si l'API publique utilisée change (`think_setting`, `load_prompt`, `connect_encrypted`, `scrub_credentials` ; révision 6 : `scrub_credentials` sort de la liste, remplacé par le chargeur du lexique `credential_keywords`, voir 1.4) ou si l'API HTTP `/memory` change | Chaque pull upstream qui modifie le dreamer, le client LLM ou la signature de `memory::store` casse la compilation du module, même sans conflit de texte ; coût proportionnel à la cadence upstream (E9) |
| Patch numéroté à tenir dans `local-patches.md` | Non | Oui |

**Coût propre de chaque option.**

| | R | S |
|---|---|---|
| À construire en plus | Jeton par poste, gestion de la clé SQLCipher, unité systemd, sauvegarde de l'index et de la correspondance, règle de bump du rev épinglé ; le client LLM, les prompts, le chiffrement et la passe de rédaction par mots-clés viennent de kleos-lib | Routes, job, tables overlay, module ; rien pour l'authentification, le chiffrement et le LLM |
| Effort supplémentaire, SUPPOSÉ | Environ 0,5 à 1 jour (section 1.4, révisé à la baisse par la prémisse corrigée) | Équivalent en code de module, plus l'intégration au dreamer et aux tests serveur |
| Panne | Une panne du relais n'affecte pas Kleos, et inversement ; le démon recule, le fichier sert de tampon | Une boucle ou une fuite mémoire du job touche le processus qui sert les agents |

**Ce que coûterait de s'être trompé.**

- **R choisi, S s'avère meilleur** (par exemple, l'API publique de kleos-lib casse à
  chaque pull, voir « Ce qui renverserait le verdict ») : porter les tables du relais en
  overlay Patch 41, déplacer la boucle de traitement dans un module serveur, exposer les
  mêmes routes de réception. Le démon ne change pas si le contrat 2.4 est repris tel
  quel ; la publication devient un appel direct au lieu d'un appel HTTP. Le code qui
  appelle kleos-lib se déplace sans réécriture, puisqu'il utilise déjà les mêmes
  fonctions. Refactor borné au côté serveur, données de 14 jours seulement à migrer (le
  reste se recalcule), plus l'index des uuid et la correspondance des faits. Ordre de
  grandeur SUPPOSÉ : quelques jours.
- **S choisi, R s'avère meilleur** (conflits récurrents au merge, ou contention du
  processus Kleos) : écrire le relais de toute façon, extraire les tables de
  `kleos.db`, retirer les deux patchs et le module, rétablir l'authentification et le
  chiffrement hors de Kleos. Le démon ne change pas non plus. Plus coûteux que le sens
  inverse : il faut défaire du delta upstream en plus de construire.

**Verdict du consultant (2026-10-21) : R, avec une prémisse corrigée.**

- **Prémisse corrigée.** La révision 2 décrivait R comme un client HTTP qui réécrit le
  client LLM, les prompts et le chiffrement. Le relais est un binaire dans un dépôt
  séparé qui dépend de kleos-lib par une dépendance git épinglée sur le fork VOCSAP, sur
  le modèle de kleos-ingest (qui, lui, dépend de kleos-lib par chemin dans le workspace,
  DÉDUIT de `kleos-ingest/Cargo.toml:26`). Il réutilise `think_setting`, `load_prompt`,
  `connect_encrypted` avec la feature `sqlcipher`, et `scrub_credentials`. **E3 et E7
  deviennent neutres.** Il n'entre pas dans les `members` du `Cargo.toml` racine (E10).
- **Force décisive : E9.** Le dreamer, où S devrait se brancher, est la zone la plus
  agitée côté upstream ; l'API publique de kleos-lib bouge aussi, mais bien moins.
- **Ce qui renverserait le verdict.**
  1. Une API kleos-lib utilisée par le relais qui casse à chaque pull.
  2. Une transaction SQLite partagée exigée entre le juge et le staging.
  3. Une mise en service qui ne survivrait à aucun pull.
- **À figer avant tout code.**
  1. Le contrat HTTP démon vers relais (sections 2.3 et 2.4).
  2. Retiré en révision 4 : le correctif (a') de la dédup n'a plus d'objet sans porte
     (F13).
  3. Le rev épinglé de kleos-lib et sa règle de bump : il suit chaque merge du fork
     après un build vert, donc `418cdd26` ou un commit postérieur dès que le relais
     existe. Il reste simultanément antérieur à `f1db1cf7` tant que le gel ELv2 tient.
     La décision opérateur du 2026-10-22 a mergé upstream jusqu'à `8182e57f` avec
     `418cdd26`, puis a exclu `f1db1cf7` et tout commit ultérieur sous PolyForm
     Noncommercial (Kleos #32284). Voir [Gel upstream](../dev-notes/local-patches.md#gel-upstream).
     L'API de kleos-lib utilisée par le relais cesse donc de bouger côté upstream, ce
     qui renforce le verdict R : la condition 1 de renversement est levée.
  4. La liste des champs de `StoreRequest` utilisés par le relais (`content`,
     `category`, `source`, `importance`, `tags`, `session_id`, `space`,
     `parent_memory_id`, `created_at`), avec un test d'intégration jetable rejoué à
     chaque bump. Révision 6 : les fonctions de kleos-lib couvertes par ce test sont
     `think_setting`, `load_prompt`, `connect_encrypted` et le chargeur du lexique
     `credential_keywords` ; `scrub_credentials` n'en fait plus partie (1.4, F19).
- **Non vérifié par le consultant, et toujours SUPPOSÉ ici :** l'existence de `/api/ps`
  sur la version d'Ollama déployée (4.1), et le chiffrement de `kleos.db` en production
  (E7).

**Conséquence sur la réversibilité.** Comme R appelle les mêmes fonctions de kleos-lib
que S appellerait, la bascule R vers S ne réécrit plus de client LLM ni de chiffrement :
l'asymétrie en faveur de R s'accentue (DÉDUIT). La révision 6 ne touche pas ce verdict :
la rédaction et la vue A+ vivent dans le démon, hors de l'embranchement R contre S ; le
détecteur de secrets avant publication (1.4) est du code du relais, qui se déplacerait
avec lui dans une bascule vers S.

### 1.4 Sécurité et exploitation du relais (point 3 du challenge)

La décision opérateur du 2026-10-20 dit que la sécurité des transcripts n'est pas un
frein, parce que les sessions Claude Code détiennent déjà tous les secrets lisibles
(MESURÉ rapporté, #30938). Les révisions 2 à 5 faisaient pourtant du relais **une
seconde copie sur une seconde machine**, en B, avec les `tool_result` entiers, donc des
sorties de commandes et des contenus de fichiers. **Révision 6 : cette seconde copie
disparaît.** Le relais ne reçoit que la vue A+ (3.3), où les `tool_result` sont réduits
à 200 caractères et à leurs lignes d'erreur ; la mesure F17 montre que les secrets réels
du corpus vivent surtout dans ces sorties d'outils (`env`, `Get-ChildItem Env:`,
`docker inspect`, `cat .env`, `kleos.env`, `sqlite3` en colonnes `|`), et la mesure F19
montre que la passe par mots-clés prévue ne les reconnaissait pas. Deux barrières
distinctes en découlent : la **rédaction au démon**, avant tout envoi, et le
**détecteur de secrets au relais**, avant toute publication Kleos, parce qu'une mémoire
n'expire pas alors qu'un enregistrement du relais expire en 14 jours. Chaque manque
nommé par le challenge reçoit une parade.

| Manque | Parade | Coût, SUPPOSÉ | Limite restante |
|---|---|---|---|
| Purge des secrets, au démon | **Rédaction à la source**, dans le démon, **avant la réduction A+** (sinon les 200 premiers caractères d'un `tool_result` emportent le début d'une sortie `env`), **au niveau du jeton : la valeur est masquée, jamais la ligne** (révision 6 ; F19 montre qu'un remplacement de ligne efface des faits comme « 9 084 tokens »). Trois passes : (1) formats de jetons connus : `ghp_`, `github_pat_`, **`gh[opsur]_` (12 jetons `gho`/`ghu`/`ghs`/`ghr` dans le corpus, F17)**, `sk-`, `xox[bpas]-`, `AKIA`, JWT `eyJ…`, blocs PEM, en-têtes `Authorization`/`Bearer`, **`kleos_[0-9a-f]{32,64}`** (3 clés dans le corpus, F17), **identifiants dans les URL `scheme://user:secret@`** (51 candidats, F17 ; seul `secret` est masqué, l'hôte et le chemin restent), **`PrivateKey =`** (0 dans le corpus, F17, gardé par principe) ; (2) passe par mots-clés **au niveau de la valeur** : la valeur qui suit un nom se terminant par `_KEY`, `_SECRET`, `_PEPPER`, `_TOKEN` (4 valeurs `KLEOS_*_KEY` dans le corpus, F17), ou un mot du lexique `credential_keywords` de kleos-lib (repris par la dépendance épinglée, **sans appeler `scrub_credentials`**, qui remplace la ligne, F19), avec `=`, `:`, des espaces en colonnes (`Get-ChildItem Env:`, `docker inspect`) ou `|` (`sqlite3`) comme séparateurs ; (3) candidats d'entropie sur les jetons longs. Remplacement par `[REDACTED:<type>]` à la place de la valeur seule. La version de rédaction, celle de la vue A+ et le rev de kleos-lib entrent dans `wire_purge_version` ; un changement de version ouvre une montée de vue (2.8) | Passes (1), (2) et (3) à écrire ; la révision 2 réutilisait la passe (2), ce n'est plus le cas ; fixtures `env`, `Get-ChildItem Env:`, `docker inspect`, `.env`, `kleos.env`, `sqlite3` en `|` (lot 1). kleos-lib compile sous Windows MSVC, puisque kleos-ingest, qui en dépend, y a été porté (DÉDUIT, Patch 69) | **Nommée : la rédaction est incomplète par construction.** Un secret sans format connu, sans nom reconnu devant lui et sous le seuil d'entropie passe. Exposition bornée par la surface A+ (plus de `tool_result` entier), par la rétention de 14 jours et par le chiffrement. Un faux positif masque une valeur, jamais une ligne : le fait autour survit |
| **Secret dans un fait publié** (révision 6) | **Détecteur de secrets côté relais, avant toute publication Kleos**, en échec fermé : chaque fait candidat de l'état final de lignée passe par les trois passes ci-dessus (mêmes formats, même lexique, même version partagée `secret_patterns_version` entre démon et relais, plus la passe d'entropie) ; un fait qui déclenche le détecteur **n'est pas publié**, reste dans le relais avec le type détecté, et compte au rapport quotidien. Une publication déjà faite n'est jamais réexaminée par ce détecteur : la couche 4 (4.6) ne lit pas les secrets. Couche 0 de 4.4 | Un appel de fonction partagé entre démon et relais (même crate de rédaction, dépendance commune), une table de refus, fixtures (lot 1) | Mêmes angles morts que la rédaction : les deux barrières partagent les formats, donc un secret inconnu des deux passe jusqu'à Kleos ; seule la passe d'entropie sur un texte court (un fait fait rarement plus de 500 caractères) ajoute du rappel. Un faux positif retient un fait vrai dans le relais, consigné, jamais perdu avant 14 jours |
| Chiffrement au repos | Base du relais ouverte par `kleos_lib::db::Database::connect_encrypted` avec la feature `sqlcipher` de kleos-lib (DÉDUIT : `kleos-lib/src/db/mod.rs:57-65`, `kleos-lib/Cargo.toml:30`) ; clé de 32 octets dans un `EnvironmentFile` systemd en 0600 root, comme les secrets de kleos-server | Une feature de build et une clé, plus rien à écrire | La clé vit sur la même machine que la base, comme pour Kleos : protège une copie volée du disque, pas un accès root. Révision 6 : le contenu protégé est A+ seulement |
| Raisonnement du modèle | `kleos_lib::llm::think_setting` et l'injection associée (DÉDUIT : `kleos-lib/src/llm/mod.rs:45-50`), avec `KLEOS_LLM_THINK=0` posé dans l'`EnvironmentFile` du relais ; la vérification de `done_reason` (3.2) reste à écrire | Une variable et un contrôle | Si la variable manque, `think_setting` rend `None` et l'injection ne fait rien (DÉDUIT, même fonction) : le contrôle de `done_reason` est le filet |
| Authentification démon vers relais | Jeton Bearer **par poste**, stocké dans `cred` côté poste et injecté par `cred exec` ; le relais ne garde que son sha256 et le `host_id` associé ; un jeton révoqué rend 401 | Une table et un contrôle | Transport HTTP sur le LAN, comme Kleos aujourd'hui (DÉDUIT : `CLAUDE.local.md` appelle `http://192.168.10.21:4200`). TLS en option, non requis pour atteindre la posture de Kleos |
| Authentification relais vers Kleos | Clé API Kleos dédiée, portée écriture, dans l'`EnvironmentFile` | Une clé | Aucune au-delà de celles de Kleos |
| Exploitation | Unité systemd, journal, alerte sur `507` et sur fenêtres en erreur ; sauvegarde de l'index des uuid et de la correspondance des faits seulement (le reste se recalcule ou expire) | Une unité et un script | Un service de plus à surveiller (option R) |
| Suivi de kleos-lib | Rev git épinglé ; bump à chaque merge upstream du fork, après un build vert ; test d'intégration jetable des champs de `StoreRequest` utilisés et des quatre fonctions réutilisées, rejoué à chaque bump (1.3) | Un test et une ligne de procédure | Le bump est une étape de plus à chaque merge ; s'il casse à chaque fois, le verdict R est à rouvrir (1.3) |

**Effort révisé, SUPPOSÉ : environ 0,5 à 1 jour**, au lieu de 1 à 2 jours en révision 2.
Ce qui reste à écrire : le jeton par poste et sa table, les passes de rédaction (1) et
(3), le contrôle de `done_reason`, l'unité systemd et le test de bump. Le chiffrement,
la passe par mots-clés, le réglage du raisonnement et la résolution des prompts sont
réutilisés.

**Effort révisé en révision 6, SUPPOSÉ : environ 1,5 à 2,5 jours.** S'ajoutent : la
passe (2) au niveau du jeton (plus réutilisée), les formats de F17, les fixtures de
sorties d'outils, le détecteur au relais et sa table de refus, le crate de rédaction
partagé entre démon et relais. Le chiffrement, le réglage du raisonnement et la
résolution des prompts restent réutilisés. En contrepartie, la vue A+ côté serveur
(`model_view_version`, révision 2) disparaît, puisqu'elle est calculée au démon (3.2).

---

## 2. Protocole démon vers relais

> **Révision 7.** Repris par hypomnema entre son client et son serveur (conception §5),
> avec deux écarts : `wire_purge_version` est remplacé par la paire `view_version`,
> `secret_patterns_version`, et `same_view` se juge sur la paire (accepté si une version
> monte et qu'aucune ne baisse). L'époque appartient au client, qui lit la source. La
> lignée (2.6) est calculée par le serveur hypomnema avec une règle élargie par F21 :
> ligne `continued-in`, sinon étiquette `forked_from` de Kory, sinon inclusion, sinon
> branches sœurs. Kleos ne parle plus ce protocole : il lit le flux de 11.2.

### 2.1 Principe

Le fichier transcript est le tampon. Le démon ne conserve qu'un curseur par fichier ;
tout ce qui n'est pas acquitté est relu depuis le fichier au passage suivant. Le relais
acquitte un segment seulement après l'avoir écrit dans une transaction. L'effet est
« au moins une fois » sur le réseau, « exactement une fois » dans le stockage du relais
grâce à une clé d'idempotence.

**Révision 6 : la mécanique de cette section est conservée telle quelle**, bien que le
segment transporte désormais la vue A+ et non plus B (3.2). Elle reste valide parce que
les empreintes et les offsets portent sur le **brut** (règle 4 de 2.3) : le contenu
purgé peut changer de forme sans que le curseur, la chaîne des segments ni la dédup par
uuid ne changent. Ce qui s'ajoute : un **motif d'époque** (`epoch_reason`), qui
distingue une réécriture détectée d'une montée de vue demandée (2.8), et une règle sur
les faits déjà extraits lors d'une montée de vue (2.8).

### 2.2 Table d'état du démon (SQLite poste, `%LOCALAPPDATA%\kleos-transcript\state.db`)

```sql
CREATE TABLE files (
  path            TEXT PRIMARY KEY,          -- chemin absolu normalisé
  file_key        TEXT NOT NULL UNIQUE,      -- sha256(host_id || path relatif à projects/)
  session_id      TEXT NOT NULL,             -- nom du fichier principal (F5)
  kind            TEXT NOT NULL,             -- 'main' | 'subagent'
  parent_session  TEXT,                      -- pour kind = 'subagent'
  project_dir     TEXT NOT NULL,             -- dossier <projet> sous projects/
  space           TEXT,                      -- résolu une fois par session (4.5)
  space_rule      TEXT,                      -- règle qui l'a produit, ou 'none'
  epoch           INTEGER NOT NULL DEFAULT 0,-- incrémenté à chaque réécriture détectée ou montée de vue (2.8)
  epoch_reason    TEXT NOT NULL DEFAULT 'initial', -- initial | rewrite | view_upgrade (révision 6)
  sent_purge_ver  INTEGER NOT NULL DEFAULT 0,-- wire_purge_version sous laquelle l'époque courante est envoyée
  cc_version      TEXT,                      -- champ `version` de Claude Code lu sur la première ligne (3.3, alerte)
  acked_end       INTEGER NOT NULL DEFAULT 0,-- offset d'octets acquitté par le relais
  acked_tail_sha  TEXT,                      -- sha256 des <= 4 096 octets avant acked_end
  head_sha        TEXT,                      -- sha256 des <= 4 096 premiers octets
  size_seen       INTEGER NOT NULL DEFAULT 0,
  mtime_seen      INTEGER NOT NULL DEFAULT 0,
  state           TEXT NOT NULL DEFAULT 'active', -- active | rewritten | ignored | error
  last_error      TEXT,
  updated_at      INTEGER NOT NULL
);
CREATE TABLE meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
-- meta : host_id, wire_purge_version, relay_url (le jeton vit dans cred, pas ici)
CREATE TABLE user_prefixes (              -- révision 6, 3.3 : préfixes de texte user répétés
  prefix40        TEXT PRIMARY KEY,         -- 40 premiers caractères normalisés d'un bloc text user
  sessions        INTEGER NOT NULL,         -- nombre de sessions distinctes où il apparaît
  classified      TEXT NOT NULL             -- 'user' | 'injection' (>= 5 sessions)
);
```

Aucune table d'envoi en attente : un segment est reconstruit depuis le fichier entre
`acked_end` et la dernière fin de ligne. La table `user_prefixes` est la seule donnée
dérivée du contenu que le démon conserve : des préfixes de 40 caractères, jamais un
bloc entier (révision 6, 3.3).

### 2.3 Segment

```json
{
  "file_key": "…", "session_id": "…", "kind": "main", "parent_session": null,
  "project_dir": "C--Users-Olivier-workspace-Kleos", "host_id": "…",
  "space": "kleos", "space_rule": "session_file", // 4.5 : session_file | marker | git | dir | none
  "epoch": 0, "epoch_reason": "initial", // révision 6 : initial | rewrite | view_upgrade (2.5, 2.8)
  "start": 1048576, "end": 1310720,
  "raw_sha256": "…",            // empreinte des octets BRUTS [start, end)
  "prev_tail_sha256": "…",      // = acked_tail_sha, chaîne le segment au précédent
  "wire_purge_version": 2,      // révision 6 : vue A+ + rédaction des secrets (1.4, 3.2, 3.3)
  "cc_version": "2.1.x",        // champ `version` des lignes Claude Code, pour l'alerte de 3.3
  "stats": { "user_text_chars": 0, "user_text_kept_chars": 0, "unparsable_lines": 0 },
  "records": [ { "offset": 1048576, "uuid": "…", "parent_uuid": "…", "type": "user",
                 "ts": "…", "cwd": "…", "role": "user", "text": "…",
                 "tool": null, "tool_result": null, "is_error": null,
                 "marker": null, "truncated": false } ]
}
```

Types de `marker` (révision 6, 3.3 et 3.4) : `compact_boundary`, `subagent_start`,
`subagent_end`, `{from_peer, sent_at}` pour un `<channel>` reçu, `{stored: id}` pour un
store Kleos de l'agent, `{sent_to: [peer…]}` sur un `send_message` dédupliqué,
`{unparsable: {len, sha256}}` pour une ligne complète illisible, `{truncated_at: n}`
sur un enregistrement coupé au plafond dur.

Règles :

1. `end` tombe toujours juste après un octet `\n`. Une ligne en cours d'écriture n'est
   jamais envoyée ni comptée. **DÉDUIT de `kleos-ingest/src/tailer.rs:74-119` :** le
   tailer actuel compte une ligne partielle lue en fin de fichier, échoue à la parser,
   puis avance l'offset au-delà : la fin de cette ligne, écrite plus tard, est relue
   comme un fragment illisible et l'enregistrement est perdu. La fréquence réelle est
   SUPPOSÉE faible (une ligne s'écrit probablement en un seul appel), mais le nouveau
   démon ne doit pas reproduire ce défaut.
   **Ligne complète illisible (révision 6).** Deux cas distincts, traités
   explicitement : (a) un **fragment** en fin de fichier, sans `\n` : jamais lu, le
   démon attend ; (b) une **ligne terminée par `\n` qui ne se parse pas** (JSON invalide,
   encodage cassé) : le démon émet un enregistrement `{marker: {unparsable: {len,
   sha256}}}` à son offset, l'inclut dans `stats.unparsable_lines`, et **avance** ; le
   fichier n'est jamais bloqué par une ligne. Le cas « fragment + ligne » (un fragment
   complété plus tard par le reste de la ligne) relève de (a) : rien n'est consommé
   avant le `\n`, donc la ligne est lue entière au passage suivant.
2. Taille bornée : au plus 256 Kio purgés ou 2 000 enregistrements par segment
   (paramètres, SUPPOSÉS suffisants). **Enregistrement énorme (révision 6) :** si un
   seul enregistrement A+ dépasse à lui seul 256 Kio (texte assistant très long,
   `send_message` volumineux), il part **seul dans son segment**, jusqu'à un plafond dur
   de 4 Mio (paramètre, SUPPOSÉ) ; au-delà, son texte est coupé à ce plafond avec
   `truncated: true` et `{marker: {truncated_at: n}}`. La route de réception du relais
   accepte 4 Mio (E8 : route neuve, limite propre). Dans aucun cas un enregistrement ne
   bloque le fichier.
3. Un seul segment en vol par fichier, au plus 4 fichiers en parallèle.
4. L'empreinte porte sur le brut : changer la purge ou la rédaction ne crée pas de
   conflit. C'est la règle qui rend la révision 6 (A+ au démon) compatible avec tout le
   reste de la section : la vue envoyée peut changer, le curseur et la chaîne des
   segments ne bougent pas.

### 2.4 Réponses du relais et idempotence

Clé d'idempotence côté relais : `(file_key, epoch, start)`. Toute requête sans jeton
valide reçoit `401` avant ce tableau (1.4).

| Cas reçu | Réponse | Action démon |
|---|---|---|
| `start == acked_end` du relais et `prev_tail_sha256` cohérent | `200 accepted` | `acked_end = end`, mise à jour de `acked_tail_sha` |
| Clé connue, même `raw_sha256` | `200 duplicate` | idem : renvoi après une coupure entre l'écriture relais et l'ack |
| Clé connue, `raw_sha256` différent | `409 conflict` | Vérifier la réécriture (2.5) |
| `start > acked_end` du relais | `409 gap {expected_start}` | Reculer à `expected_start` et renvoyer depuis le fichier |
| `start < acked_end` hors segment connu | `409 overlap {expected_start}` | Idem |
| Époque inférieure à l'époque connue | `409 stale_epoch {epoch}` | Adopter l'époque du relais |
| Époque supérieure, `epoch_reason = rewrite` | `200 accepted` ; l'époque précédente passe `superseded` (2.5) | `acked_end = end` |
| Époque supérieure, `epoch_reason = view_upgrade`, `wire_purge_version` **supérieur** à celui de l'époque connue | `200 accepted` ; l'époque précédente passe `superseded_by_view`, la lignée est marquée `reopen_for_view` (2.8) | `acked_end = end` |
| Époque supérieure, `epoch_reason = view_upgrade`, `wire_purge_version` **égal ou inférieur** | `409 same_view {wire_purge_version}` : un renvoi sans vue plus large n'a pas d'objet | Rester sur l'époque connue, consigner |
| Disque relais plein | `507` | Backoff, curseur inchangé |

`GET /v1/files/{file_key}/cursor` renvoie `{epoch, epoch_reason, wire_purge_version,
acked_end, tail_sha256}` : il reconstruit l'état du démon si sa base est perdue ou
réinstallée. `GET /v1/files/stale?purge_version_lt=N` (révision 6) liste les `file_key`
du poste appelant dont la dernière époque a été envoyée sous une version de purge
inférieure à N : c'est la liste de travail d'une montée de vue (2.8).

### 2.5 Réécriture détectée

Avant chaque lecture d'un fichier modifié, le démon compare :

- `size < acked_end` : troncature ;
- sha256 des 4 096 octets avant `acked_end` différent de `acked_tail_sha` : réécriture ;
- sha256 des 4 096 premiers octets différent de `head_sha` : fichier remplacé.

Dans les trois cas : `epoch += 1`, `epoch_reason = rewrite`, `acked_end = 0`, renvoi
complet sous la nouvelle époque. Le relais marque l'époque précédente `superseded` ; la
dédup par uuid (2.6) empêche de réextraire ce qui a déjà été traité. Coût : une lecture
de 4 Kio par fichier modifié (DÉDUIT du schéma). Une montée de vue (2.8) emprunte le
même chemin avec `epoch_reason = view_upgrade`, mais la dédup par uuid y joue à
l'inverse : elle **remplace** l'enregistrement connu au lieu de l'ignorer.

### 2.6 Dédup des historiques recopiés et lignées

**Contrainte F5 :** resume et fork recopient les lignes avec les mêmes uuid sous un
nouveau `sessionId`. La dédup se fait au relais, qui voit tous les fichiers et tous les
postes :

```sql
CREATE TABLE uuid_index (           -- conservé bien après le contenu (section 5)
  owner        TEXT NOT NULL,
  uuid         TEXT NOT NULL,
  file_key     TEXT NOT NULL,
  epoch        INTEGER NOT NULL,
  offset       INTEGER NOT NULL,
  session_id   TEXT NOT NULL,
  lineage_id   TEXT NOT NULL,       -- session racine de la lignée
  first_seen   INTEGER NOT NULL,
  purge_version INTEGER NOT NULL,   -- révision 6 : wire_purge_version de l'enregistrement conservé (2.8)
  PRIMARY KEY (owner, uuid)
);
CREATE TABLE lineage (
  session_id       TEXT PRIMARY KEY,
  lineage_id       TEXT NOT NULL,   -- = session_id pour une racine
  branch_uuid      TEXT,            -- dernier uuid recopié depuis la lignée
  last_segment_at  INTEGER NOT NULL -- horloge du relais, pas des timestamps (F4)
);
```

Un enregistrement dont l'uuid existe déjà sous un autre `file_key` reçoit `copy_of` et
sort de l'extraction ; la session rejoint la lignée de la première occurrence, avec
`branch_uuid` = dernier uuid recopié. **C'est ce lien qui permet à une reprise de
recharger l'état de sa session parente** (section 4.3). Pour les 29 % de lignes sans
uuid (F4), repli sur `sha256(type || ts || text)` dans la même lignée ; la part de ces
lignes qui survit à la purge est **SUPPOSÉE** faible (types techniques purgés), à
mesurer en lot 0.

### 2.7 Reprise après arrêt

1. Au démarrage puis toutes les 10 minutes : scan **par stat seulement** des 4 751
   fichiers (taille, mtime), sans lecture du contenu. Seuls les fichiers dont la taille
   dépasse `acked_end` sont lus. Cela borne le coût que 20d18577 redoutait d'un scan
   initial (DÉDUIT de l'append du 2026-10-21 sur cette carte).
2. Les événements `notify` accélèrent la détection ; ils ne sont pas la source de
   vérité.
3. Un arrêt brutal à n'importe quel point produit au pire un renvoi, absorbé par la clé
   d'idempotence.

### 2.8 Renvoi motivé : montée de vue (`view_upgrade`), révision 6

**Pourquoi.** En révision 2, le relais stockait B et dérivait A+ : élargir la vue
présentée au modèle était un réglage serveur, sans renvoi. En révision 6, le relais ne
détient que A+ : **élargir la vue demande de relire la source**. La source est le
fichier sur le poste, gardé 180 jours (F20), bien plus que les 14 jours du relais ; une
montée de vue est donc possible plus longtemps qu'avant, au prix d'un renvoi.

**Limite assumée (décision opérateur du 2026-10-22).** Le retraitement ne couvre que
les fichiers **encore sur le poste, poste allumé**. Un fichier supprimé par Claude Code
(au-delà de 180 jours, ou purge manuelle) ou un poste éteint pendant la montée ne sont
pas retraités : la lignée garde ses faits extraits sous l'ancienne vue. Le rapport du
relais liste les `file_key` non remontés.

**Déclenchement.** Une montée de vue est toujours **déclenchée par le poste**, jamais
par le relais : elle suit un déploiement du démon qui incrémente `wire_purge_version`
(nouvelle règle A+ de 3.3, nouveau format de rédaction de 1.4). Au démarrage, le démon
compare sa version à `meta.wire_purge_version` ; si elle est supérieure, il demande
`GET /v1/files/stale?purge_version_lt=<version>`, filtre les `file_key` dont le fichier
existe encore sur le disque, et pour chacun : `epoch += 1`, `epoch_reason =
view_upgrade`, `acked_end = 0`, renvoi complet sous la nouvelle époque, à raison d'au
plus 4 fichiers en parallèle (règle 3 de 2.3), après les fichiers actifs. Un
`409 same_view` arrête la montée pour ce fichier. Le relais ne redemande jamais un
fichier de lui-même : il répond seulement à la liste.

**Ce que le relais fait d'un segment `view_upgrade`.**

1. Les enregistrements dont l'uuid est déjà dans `uuid_index` sous le **même
   `file_key`** ne reçoivent pas `copy_of` : l'enregistrement A+ stocké est **remplacé**
   par la nouvelle forme, `uuid_index.purge_version` et `epoch` sont mis à jour. Un uuid
   connu sous un **autre** `file_key` (historique recopié, F5) garde sa règle de 2.6 :
   `copy_of`, hors extraction, parce que l'original sera lui-même remonté par son propre
   fichier s'il est encore sur le poste.
2. La lignée passe `reopen_for_view` : l'état de lignée et ses instantanés sont
   **recalculés depuis le début** sous la nouvelle vue (nouveau fenêtrage, nouvelle
   map, 4.2), parce qu'une vue plus large déplace les frontières de fenêtres. Les
   instantanés de l'ancienne vue sont conservés jusqu'à la stabilisation suivante, puis
   expirent avec le contenu (section 5). La correspondance `(lineage_id, id local) →
   (memory_id, version)` et le contenu publié sont conservés : ils sont l'entrée de la
   règle 3.
3. **Règle sur les faits déjà extraits.** Sans cette règle, la dédup par uuid de 2.5 et
   2.6 interdit toute réextraction, et sans elle une réextraction naïve publierait deux
   fois le même fait. À la stabilisation d'une lignée rouverte pour montée de vue, le
   relais compare l'état final recalculé à **ses propres publications de la lignée**
   (correspondance, contenu conservé localement, jamais une recherche sur tout
   l'espace) :

   | Fait de l'état final recalculé | Action | Fondement |
   |---|---|---|
   | Quasi identique à un fait publié de la lignée (même simhash à distance de Hamming inférieure à 3, ou mêmes `turn:<uuid>` sources et même statut) | **Rien** ; le fait reprend l'id local et le tag `fact:` de la publication existante, pour que l'idempotence de 4.3 tienne | Dédup à l'écriture, `mod.rs:586-616` (DÉDUIT) |
   | Reformulé ou complété, jugé `met_a_jour` ou `prolonge` d'un fait publié de la lignée par le juge amont (4.4, couche 1, avec la liste des publications de la lignée comme liste négative) | **Nouvelle version** par `parent_memory_id` = tête connue (4.3) ; `Conflict` traité comme en 4.3 | F12 |
   | `distinct` de tout fait publié de la lignée | **Fait nouveau** : détecteur de secrets (couche 0), juge amont (couche 1), publication ; id local au-delà du maximum précédent | 4.4 |
   | Fait publié de la lignée **absent** de l'état recalculé | **Aucun archivage** : une vue plus large ajoute de l'information, elle n'en invalide pas ; l'absence peut venir d'un nouveau fenêtrage. Le fait reste servi, consigné au rapport | Principe « un faux retrait est pire qu'un retrait manqué » (étude nightly) |
   | Fait publié de la lignée **annulé** par l'état recalculé (`annule #local`, ou toutes ses sources sur une branche abandonnée, 4.3) | Archivage réversible, comme en 4.3 | Q2 |

   Deux filets de plus contre le doublon : la dédup à l'écriture de Kleos absorbe un
   renvoi quasi identique sur la mémoire déjà publiée (même espace, Hamming inférieure
   à 3, DÉDUIT `mod.rs:586-616`), et la dédup de fond (4.8) traite une reformulation
   au-delà de 0,9. Au-dessous, deux mémoires de transcript peuvent coexister : la bande
   grise de 4.8 archive alors **la plus ancienne**, c'est-à-dire celle de l'ancienne
   vue, ce qui est le sens voulu.
4. Une montée de vue coûte un passage Ollama complet sur la lignée (3.2 : 50 à 90 min
   d'Ollama par jour de flux remonté, DÉDUIT de T2) et un passage du juge amont sur les
   seuls faits `distinct` ou `met_a_jour`. Elle se planifie donc comme un rattrapage
   (section 5), pas comme un flux : au plus N jours remontés par nuit (paramètre).

**Ce que cette règle ne couvre pas.** Une montée de vue sur une lignée dont le contenu
du relais a expiré mais dont la correspondance subsiste (section 5) suit la même table :
la correspondance suffit, le contenu publié est conservé sans limite avec elle.
Une montée de vue **plus étroite** (réduire A+) n'existe pas : `409 same_view` la refuse,
parce que rien ne la justifie et qu'elle ferait disparaître des faits sans les invalider.

---

## 3. Règle de purge

> **Révision 7.** La vue A+, la rédaction et le squelette sont calculés par
> `hypomnema-core` sur le poste (conception §4.3 à §4.6, §6), avec ces écarts : corps des
> `<channel>` toujours gardés à l'affichage mais retirés de la vue présentée au modèle
> quand l'original est connu (3.4 reste la règle d'extraction) ; `tool_result` de l'outil
> d'agent du parent gardé entier (F23) ; préfixes classés injection au seuil de 5
> sessions **et** à un ratio au plus 0,2 (F24) ; squelette `{uuid, parent_uuid,
> src_offset, type}` pour chaque ligne source. Les marqueurs `{stored: id}` restent dans
> la vue ; leur résolution par `GET /memory/{id}` passe au juge du consommateur (11.3),
> hypomnema ne lit jamais Kleos.

### 3.1 Ce que le serveur doit extraire

Décisions et leur raison, faits d'infrastructure (hôte, chemin, commande qui marche),
causes racines, contournements validés, résultats mesurés, préférences de l'opérateur.
Ces faits vivent surtout dans le texte user tapé et le texte assistant (F3). Les
résultats d'outils les **prouvent**, mais l'assistant les reformule presque toujours
(SUPPOSÉ, cohérent avec la discipline MESURÉ/DÉDUIT des sessions de ce dépôt).

### 3.2 Deux niveaux distincts : ce qui est stocké, ce qui est présenté au modèle

Définitions : **A** = purge complète sans outils (2,0 %, F3) ; **B** = purge complète
avec `tool_use`/`tool_result` (14,7 %, F3) ; **A+** = A plus les morceaux d'outils
décrits en 3.3 (cible au plus 3 % du brut, à mesurer).

**Révision 2 : l'idée du challenger est retenue.** Le démon envoie et le relais stocke
**B rédigé** ; le relais dérive **A+** au moment du traitement et ne présente que A+ au
modèle. Raisons :

- la purge présentée au modèle devient un paramètre serveur (`model_view_version`), qui
  évolue sans redéployer le démon ni renvoyer les fichiers ;
- un retraitement sur 14 jours peut élargir la vue (par exemple, une sortie d'erreur
  entière pour une cause racine) sans perte ;
- le coût réseau et disque reste faible : environ 44 Mo par jour, environ 620 Mo sur 14
  jours (DÉDUIT de F1 et F3).

Le prix est la surface de secrets côté serveur, traitée en 1.4.

**Révision 6 : la décision ci-dessus est remplacée. Le démon calcule la vue A+ et
n'envoie qu'elle ; le relais ne stocke plus B** (décision opérateur du 2026-10-22). Les
trois raisons de la révision 2 sont pesées à nouveau :

- la surface de secrets n'était pas théorique : F17 compte 3 clés `kleos_`, 4 valeurs
  `KLEOS_*_KEY`, 12 jetons GitHub et 51 URL à identifiants dans le corpus, surtout dans
  des sorties d'outils que B conserve en entier et que A+ réduit (MESURÉ rapporté,
  #32381) ; F19 montre que la rédaction prévue ne les voyait pas. Garder B au relais,
  c'est garder une copie de ces sorties sur une seconde machine pendant 14 jours, à
  chaque fois que la rédaction manque un format ;
- le retraitement « sans perte » de la révision 2 reposait sur B. Il est remplacé par un
  renvoi depuis la source (2.8), possible tant que le fichier est sur le poste : 180
  jours (F20), soit plus que les 14 jours du relais. Ce qui est perdu : le retraitement
  d'un fichier supprimé ou d'un poste éteint, limite assumée ;
- le coût réseau et disque baisse : au plus 9 Mo par jour (cible A+), environ 130 Mo
  sur 14 jours au lieu de 620 (DÉDUIT de F1 et F3).

Conséquences sur le reste de l'étude : `model_view_version` disparaît comme paramètre
serveur ; la vue est portée par `wire_purge_version` dans le segment (2.3), et un
changement de vue est un déploiement du démon suivi d'une montée de vue (2.8). Le
tableau des vues ci-dessous reste valide : il chiffre ce que le modèle voit, et le
modèle voit toujours A+. La ligne B y reste comme borne de ce qu'un élargissement
coûterait.

**Budget de calcul recalculé sur T2 (point 4 du challenge).** Une fenêtre de 36 000
caractères fait 9 084 tokens (T2), soit 3,96 caractères par token ; l'hypothèse de 3,5
de la révision 1 est remplacée. Fenêtre, état de session (au plus 2 000 tokens),
consignes (environ 1 000) et sortie (au plus 800) tiennent dans 16 384 tokens (DÉDUIT).
Durée par fenêtre, raisonnement coupé : **12 s à 22 s** selon la sortie (MESURÉ
rapporté, T2).

| Vue présentée au modèle | Volume par jour | Fenêtres par jour | Temps Ollama par jour, `think: false` (DÉDUIT de T2) | Ce qui est perdu |
|---|---|---|---|---|
| A | environ 6 Mo | environ 170 | 35 à 60 min | Commandes, sorties d'erreur, rapports de sous-agents, envois inter-agents, stores de l'agent |
| **A+ (recommandé)** | au plus 9 Mo (cible) | au plus 250 | **50 à 90 min**, plus environ 8 % de relances (M-N5) et environ 50 appels de réduction (687 sessions sur 14 jours, T3), soit **environ 1 à 2 h** | Contenu des lectures de fichiers et sorties volumineuses, qui restaient stockés en révision 2 et qui, depuis la révision 6, restent **sur le poste seulement** |
| B | environ 44 Mo | environ 1 220 | 4 à 7,5 h, plus relances et réductions | Presque rien |

**Conséquence sur la décision de la révision 1.** B présenté au modèle n'est plus
« impossible » (10 à 20 h estimées) : il coûte 4 à 8 h de GPU partagé par jour, soit
environ cinq fois A+. A+ reste recommandé pour la vue du modèle, parce que le gain de B
porte sur des contenus que l'assistant reformule déjà (3.1, SUPPOSÉ) ; le stockage en B
garde l'option ouverte. **Révision 6 :** l'option reste ouverte par un autre chemin, la
montée de vue depuis la source (2.8) ; elle n'est plus gratuite, puisqu'elle demande un
renvoi et un nouveau passage Ollama, et elle est bornée aux fichiers encore sur le
poste.

**Raisonnement coupé : obligatoire et vérifié.** Le relais n'hérite pas de
l'environnement de kleos-server (T2-a) : il porte `KLEOS_LLM_THINK=0` dans son propre
`EnvironmentFile` et coupe le raisonnement par `kleos_lib::llm::think_setting` et
l'injection associée (`kleos-lib/src/llm/mod.rs:45-50`, DÉDUIT), réutilisés par la
dépendance épinglée (1.3). Une réponse qui porte un contenu de raisonnement, ou dont
`done_reason` vaut `length`, est rejetée et la fenêtre passe en erreur ; elle n'est
jamais acceptée tronquée. Ce contrôle est le filet si la variable manque, puisque
`think_setting` rend alors `None` et n'injecte rien. Le drapeau entre dans
`contract_version`. SUPPOSÉ : le client LLM de kleos-lib ne fait pas lui-même ce
contrôle de `done_reason`, non relu.

**Limite du chiffrage.** 6,2 s d'ingestion à froid valent pour 9 084 tokens nouveaux ;
le préfixe de consignes est servi par le cache KV (T2), mais la fenêtre et l'état de
session sont nouveaux à chaque appel, donc l'ingestion à froid est la bonne base. La
mesure porte sur un appel isolé : la contention avec `dreamer-growth`, Broca et les
autres usages de l'hôte n'est pas mesurée. La porte du lot 4 la mesure sur une journée.

### 3.3 Contenu de la vue A+ (révision 6 : calculée côté démon, après rédaction)

Jusqu'à la révision 5, cette vue était dérivée côté serveur depuis B. Depuis la révision
6, **le démon la calcule** et c'est la seule forme envoyée ; la liste ci-dessous est
donc le contrat du fil, versionné par `wire_purge_version`. Ordre des passes dans le
démon : parse de la ligne, rédaction des secrets sur l'enregistrement entier (1.4),
puis réduction A+. La rédaction précède la réduction pour qu'un extrait de 200
caractères ne porte jamais le début d'une sortie `env`.

Gardé :

- texte user **tapé**, défini **par la structure** et non par une liste de motifs à
  retirer (révision 6, amendement b du challenger ; F3 mesure 73 % de texte user
  injecté, F18 en nomme les formes) : est du texte user tapé un bloc `text` d'une ligne
  de type `user`, **sans `isMeta`**, **hors `tool_result`**, qui ne commence par aucune
  balise de harnais (`<system-reminder>`, `<channel …>`, `<task-notification>`,
  `<local-command-*>`, `<command-*>`, bloc `MANDATORY RULES`, préambule de reprise après
  compaction) et dont le préfixe de 40 caractères n'est pas classé injection (ci-dessous).
  **Échec fermé** : un bloc qui ne satisfait pas toutes les conditions est traité comme
  harnais et retiré ; un doute retire, il ne garde pas. Sont classés harnais par leur
  forme, au-delà des balises : le corps de skill (« Base directory for this skill »,
  2 673 occurrences en lignes `user`, F18), les blocs `=== … ===` du hook de démarrage
  (`=== RECENT MEMORIES ===` : 37 en lignes `user`, 7 948 en `attachment`, F18), et les
  refus de hook dans les `tool_result` (`PreToolUse:… hook error`, 6 285, F18), qui ne
  sont pas du texte user mais sortent aussi du filtre des lignes d'erreur ci-dessous
  (un refus de hook n'est pas une erreur de la commande) ;
- **préfixes répétés** : le démon tient la table `user_prefixes` (2.2) des 40 premiers
  caractères normalisés (espaces réduits, casse pliée) de chaque bloc `text` user ; tout
  préfixe vu dans **au moins 5 sessions distinctes** est classé `injection` et ses blocs
  sont retirés, y compris rétroactivement à la prochaine montée de vue (2.8). Un humain
  retape rarement 40 caractères identiques dans 5 sessions ; un hook le fait à chaque
  session. Seuils SUPPOSÉS, calibrés au lot 0 (i) ; la liste des préfixes classés est
  remontée au relais dans le rapport, jamais les blocs ;
- **alerte de dérive** : le segment porte `cc_version` (champ `version` des lignes
  Claude Code) et `stats.user_text_chars` / `stats.user_text_kept_chars` (2.3). Le
  relais agrège la part gardée par version de Claude Code ; si, sur les 100 premières
  sessions d'une nouvelle version, elle **double ou tombe à la moitié** de la version
  précédente, le rapport quotidien lève une alerte : la structure des lignes a
  probablement changé et le filtre, en échec fermé, retire ou garde trop. Seuil
  SUPPOSÉ ;
- texte assistant ;
- `tool_use` réduit : nom de l'outil et arguments clés (commande Bash, chemin, motif,
  description), tronqués à 300 caractères ;
- `tool_result` réduit : `is_error`, les 200 premiers caractères, et les lignes qui
  contiennent `error`, `failed`, `panic` ou un code de sortie non nul, dans la limite de
  600 caractères. **Révision 6** : le filtre des lignes d'erreur reconnaît aussi les
  erreurs Windows en français (« n'est pas reconnu », « Accès refusé », « introuvable »),
  « Caused by: », les codes HTTP 4xx et 5xx en tête de ligne ou après `HTTP/`, et le
  résumé final d'une suite de tests (`test result:`, `Tests:`, `passed`, `failed`,
  `ok`, `FAILED`, comptes `N passed, M failed`), parce qu'un résultat mesuré est un fait
  (3.1) ;
- **gardés en entier**, parce qu'ils sont du texte écrit par un agent : l'argument
  `message` de `send_message` (claude-peers) et le résultat final d'un outil de
  sous-agent. **Révision 6 (amendement c) :** un `send_message` dont le `message` est
  identique (même sha256) à un envoi précédent de la **même session** est dédupliqué :
  le premier est gardé en entier, les suivants deviennent `{marker: {sent_to:
  [peer…]}}` sur le premier. Le démon tient cette dédup en mémoire par fichier ouvert,
  sans table ;
- **store Kleos de l'agent** (`kleos-cli store …`, `memory_store`) : en révisions 2 à 5,
  gardé en entier. **Révision 6 (amendement c) : remplacé par le marqueur `{stored:
  id}`**, où `id` est lu dans la sortie de l'outil (`kleos-cli store` imprime l'id ;
  `memory_store` le rend dans son résultat, formes SUPPOSÉES, fixtures au lot 1). Le
  fait est déjà écrit là où l'agent l'a écrit, dans Kleos ; le marqueur sert de **liste
  négative** au juge (4.2, 4.6), que le relais résout par `GET /memory/{id}` (route
  SUPPOSÉE, `routes/memory/mod.rs:40-63` non relu pour ce point). Si aucun id n'est
  lisible (store en échec, `created=false` sans `duplicate_of`), le marqueur vaut
  `{stored: null}` et l'argument est réduit comme tout `tool_use`, à 300 caractères ;
- marqueurs : `compact_boundary`, début et fin de sous-agent, `{from_peer, sent_at}`
  pour un message reçu (3.4), `{stored: id}`, `{sent_to: […]}`, `{unparsable: …}`,
  `{truncated_at: n}` (2.3).

Retiré dès le démon (jamais envoyé) : `thinking`, `attachment`, `api-request*`, doublon
`toolUseResult`, images, enveloppe des lignes `message`, ligne `isCompactSummary`
(dérivée des tours déjà transmis ; F5), et, depuis la révision 6, le reste des
`tool_result` et des arguments d'outils, qui n'existent plus qu'à la source. La phrase
« retiré par la vue A+ seulement (stocké en B) » des révisions 2 à 5 n'a plus d'objet.

**Branches abandonnées par `/rewind` (révision 6, point secondaire).** Un `/rewind`
laisse dans le fichier les lignes de la branche abandonnée et poursuit avec un
`parentUuid` qui pointe avant elles (SUPPOSÉ sur la forme exacte, à confirmer au lot 0
(j) ; la mécanique `parentUuid` est MESURÉE rapportée, F4 et F5). Le démon envoie tout
(le curseur ne saute jamais), avec `parent_uuid` sur chaque enregistrement. Le relais
reconstruit, à chaque map et à la stabilisation, la **chaîne des parents depuis la
dernière feuille** (dernier enregistrement par offset) ; un enregistrement hors de cette
chaîne est `abandoned`. Un fait dont **toutes** les sources `turn:<uuid>` sont
`abandoned` reçoit `annule` à la réduction (4.3) et n'est jamais publié comme valide ;
s'il l'était déjà (rewind après stabilisation, cas d'une reprise), il suit la ligne
« fait annulé » de 4.3. Un fait dont une source au moins est sur la chaîne reste valide.

### 3.4 Statut des `<channel>` et des sous-agents

- **`<channel source="claude-peers">` reçu : retiré de la vue A+, remplacé par un
  marqueur** `{from_peer, sent_at}`. Le message est une **copie** : son original est
  l'argument `send_message` de la session émettrice, gardé en entier. Règle générale :
  **extraire un fait là où il est écrit, jamais là où il est recopié.** Cela évite qu'un
  brief de team-lead produise N mémoires dans N sessions réceptrices. Le marqueur
  compte : la moitié des reprises après un long écart sont déclenchées par un message
  injecté (T3), et le tour qui suit doit rester rattaché à sa cause.
  **Révision 6 (amendement c) : un `<channel>` dont l'original n'est pas transmis reste
  en entier.** C'est le cas d'un émetteur `operator` (message tapé dans le Deck, aucune
  session Claude Code ne le porte), du Deck lui-même, ou d'un pair dont le poste n'a pas
  de démon (hors parc). Le démon ne connaît pas le parc : il applique une règle locale
  et laisse l'arbitrage au relais. Règle locale : `from_peer_id = operator` → bloc gardé
  en entier avec son marqueur ; tout autre émetteur → marqueur **et corps** envoyés,
  le corps portant `channel_body: true`. Au relais, un index `sent_index(owner,
  sha256(message), sender_host, session_id, sent_at)` est alimenté par chaque
  `send_message` reçu en A+. À la **stabilisation** de la lignée réceptrice (8 h après
  son dernier segment, 4.1, donc bien après l'arrivée du segment émetteur si le poste
  émetteur a un démon), le relais cherche l'original par `sha256(message)` : trouvé, le
  corps est retiré de la vue présentée au modèle et seul le marqueur reste ; non
  trouvé, le corps reste en entier et le fait s'extrait dans la session réceptrice,
  faute d'autre endroit où il soit écrit. Le corps est donc stocké au relais 14 jours
  dans tous les cas, ce qui est la surface de B pour ces seuls blocs : acceptée, parce
  qu'un message de pair est du texte d'agent, pas une sortie d'outil (F17 situe les
  secrets dans les sorties d'outils). Décision prise dans cette révision sans mesure du
  volume de ces blocs ; lot 0 (k) le compte.
- **Sous-agents : non envoyés en v1** (révisions 1 à 5). Leur rapport final arrive dans
  le `tool_result` de l'outil d'agent du parent, gardé en entier par A+. **SUPPOSÉ** :
  que le rapport soit bien dans ce `tool_result` ; à vérifier sur 20 sessions en lot 0
  (geste opérateur, F9). Si la vérification échoue, les fichiers `subagents/` passent
  par le même protocole avec `kind = 'subagent'`, et seul leur dernier message assistant
  est extrait.
  **Révision 6 (amendement c) : les fichiers `subagents/` sont envoyés dès la v1**, par
  le même protocole, avec `kind = 'subagent'` et `parent_session`, sans attendre la
  mesure du lot 0 (d), qui devient informative. Vue A+ d'un sous-agent, plus étroite que
  celle d'une session principale : **au moins** son dernier message assistant (le
  rapport, là où il est écrit) et ses `tool_use` Bash **réussis** (`is_error` faux :
  la commande qui marche est un fait de 3.1), avec les 200 premiers caractères de leur
  résultat ; le reste de ses tours est retiré. Le `tool_result` du parent qui recopie le
  rapport devient alors une **copie** : il est réduit comme tout `tool_result`, et le
  marqueur `subagent_end` du parent renvoie à la session du sous-agent. Si un rapport
  n'est pas dans le dernier message assistant du sous-agent (lot 0 (d) le dit), la vue
  du sous-agent s'élargit par une montée de vue (2.8), sans toucher au protocole. Coût :
  les sous-agents font 10 % du stock (F1) et leur vue est plus étroite ; volume SUPPOSÉ
  sous 1 Mo par jour, mesuré au lot 0 (b).

---

## 4. Traitement côté serveur

> **Révision 7.** Partage des sous-sections :
> - **passent à hypomnema** (conception §7.4) : 4.1 (déclencheurs du map, stabilisation à
>   8 h), 4.2 (fenêtres, état de lignée, sortie JSON ; taille de fenêtre calculée par le
>   tokenizer du modèle branché, LiteLLM ou Ollama), et les points 1 et 2 de 4.3 (état de
>   lignée hérité, publication à la stabilisation). Le préfixe forké n'est jamais remappé :
>   le map d'une copie part de l'instantané de la lignée parente ;
> - **restent à Kleos** (consommateur, section 11) : le point 3 de 4.3 (révision des faits
>   publiés par la chaîne de versions, correspondance, idempotence), 4.4 (couches 0 à 5),
>   4.5 (typage ; le space vient désormais d'une étiquette, 11.3), 4.6 (juge nightly), 4.7
>   et 4.8 (dédup de fond).
> Les « tours sources » que le juge amont joint à un fait (4.4, couche 1) se lisent par
> `GET /v1/sessions/{id}/read?around=<uuid>` sur hypomnema, et non plus dans la base du
> relais. La limite nommée au point 1 de 4.3 (vraie branche jamais observée) est levée
> par F21 : les branches sœurs sont fréquentes, voir 11.4.

### 4.1 Quand

| Déclencheur | Rôle |
|---|---|
| Fenêtre complète (36 000 caractères de vue A+) ou session sans nouveau segment depuis 45 min | **Map** de la fenêtre : met à jour l'état de lignée. **Ne publie rien** (révision 2) |
| Lignée sans nouveau segment depuis 8 h (`T_settle`, paramètre) | **Stabilisation** : réduction et publication des faits de l'état de lignée (4.3) |
| Nouveau segment sur une lignée stabilisée (reprise tardive, fork, resume) | Réouverture : map des nouveaux tours sur l'état hérité, puis révision des faits déjà publiés à la prochaine stabilisation (4.3) |
| Indice de fin optionnel (hook `SessionEnd` vers le démon, puis le relais) | Information seulement : **ne déclenche pas la publication**, parce qu'une reprise reste possible (T3-a) |
| File bornée, concurrence 1 vers Ollama | Remplace la garde du dreamer |

**La garde `dream_idle_threshold_secs` ne s'applique pas telle quelle en option R** :
elle mesure l'inactivité HTTP de kleos-server (`kleos-server/src/dreamer.rs:193-206`,
DÉDUIT par l'étude nightly 4.4, non relu ici), qu'un processus séparé ne voit pas. La
contention réelle porte sur l'hôte Ollama, partagé. Équivalent proposé : concurrence 1,
et report d'une fenêtre si l'hôte sert déjà un autre modèle (endpoint `/api/ps`
d'Ollama, SUPPOSÉ disponible sur la version déployée, vérifié au lot 0 (g)).

### 4.2 Comment : fenêtres de 16k et état de lignée

Une session de 58 Mo réduite en vue A+ fait environ 1,2 à 1,7 Mo (DÉDUIT de F3), soit
35 à 50 fenêtres. Trois étages :

1. **Fenêtrage** : découpe aux frontières de tour (prompt user tapé jusqu'à la dernière
   réponse assistant avant le prompt suivant), environ 36 000 caractères. Un tour trop
   long est coupé à une frontière d'enregistrement, jamais au milieu d'un texte.
2. **Map, une fenêtre par appel.** Entrée : consignes, **état de lignée** reporté (au
   plus 2 000 tokens : résumé courant et faits courants avec leur id local), la liste des
   stores Kleos faits par l'agent dans la fenêtre (révision 6 : les marqueurs `{stored:
   id}` de la fenêtre, résolus en contenu par le relais par `GET /memory/{id}`, 3.3 ;
   un id introuvable est présenté comme « stocké, contenu inconnu »), puis la fenêtre.
   Sortie JSON sous
   schéma imposé : résumé de fenêtre (au plus 1 500 caractères) ; faits nouveaux avec
   `statut ∈ {proposé, décidé, mesuré, abandonné}`, catégorie, uuid des tours sources ;
   révisions de faits courants (`révise #local`, `annule #local`). Un instantané de
   l'état est gardé après chaque fenêtre, indexé par le dernier uuid de la fenêtre.
3. **Réduction à la stabilisation** : l'état final de lignée, construit en appliquant
   les révisions dans l'ordre des offsets (jamais des timestamps, F4). Si les résumés de
   fenêtre dépassent une fenêtre, réduction en deux niveaux.

**Modèle :** Qwen pour map et réduction (contexte de 16k suffisant par construction),
`think: false` vérifié (3.2). **Limite connue :** M-N5 mesure 92,3 % de JSON valide pour
Qwen sous schéma imposé (MESURÉ rapporté, étude nightly, section 8) : validation et
relance bornée sont obligatoires, et une fenêtre en échec n'est jamais marquée faite. La
qualité de l'extraction par Qwen n'est **pas mesurée** : la porte du lot 4 décide, et
l'escalade d'une fenêtre vers Sonnet reste une option de configuration, chiffrée à la
même porte.

**Version du contrat :** chaque fenêtre porte `(modèle, prompt, think, model_view_version,
contract_version)` ; un changement rouvre le travail encore en rétention, comme l'état
de traitement de l'étude nightly 4.2. **Révision 6 :** `model_view_version` est
remplacé par le `wire_purge_version` du segment (la vue est calculée au démon, 3.2) ;
un changement de modèle, de prompt ou de `think` rouvre les lignées encore en rétention
sans renvoi, puisque le relais détient A+ ; un changement de vue passe par une montée de
vue depuis le poste (2.8). Les prompts passent par le canal
`prompts-overrides` : le relais appelle `kleos_lib::llm::prompts::load_prompt`
(`kleos-lib/src/llm/prompts.rs:97`, DÉDUIT), avec `KLEOS_LLM_PROMPT_REPOSITORY` pointé
sur le clone `/var/lib/kleos/prompts` de LXC 121 ; les ids du relais vivent sous un
préfixe propre (`transcript/…`), et le texte embarqué par défaut vit dans le dépôt du
relais.

### 4.3 Finalisation tolérante à la reprise (point 2 du challenge)

**Ce qui tombe.** La révision 1 publiait l'état d'une session au point de contrôle de
45 min et considérait comme stable un fait « non révisé ensuite ». T3 mesure que 28,97 %
des sessions reprennent après un tel écart, et un `--resume` ouvrait un nouvel état vide
au relais : un fait publié pouvait être contredit dans la reprise sans que rien ne le
révise. Le mécanisme ci-dessous combine les trois leviers demandés.

**1. Supersession par lignée : la reprise hérite de l'état.** L'état de travail n'est
plus par session mais par **lignée** (2.6). Une session dont les premiers
enregistrements sont des copies d'une lignée L démarre avec l'instantané de L pris à
`branch_uuid`, puis ses propres tours s'y appliquent. Les ids locaux des faits restent
valides d'une session à l'autre, donc `révise #12` dans la reprise vise bien le fait 12
de la session parente. T3-a mesure qu'aucune source n'a continué après un fork : en
pratique, la lignée est une suite linéaire de fichiers (MESURÉ rapporté).
**Limite nommée :** une vraie branche, où la source et la copie continuent toutes deux,
n'a pas été observée ; si elle survient, les deux branches partent du même instantané et
publient chacune ; un fait révisé dans une branche seulement crée deux têtes, et la
seconde publication rencontre le refus de la chaîne de versions (point 3 ci-dessous).

**2. Publication à la stabilisation, pas à l'inactivité courte.** Les maps tournent au
fil de l'eau ; la publication attend `T_settle = 8 h` sans nouveau segment dans la
lignée. Avec ce seuil, 0,73 % des sessions reprennent encore après publication, contre
28,97 % à 45 min (MESURÉ rapporté, T3). Un seuil de 2 h laisserait 7,71 %. Le coût est
un délai : un fait d'une session du matin devient mémoire le soir, ce qui convient à un
traitement de nuit. Le cas résiduel passe par le point 3.

**3. Révision des faits déjà publiés : chaîne de versions de Kleos.** Le relais garde la
correspondance `(lineage_id, id local) → (memory_id, version)`. À la stabilisation
suivante d'une lignée rouverte, il compare l'état final à ce qu'il a publié :

| Changement | Action du relais | Fondement |
|---|---|---|
| Fait nouveau | Passage par le juge amont (4.4, couche 1), puis `POST /memory`, `source=kleos-transcript` | F11 |
| Fait révisé, mémoire publiée | Juge amont sur la version révisée, puis `POST /memory` avec `parent_memory_id` = tête connue : nouvelle version, l'ancienne passe à `is_latest = 0`, la dédup est sautée | DÉDUIT (F12, `mod.rs:578`, `:873-890`) |
| Fait révisé, mais la tête a changé (une autre écriture a versionné la mémoire) | Kleos refuse (`Conflict`) ; le relais publie le fait révisé comme mémoire neuve, après le juge amont, et le juge nightly annote la relation (4.6) | DÉDUIT (`mod.rs:885-889`) |
| Fait annulé, mémoire publiée | **Proposition soumise à l'opérateur (Q2)** : archivage par `POST /memory/{id}/archive`, réversible par `POST /memory/{id}/unarchive`, seulement sur une mémoire que le relais a écrite et que personne n'a versionnée depuis | Routes DÉDUITES (`routes/memory/mod.rs:62-63`) |
| Fait dont toutes les sources `turn:<uuid>` sont sur une branche abandonnée par `/rewind` (révision 6, 3.3) | Reçoit `annule` à la réduction ; jamais publié comme valide ; s'il l'était déjà, ligne « fait annulé » ci-dessus | Chaîne des parents depuis la dernière feuille (3.3) |
| Lignée rouverte pour montée de vue (`view_upgrade`, révision 6) | Table de 2.8 : rien pour un fait identique, version pour un fait révisé, publication pour un fait distinct, **aucun archivage** pour un fait absent de l'état recalculé | 2.8 |

Règle d'autorité : **le relais ne révise, n'annule ou n'archive que ses propres
publications**, identifiées par sa correspondance et par `source=kleos-transcript`.
Jamais une mémoire d'agent.

**Idempotence des publications.** Chaque publication est inscrite dans la
correspondance avec l'état `intention` avant l'appel, puis l'id rendu. Les mémoires
portent le tag `fact:<lineage8>-<id local>`. Après une coupure entre l'appel et
l'inscription, le relais cherche ses propres mémoires par ce tag avant de republier
(le filtre `tags` de la recherche existe, `search.rs:1543-1575`, DÉDUIT ; son
exposition par `POST /search` est SUPPOSÉE, porte du lot 5). En dernier recours, un
renvoi identique est absorbé par la dédup à l'écriture sur la mémoire déjà publiée :
même espace, distance de Hamming inférieure à 3, rien de nouveau écrit (DÉDUIT,
`mod.rs:586-616`).

**Ce qui devient une mémoire :** un fait de l'état final de lignée, au statut `décidé`
ou `mesuré`, ni annulé ni révisé ensuite, absent des stores de l'agent (liste vue en
fenêtre), et **accepté par le juge amont** (4.4). Un fait `proposé` ou `abandonné`, ou
refusé par le juge amont, reste dans le relais.
**C'est la réponse à l'inquiétude de l'opérateur :** une décision contredite trois tours
plus tard, ou dans une reprise, ne sort pas de l'état de lignée ; et si elle est déjà
sortie, sa révision suit.

### 4.4 Protection sans porte contre les faits faux

**Le problème que la porte réglait reste entier.** La porte de revue ne sera jamais
activée (F13). Toute mémoire publiée par le relais est donc servie aux agents dès son
écriture. Or l'extraction par Qwen n'est pas mesurée (4.2), et M-N5 mesure 7,7 % de
JSON invalide avant relance (MESURÉ rapporté, étude nightly, section 8). Un fait faux, mal attribué ou périmé peut sortir.
L'option (c) de l'ancienne carte 6ee2297f devient la seule voie, et l'objection qu'on
lui faisait (« le relais devient seul juge avant publication ») devient le principe de
la protection.

**Pistes évaluées.**

| Piste | Ce qu'elle protège | Fondement | Delta upstream | Verdict |
|---|---|---|---|---|
| Importance plafonnée | Un fait de transcript n'est jamais poussé sans requête ; il ne sort que si une recherche le juge pertinent | F14 : tier important de `/recall` à partir de 7, statiques de `/context` à partir de 9, insights à partir de 8 | Nul | **Retenue (couche 2)**, mais elle ne retire pas un fait faux d'une recherche pertinente |
| Source et tags distincts, filtre de lecture existant | Provenance visible dans la réponse JSON (`source`, `kleos-server/src/routes/memory/mod.rs:88`) ; listage exact des mémoires de transcript par `source_filter` | F15 | Nul pour le marquage | **Retenue (couche 3)**. Mais le seul filtre existant est positif : **exclure** les transcripts d'une lecture demanderait un champ neuf dans `SearchRequest`, `search.rs` et les routes, soit au moins trois fichiers upstream, et chaque lecteur devrait s'y abonner. Écarté |
| Catégorie de bruit (`activity`, `growth`) | Exclue de la recherche par défaut | `search.rs:1101-1107` | Nul | **Écartée** : détournement de sens, et un fait caché partout équivaut à la porte refusée |
| Publication seulement après le juge nightly | Le juge voit le fait avant qu'il soit servi | Le juge nightly a pour racines des mémoires déjà dans Kleos (étude nightly 4.6) : sans statut caché, un fait présent dans Kleos est déjà servi | Exigerait un statut caché, c'est-à-dire la porte | **Transformée** : le juge tourne en amont, dans le relais (couche 1) |
| Juge qui rétrograde ou retire a posteriori par la chaîne de versions | Un fait faux déjà servi en sort au passage nightly suivant | Archivage et désarchivage par l'API (`routes/memory/mod.rs:62-63`) ; mise à jour en nouvelle version (`:59`, `kleos-lib/src/memory/mod.rs:1821-1839`) | Nul | **Retenue (couche 4)**, bornée aux mémoires de transcript |

**Protection retenue : cinq couches, zéro delta upstream, plus une couche 0 depuis la
révision 6.** Contre le fait faux, la couche principale est la 1 ; contre le doublon
« transcript publié avant l'agent », c'est la 5 (section 4.8), à la demande de
l'opérateur ; contre le secret publié, c'est la couche 0.

0. **Détecteur de secrets, avant le juge (révision 6, 1.4).** Chaque fait candidat de
   l'état final de lignée passe par les passes de rédaction partagées avec le démon
   (formats, noms de variables, lexique, entropie). Un fait qui déclenche le détecteur
   **n'est pas publié**, quel que soit le verdict du juge ; il reste au relais avec le
   type détecté et compte au rapport quotidien. Échec fermé : si le détecteur ne répond
   pas (crate indisponible, version de patterns inconnue), rien n'est publié. Raison
   d'une couche distincte du juge : le juge est un modèle, qui peut laisser passer ; le
   détecteur est déterministe, et une mémoire n'expire pas. Les publications
   antérieures à l'activation d'un nouveau format ne sont pas rebalayées par cette
   couche ; un rebalayage ponctuel par `source_filter = kleos-transcript` (F15) est un
   geste d'exploitation, consigné en Q6.
1. **Juge amont dans le relais, avant toute publication.** Le juge est **Claude Haiku**,
   et non Qwen : la confidentialité n'est plus un frein, puisque les transcripts sont
   déjà des échanges avec des modèles cloud (décision opérateur, Kleos #31730), et un
   juge d'une autre famille que l'extracteur ne partage pas ses biais d'extraction
   (SUPPOSÉ, à mesurer au lot 4). Pour chaque fait candidat de l'état final de lignée,
   le relais :
   - récupère ses voisins par `POST /search` : même propriétaire, même espace, environ 8
     résultats hybrides (F11 pour l'API ; taille SUPPOSÉE, calibrée au lot 4) ;
   - joint au fait les tours sources du transcript, extraits de la vue A+ par leurs uuid ;
   - appelle le juge avec le prompt du juge nightly, partagé par `load_prompt` (1.3),
     plus une question de **fidélité** : le fait est-il affirmé par les tours sources,
     sans ajout ?

   Effets :
   - fidélité `infidèle` ou `invérifiable` : le fait n'est pas publié ;
   - `doublon` d'une mémoire servie : non publié ;
   - `contredit` une mémoire servie : non publié, conflit gardé dans le relais et
     réévalué automatiquement si la mémoire contradictoire est archivée ou versionnée ;
     le rapport quotidien du relais est informatif, aucune action humaine n'est attendue
     (4.7) ;
   - `distinct`, `prolonge`, `met_a_jour`, `partiel` : publié, et la relation est
     transmise en tag pour l'annotation nightly.

   **C'est la couche qui répond au risque de fait faux**, les trois autres en bornent
   l'effet résiduel.
2. **Importance plafonnée à 5**, au lieu de 6 en révision 3. Le fait reste sous les trois
   seuils de F14, et sous l'importance minimale des stores d'agent : 6 par la convention
   `-i 6-9` des sessions, SUPPOSÉ que tous les agents la suivent. Avec F16, la dédup de
   fond garde alors toujours la copie de l'agent face à une copie de transcript ; à
   importance égale, elle garderait l'id le plus haut, souvent celui du transcript
   publié après coup (DÉDUIT de F16). `is_static` n'est jamais posé.
3. **Provenance** : `source=kleos-transcript`, tags `transcript`,
   `turn:<uuid>` et `fact:<lineage8>-<id local>`. La provenance est dans la donnée et
   dans la réponse JSON (F15). **Limite nommée** : un rendu qui n'affiche que la
   catégorie, comme le bloc injecté par les hooks de session (`[decision] [local]`),
   ne montre pas la source à l'agent qui lit (SUPPOSÉ, observé sur le rendu de
   kleos-cache, non vérifié dans son code ; question Q2).
4. **Juge nightly a posteriori**, avec une autorité bornée aux mémoires de transcript
   (section 4.6).
5. **Dédup de fond**, couche principale contre le doublon publié avant l'agent : niveau
   déterministe existant (Patch 60) et bande grise jugée par Sonnet dans le relais
   (section 4.8).

**Ordre des écritures : dans quel cas le transcript passe avant l'agent.** La
publication attend 8 h sans nouveau segment dans la lignée (4.3). Un store fait par
l'agent pendant la session précède donc toujours la publication du fait de transcript
correspondant. Le juge amont l'écarte alors en `doublon`, et s'il passe, la dédup à
l'écriture absorbe la copie de transcript sur la mémoire de l'agent (`mod.rs:586-616`) :
c'est le bon sens (DÉDUIT de 4.3 et du code ; l'observation du lead est juste). **Le
mauvais ordre n'arrive que si un agent stocke le même fait plus de 8 h après le dernier
segment de la lignée** : une autre session, ou une reprise tardive (0,73 % des sessions,
T3).

**Résiduel nommé, réécrit.** Dans ce mauvais ordre, deux cas :

- **Texte quasi identique** (distance de Hamming de simhash inférieure à 3) : le store de
  l'agent est absorbé par la ligne de transcript (`created=false`, `duplicate_of`), car
  la dédup à l'écriture ne distingue pas les sources et son chemin doublon ne relève
  rien (`mod.rs:586-616`, `:607-616`). Le fait reste servi, mais à l'importance 5 et
  sous la source du transcript. **Aucune couche ne le rattrape**, puisqu'aucune seconde
  ligne n'existe. Fréquence SUPPOSÉE faible : il faut un agent qui réécrit, plus de 8 h
  après, un texte presque mot pour mot identique à l'extraction de Qwen. Le lot 5 la
  mesure ; un correctif chirurgical (relever l'importance au maximum des deux dans le
  chemin doublon) ne se rouvre que si elle n'est pas marginale.
- **Reformulation** : deux lignes coexistent ; la couche 5 garde l'agent (4.8).

**Coût des juges, SUPPOSÉ.**

| Juge | Modèle | Volume par jour ouvré | Coût unitaire | Coût par jour |
|---|---|---|---|---|
| Juge amont, couche 1 | Haiku | 30 à 60 candidats | environ 0,0003 USD (environ 2 000 tokens d'entrée, 150 de sortie ; tarif Haiku 5.5 de 0,10 et 0,50 USD par million de tokens, vérifié sur la page tarifaire, Kleos #30268) | environ 0,01 à 0,02 USD |
| Bande grise, couche 5 | Sonnet (4.8) | 5 à 30 paires (SUPPOSÉ, mesuré au lot 5) | environ 0,004 USD (environ 1 500 tokens d'entrée, 100 de sortie ; tarif Sonnet 5.5 de 2 et 10 USD par million de tokens, même source), hors tokens de raisonnement | moins de 0,12 USD |
| Juge nightly, couche 4 | celui de l'étude nightly (Sonnet) | 30 mémoires publiées | 0,0151 USD par groupe (M-N4, MESURÉ rapporté) | environ 0,45 USD |
| Juge de mesure des lots 4 et 5 | Sonnet, tirage de 40, ponctuel | | idem | environ 0,60 USD par mesure |

Total courant : environ 0,5 à 0,6 USD par jour, dominé par le juge nightly, sous le
budget D5 d'environ 3,5 USD par nuit (Kleos #30326) et dans les 100 USD mensuels d'API
inclus à l'abonnement (#31729) : environ 18 USD par mois au plus haut (DÉDUIT des lignes
ci-dessus). Le client LLM de
kleos-lib est SUPPOSÉ capable d'appeler l'API Anthropic, sinon le relais ajoute ce
client. Le volume réel sort des portes des lots 4 et 5.

### 4.5 Typage des mémoires issues des transcripts

Réponse à la question de l'opérateur : **oui, un typage distinct par la source, mais
la même taxonomie de catégories que les agents.**

| Champ | Valeur | Raison |
|---|---|---|
| `source` | `kleos-transcript` (nouvelle) | Distincte de `claude-session` (agents) et de `kleos-ingest` (ancien démon) ; clé de l'autorité de révision et d'archivage (4.3, 4.6), du filtre positif `source_filter` (F15), du budget et des mesures par source (M-N2) |
| `category` | Ensemble des agents : `decision`, `discovery`, `state`, `issue`, `task`, `reference`, `general` | Le juge compare des mémoires de même vocabulaire. Les catégories lexicales `infrastructure` et `session-summary` disparaissent (DÉDUIT de `kleos-ingest/src/writer.rs:198`, `extractor.rs:84`) |
| `importance` | Proposée par le modèle, **plafonnée à 5** | Sous les seuils des tiers servis sans requête (F14) et sous les stores d'agent, pour que la dédup de fond (Patch 60, F16) garde la copie de l'agent (4.4, couche 2) |
| `created_at` | Timestamp du tour source | Champ accepté par l'API (F11) ; l'ordre temporel des voisinages reste juste malgré un traitement différé de plusieurs heures |
| `session_id`, `tags` | Session source ; `transcript`, projet, `turn:<uuid>`, `fact:<lineage8>-<id local>` | Provenance pour la revue, pour l'annotation du juge et pour l'idempotence (4.3) |
| `space` | Résolu **par session, côté démon poste**, selon la règle ci-dessous, et transmis dans le segment | D1 : jamais d'espace croisé ; la dédup à l'écriture (`mod.rs:571-572`) et la dédup de fond (`duplicates.rs:31-32`) sont bornées à l'espace. L'ancien écrivain n'envoie aucun espace (DÉDUIT de `kleos-ingest/src/writer.rs:100-107`) |
| `status` | Celui par défaut de Kleos (`approved`), puisque la porte n'est jamais activée | F13 ; protection en 4.4 |

**Résolution du space d'une session.** Elle se fait sur le poste, parce que seul le
poste voit le système de fichiers de la session ; le relais sur LXC 121 ne peut pas
remonter l'arborescence Windows (DÉDUIT de 1.1). Règle, dans l'ordre :

1. **Le space exact de la session**, s'il existe : le hook de démarrage écrit le space
   résolu dans `~/.kleos/sessions/<session_id>/space_name`
   (`~/.claude/hooks/lib-kleos-space.sh:133-144`, DÉDUIT). C'est le même space que
   celui des stores de l'agent dans cette session, donc le seul qui garantit que la dédup
   compare les bonnes lignes.
2. **Sinon, la résolution du hook appliquée au `cwd` du premier enregistrement de la
   session** : marqueur `.kleos-space` en remontant, puis nom du dépôt git, puis nom du
   dossier, normalisé en `[a-z0-9_-]` (`lib-kleos-space.sh:49-78` ; même règle en Rust
   dans `kleos-cli/src/space.rs:56-85`, tenue en parité par un test, DÉDUIT des en-têtes).
   Le démon la réimplémente sur ce modèle avec les mêmes fixtures, puisque
   `kleos-cli` est un binaire et non une bibliothèque (SUPPOSÉ). **Le démon ne crée
   jamais de marqueur** : il lit seulement, alors que le hook en crée un s'il manque
   (`lib-kleos-space.sh:84-99`).
3. Un `cd` en cours de session ne change pas le space : la session garde celui de son
   démarrage, comme le hook.

**Un `cwd` sans space.** La règle 2 rend toujours un nom (dossier en dernier recours).
En pratique, le hook a déjà posé un marqueur dans le `cwd` au démarrage de la session
quand il n'y avait ni marqueur ni dépôt (`lib-kleos-space.sh:90-99`) : le dernier recours
est donc rare (DÉDUIT). Il reste deux cas à traiter :
- **Le `cwd` est le dossier personnel ou une racine** (`C:\Users\Olivier`, `C:\`) :
  le nom obtenu ne désigne aucun projet, et Kleos créerait un space au premier store
  (Patch 33, `routes/memory/mod.rs:124-127`). Les faits de ces sessions sont **gardés
  dans le relais, non publiés** en v1. Ils ne vont pas non plus en mémoire sans space :
  l'absence de space signifie « fait transverse » pour l'opérateur (Kleos #18425), ce
  qu'une extraction de session ne garantit pas.
- **Session dont le `space_name` et la résolution divergent** : le `space_name` l'emporte
  (règle 1), la divergence est consignée dans le rapport du relais.

**Résumés de session : pas de mémoire en v1.** Ils restent dans le relais, comme
matériau de travail. F7 montre ce que vaut un résumé servi comme mémoire ; un résumé de
session est épisodique et concurrence les faits au classement. Question Q3.

### 4.6 Branchement sur le juge, la supersession et la contradiction

Quatre barrières contre le doublon agent/transcript, de la moins chère à la plus chère :

1. **Dans le prompt** : les stores de l'agent vus dans la fenêtre (gardés en entier par
   A+ jusqu'à la révision 5 ; depuis la révision 6, marqueurs `{stored: id}` résolus par
   le relais, 3.3) sont fournis au modèle comme « déjà stocké », et la même liste est la
   **liste négative** du juge amont : un fait dont le contenu recouvre un id stocké est
   `doublon` sans recherche ;
2. **Au juge amont** (4.4, couche 1) : un `doublon` d'une mémoire servie n'est pas
   publié ;
3. **À l'écriture** : la dédup de `memory::store` absorbe une copie quasi identique
   d'une mémoire déjà servie (distance de Hamming inférieure à 3, même espace,
   `mod.rs:586-616`, DÉDUIT) ;
4. **Au juge nightly**, a posteriori, selon la table ci-dessous.

**Rôle du juge nightly sans porte : il annote toujours ; il révise ou retire seulement
une mémoire de transcript.** Une mémoire de transcript publiée est une racine du
voisinage de l'étude nightly 4.6, avec les mêmes candidats (`similarity`, M1,
déduplication), le même propriétaire et le même espace. Le principe 4 de l'étude
nightly (« un faux retrait est pire qu'un retrait manqué ») reste entier pour les
mémoires d'agent : le juge ne les retire jamais sur la foi d'un transcript. Pour une
mémoire de transcript, le retrait est un **archivage réversible** (`unarchive`,
`routes/memory/mod.rs:63`), justifié parce que la mémoire est dérivée et que son
original reste dans le relais 14 jours. Cette autorité est nouvelle et **demande
l'accord de l'opérateur (Q2)** :

| Verdict du juge pour la racine de transcript | Effet |
|---|---|
| `distinct`, `prolonge` | Annotation seulement |
| `doublon` d'une mémoire d'agent | Laissé à la dédup de fond (Patch 60), qui garde la copie de l'agent grâce au plafond de 5 (F16) ; annotation |
| `met_a_jour`, `partiel`, `remplace` face à une mémoire d'agent plus ancienne | Annotation de relation ; aucun retrait de la mémoire d'agent sans le signal explicite de l'étude nightly 4.1 et 4.6 |
| `met_a_jour` ou `remplace` venant d'une mémoire d'agent **plus récente** que la mémoire de transcript | **Archivage** de la mémoire de transcript : l'agent a écrit après elle, son écriture explicite l'emporte |
| `contredit` par une mémoire d'agent | **Archivage** de la mémoire de transcript, avec annotation du conflit ; la mémoire d'agent n'est pas touchée |
| `contredit` par une autre mémoire de transcript | Annotation du conflit et **archivage de la plus ancienne** des deux, selon le `created_at` du tour source (4.5) : le fait le plus récent l'emporte, sans arbitrage humain (4.7). Limite nommée : si la plus récente est elle-même fausse, l'archivage retire le vrai ; il reste réversible |
| Version de transcript plus ancienne que la tête d'une lignée du relais | Rien à faire : la chaîne de versions (4.3) l'a déjà retirée par `is_latest = 0` |
| Juge en panne ou budget épuisé | Rien n'est archivé : échec fermé, les mémoires restent servies au plafond de 5, comme avant le passage |

**Rétrograder** une mémoire de transcript, c'est-à-dire baisser son importance par une
nouvelle version (`POST /memory/{id}/update`, `routes/memory/mod.rs:59`), reste
possible mais n'est pas une règle par défaut : au plafond de 5, la mémoire est déjà
hors des tiers poussés (F14), et l'archivage est le seul geste qui la sort d'une
recherche pertinente.

**Inbox.** L'inbox d'approbation de Kleos existe et se consulte depuis l'interface web
GUI (F13). Cette cible ne s'en sert pas, puisqu'aucune mémoire de transcript n'est en
statut `pending`, et aucune relecture humaine n'est prévue (4.7) : tout conflit est
tranché par une règle de cette table, et les annotations du juge en gardent la trace.

**Coût, SUPPOSÉ :** au juge nightly, à 30 mémoires de transcript publiées par jour ouvré,
environ 0,45 USD par jour au coût M-N4, en plus du juge amont (4.4). Le volume réel sort
de la porte du lot 4.

### 4.7 Autonomie : ce qui bloque la mise en service, ce qui ne la bloque pas

**Contrainte opérateur (2026-10-21) : aucune relecture humaine des mémoires n'est
possible.** Toute la chaîne doit trancher seule ; un rapport peut informer, il ne doit
jamais attendre une décision. Les règles de 4.4 et 4.6 sont écrites dans ce sens.

| Niveau | Contenu | Autonome | Bloque la mise en service |
|---|---|---|---|
| **(a) Minimum pour publier sans porte** | Couche 0, détecteur de secrets (révision 6, 1.4) ; couche 1, juge amont du relais (fidélité et voisinage, 4.4) ; couche 2, importance au plus 5 ; couche 3, provenance par source et tags ; révision et archivage par le relais de ses propres publications dans la même lignée (4.3) | Oui : aucun verdict n'attend un humain, et un juge ou un détecteur en panne bloque la publication sans rien exposer | **Oui.** Sans la couche 0, un secret manqué par la rédaction du démon devient une mémoire sans expiration (F17 montre que le cas est réel) ; sans la couche 1, un fait faux de Qwen est servi sans aucun contrôle ; les couches 2 et 3 sont des réglages de publication, sans coût de développement propre |
| **(a') Dédup de fond, niveau déterministe** | Auto-linker et dédup du Patch 60 à 0,9, déjà en place (4.8) | Oui | **Oui, mais déjà présente** : la porte se limite à vérifier son activation en production (lot 0) |
| **(b0) Dédup de fond, bande grise** | File `to_dedup` du relais et jugement Sonnet entre 0,80 et 0,90 (4.8) | Oui | **Non** : sans elle, une reformulation reste servie en double, la copie de transcript au plafond de 5 |
| **(b) Nettoyage autonome a posteriori, mémoires de transcript** | Couche 4, juge nightly avec archivage réversible des mémoires de transcript contredites ou remplacées (4.6) | Oui | **Non.** Elle dépend de l'étape 2 de l'étude nightly (4.10) ; sans elle, un fait faux qui a passé la couche 1 reste servi au plafond de 5, hors des tiers poussés (F14). C'est un risque borné, pas une exposition ouverte |
| **(b') Nettoyage autonome des mémoires d'agents existantes** | Repérer et sortir les mémoires inutiles déjà présentes dans Kleos, que l'opérateur constate | À concevoir | **Non.** Hors de cette étude : c'est le terrain du juge de voisinage de l'étude nightly, dont le principe 4 interdit aujourd'hui tout retrait sans signal explicite. Ce que cette étude peut lui prêter : l'archivage **réversible** comme geste autonome, qui réduit le coût d'un faux retrait ; la question de fidélité ne s'applique pas, une mémoire d'agent n'ayant pas de source à relire. Assouplir le principe 4 pour les agents est une décision à prendre dans l'étude nightly, pas ici |

**Mesures sans relecture humaine.** Les portes des lots 4 et 5 qui demandaient un
étiquetage par l'opérateur passent à un juge indépendant aveugle : un autre modèle que
le juge amont, qui ne voit pas son verdict (section 8). L'opérateur peut ajouter un
échantillon s'il le souhaite ; aucune porte n'en dépend. **Limite nommée :** deux
modèles peuvent partager le même biais ; le juge indépendant mesure l'accord, pas la
vérité, et la porte de 5 % de faits faux est une borne sur cet accord. Le juge de mesure
est Sonnet, d'une autre famille que l'extracteur (Qwen) et d'un autre modèle que le juge
amont (Haiku). La bande grise est elle aussi jugée par Sonnet (4.8), mais sa porte du
lot 5 repose sur des fixtures, pas sur le juge de mesure : l'indépendance tient.

### 4.8 Dédup de fond : couche principale contre le doublon publié avant l'agent

**Ce qui existe déjà, et qui couvre le niveau déterministe sans rien écrire.**

| Mécanisme | Quand | Seuil et règle | Fondement |
|---|---|---|---|
| Dédup à l'écriture | À chaque `POST /memory` | Simhash, distance de Hamming inférieure à 3, même propriétaire et même espace ; la nouvelle écriture est absorbée par la ligne existante | DÉDUIT (`kleos-lib/src/memory/mod.rs:563-616`) |
| Auto-linker | À chaque tick du dreamer, par lot de 50 mémoires encore sans lien | Plus proches voisins par ANN, cosinus au moins 0,55, au plus 6 liens `similarity` bidirectionnels par mémoire | DÉDUIT (`kleos-lib/src/intelligence/linker.rs:14-47`, `kleos-config/src/config.rs:162-172`) |
| Dédup de fond (Patch 60) | Juste après l'auto-linker, dans le même tick | Paires de liens `similarity` à au moins **0,9**, au plus 500 paires par tick ; deux mémoires servies, `is_latest`, même propriétaire, **même espace** (filtre du Patch 33) ; garde l'importance la plus haute, puis l'id le plus haut ; l'autre passe à `is_superseded = 1` avec un lien `supersedes` | DÉDUIT (`kleos-lib/src/intelligence/scheduler.rs:235-250`, `duplicates.rs:10-131`) |
| Cadence du dreamer | Un tick toutes les 300 s, seulement si le serveur est resté 60 s sans requête | | DÉDUIT (`config.rs:147-160`, `kleos-server/src/dreamer.rs:193-215`, `:284-289`) |
| Consolidation | Même pipeline, après la dédup | Cosinus au moins 0,85, **désactivée par défaut** | DÉDUIT (`scheduler.rs:252-264`, `config.rs:902`) ; valeur en production SUPPOSÉE |

**Lecture pour le cas transcript contre agent.** Une mémoire d'agent stockée après une
mémoire de transcript est liée à ses voisins au premier tick qui la traite ; si la
mémoire de transcript est parmi eux à au moins 0,9, la dédup de fond garde l'agent,
puisque le plafond de 5 la place sous toute mémoire d'agent (4.4, couche 2). **Le niveau
déterministe demandé par l'opérateur existe donc déjà**, avec une cadence de quelques
minutes plutôt que 24 h, et sans fenêtre de temps : l'ANN compare à tout l'espace, ce qui
couvre largement une fenêtre de 24 h ou plus (DÉDUIT). Il retire par supersession et non
par archivage, avec le même effet sur les lectures servies (filtres `is_superseded = 0`,
`mod.rs:1276-1358`).

**Ce qu'il manque pour la bande grise.** Rien n'agit sous 0,9 : une paire à 0,85 reste
servie en double. Les limites nommées du niveau existant :
- au plus 6 voisins par mémoire : dans un voisinage dense, la paire peut ne jamais être
  liée (DÉDUIT de `linker.rs:28`) ;
- l'auto-linker ne lie qu'une fois, à l'arrivée de la mémoire ; une mémoire d'agent
  arrivée après est liée de son côté, puisque le lien est bidirectionnel, donc la paire
  est vue (DÉDUIT de `linker.rs:28-31`) ;
- son activation en production est SUPPOSÉE (défaut `true`, `config.rs:164-166`), à
  vérifier au lot 0.

**Bande grise proposée, dans le relais, zéro delta.**
1. **Détection.** Chaque nuit, pour chaque mémoire de transcript publiée depuis moins de
   14 jours, le relais lit ses liens par `GET /links/{id}` (`routes/memory/mod.rs:56`).
   Toute voisine servie dans la bande **[0,80 ; 0,90[** entre dans une file `to_dedup`
   (bornes SUPPOSÉES, calibrées au lot 5 sur la distribution réelle des similarités).
   SUPPOSÉ : la réponse de `/links` porte la similarité et le type de lien.
2. **Jugement.** Sonnet reçoit les deux contenus et rend `doublon`, `distinct` ou
   `incertain`. Sonnet plutôt que Haiku (décision opérateur du 2026-10-22) : ce juge
   retire des mémoires servies, et Kleos #30268 a mesuré Haiku à 1 à 3 faux retraits
   sur 10 contrôles contre 0 pour Sonnet medium, pour un coût environ 8 fois plus élevé
   sur le même lot (0,935 contre 0,124 USD, MESURÉ rapporté) ; au volume de la bande
   grise, l'écart reste sous 0,12 USD par jour.
3. **Effet**, borné aux mémoires de transcript :
   - `doublon` d'une mémoire d'agent : la mémoire de transcript est archivée,
     réversiblement (`routes/memory/mod.rs:62-63`) ;
   - `doublon` entre deux mémoires de transcript : la plus ancienne est archivée ;
   - `distinct` : la paire est close et n'est plus reposée, tant qu'aucune des deux
     n'a de nouvelle version ;
   - `incertain` : la paire reste servie, close ; la mesure du lot 5 dit si un
     second avis d'un autre modèle (Opus, ou GPT par `codex exec` sur l'abonnement
     OpenAI) vaut son coût.

**Où ranger `to_dedup`.**

| Emplacement | Delta | Verdict |
|---|---|---|
| Tag sur la mémoire (`PUT /memory/{id}/tags`, `routes/memory/mod.rs:60`) | Nul | **Écarté** : écrit sur des lignes servies, y compris des mémoires d'agent, visible de tous les lecteurs et de kleos-cache, et sans place pour le verdict ni la similarité |
| **Table du relais** `dedup_queue(memory_id, neighbour_id, similarity, found_at, verdict, judged_at, versions)` | Nul | **Retenue** : le relais ne file que des paires dont un membre est à lui, et garde la trace des verdicts |
| Table overlay VOCSAP et passe dans kleos-server | Additif pour la table (Patch 41), plus un branchement dans le dreamer, fichier classé « ELEVE » (1.3, E9) | Réservé au cas agent contre agent, s'il est retenu par l'étude nightly |

**Agent contre agent.** Le niveau déterministe s'applique déjà à toutes les sources : la
dédup de fond ne regarde pas la source (`duplicates.rs:17-35`, DÉDUIT). La bande grise
entre deux mémoires d'agents, que l'opérateur constate, demande de retirer des mémoires
d'agent : c'est le terrain du juge de voisinage de l'étude nightly et de son principe 4
(4.7, niveau b'). Ce que la cible peut fournir sans autorité nouvelle : le relais peut
**mesurer** la bande grise agent contre agent, en lecture seule, et donner à l'étude
nightly le volume réel avant toute décision. Non bloquant.

**Mise en service.** Le niveau déterministe est **bloquant mais déjà présent** : la
porte est de vérifier en production que l'auto-linker et la dédup tournent (lot 0). La
bande grise est **non bloquante** : sans elle, une reformulation entre 0,80 et 0,90
reste servie en double, au plafond de 5 pour la copie de transcript.

---

## 5. Rétention, rattrapage, purge des copies

> **Révision 7.** Remplacée par la conception hypomnema §7.1 (stockage et rétention) et
> §7.4 (portée du map et rattrapage) : hypomnema garde tout l'extrait sans limite de
> durée, sans niveaux (décision opérateur, #33942) ; une époque remplacée n'est supprimée
> qu'après l'acquittement du flux par chaque consommateur (§10). Le rattrapage suit la décision #33968 :
> sessions team-lead et sans agent seulement, de la plus récente à la plus ancienne,
> profondeur `backfill_days` réglable sans retraiter l'existant. Côté Kleos, seule la
> correspondance des faits publiés est conservée, sans limite (11.2).

| Objet | Rétention proposée | Raison |
|---|---|---|
| Enregistrements B rédigés (relais), révisions 2 à 5 | 14 jours après la dernière stabilisation qui les couvre (X = 14, paramètre) | Retraiter après un changement de vue, de prompt ou de modèle ; couvrir une reprise tardive ; borner l'exposition des secrets résiduels (1.4). **Remplacé en révision 6 par la ligne suivante** |
| **Enregistrements A+ rédigés (relais), révision 6** | 14 jours après la dernière stabilisation qui les couvre (X = 14, paramètre) | Retraiter après un changement de prompt ou de modèle sans renvoi ; couvrir une reprise tardive ; borner l'exposition des secrets résiduels. Un changement de **vue** passe par un renvoi depuis le poste (2.8), donc la rétention n'a plus à le couvrir. Le volume (environ 130 Mo, E6) permettrait plus long ; la borne reste 14 jours par la surface de secrets (Q6) |
| Corps des `<channel>` sans original trouvé (3.4) | Comme les enregistrements A+ | Seul endroit où un message de l'opérateur ou d'un pair hors parc est écrit |
| Instantanés d'état de lignée | Comme les enregistrements, et au moins tant que la lignée peut être reprise dans la fenêtre de 14 jours ; les instantanés d'une vue remplacée (2.8) jusqu'à la stabilisation suivante | Héritage à la reprise (4.3) |
| `uuid_index`, `lineage` | 400 jours (paramètre) | **La dédup des reprises dépend de l'index, pas du contenu** : un `claude --resume` après la suppression du contenu ne doit pas réextraire l'historique recopié |
| Résumés et faits candidats | Comme les enregistrements | Matériau de travail |
| Correspondance fait vers id et version Kleos, **avec le contenu publié** (révision 6) | Sans limite | Idempotence des publications, révision (4.3), provenance ; entrée de la règle de montée de vue (2.8), qui doit pouvoir comparer après l'expiration du contenu A+ |
| Mémoires publiées | Cycle de vie Kleos ordinaire | Hors relais |
| Côté poste | Aucune copie ; le démon ne supprime jamais un transcript Claude Code ; table `user_prefixes` (préfixes de 40 caractères, 2.2) sans limite | Le fichier est la source, gardée **180 jours** par Claude Code (F20) : c'est la borne réelle de tout retraitement depuis la révision 6 (2.8) |

**Reprise après expiration du contenu.** Une reprise d'une lignée dont les
enregistrements ont expiré ne retrouve pas d'instantané : ses nouveaux tours partent
d'un état vide, mais la correspondance reste, donc une révision ne peut viser un fait
publié que par la dédup et le juge. **Limite nommée**, rare : T3-a mesure des copies à
p50 16 min de la source.

**Rattrapage des 7 Go : borné.** Recommandation : les 14 derniers jours (environ 4 Go
bruts au flux moyen, environ 600 Mo en B et environ 100 Mo en vue A+, DÉDUIT de F1 et
F3 ; révision 6 : seuls les environ 100 Mo de A+ transitent et sont stockés, le chiffre
B reste comme repère), d'abord comme **corpus de calibration** du lot 4, puis publication si la porte
passe : environ 2 800 fenêtres, soit 9 à 17 h d'Ollama étalées sur plusieurs nuits
(DÉDUIT de T2). Le reste n'est pas traité par défaut : l'historique ancien concurrencerait
des mémoires d'agent plus récentes au juge pour un gain non mesuré, et coûterait environ
20 à 35 h d'Ollama en vue A+ (DÉDUIT de T2 et de la cible de 3 %). Question Q4.

---

## 6. Modes de défaillance

> **Révision 7.** Les lignes qui touchent le poste, le protocole, la rédaction, la vue et
> le map sont reprises par hypomnema (conception §11). Restent du ressort de Kleos : Kleos
> en panne, tête de chaîne déplacée, fait faux, stores d'agent tardifs, session sans space
> exploitable, juge amont en panne, secret détecté dans un fait candidat. La ligne « vraie
> branche » est réécrite en 11.4 : F21 montre que les branches sœurs sont fréquentes.

| Défaillance | Comportement attendu | Perte possible |
|---|---|---|
| Coupure poste (extinction, `taskkill /F`) pendant un envoi | Curseur inchangé ; au redémarrage, renvoi du segment ; `200 duplicate` si le relais l'avait écrit | Aucune |
| Base d'état du démon perdue | `GET /v1/files/{file_key}/cursor` restaure époque et offset | Aucune |
| Jeton du poste absent ou révoqué | `401`, curseur inchangé, alerte | Aucune ; retard |
| Relais en panne | Le démon recule (backoff 1 s à 5 min), le fichier reste le tampon | Seulement si Claude Code supprime le fichier avant le retour du relais |
| Kleos en panne | Réception et traitement continuent ; publications en `intention`, relancées ; recherche par tag `fact:` avant republication (4.3) | Aucune |
| Ollama en panne, JSON invalide, ou réponse tronquée (`done_reason = length`, raisonnement présent) | Fenêtre en `erreur`, relance bornée, jamais marquée faite ; la rétention ne démarre qu'à la stabilisation traitée | Aucune ; retard |
| Reprise après la stabilisation (0,73 % des sessions, T3) | Lignée rouverte, état hérité de l'instantané, révision des faits publiés par chaîne de versions (4.3) | Aucune ; fenêtre de masquage si la porte est active (limite nommée en 4.3) |
| Tête de chaîne déplacée par une autre écriture | `Conflict` ; publication en mémoire neuve, juge | Aucune ; un doublon possible, rejeté ou annoté par le juge |
| Vraie branche (source et copie continuent) | Deux branches depuis le même instantané ; seconde révision en `Conflict`, voir ligne précédente | Aucune ; jamais observée (T3-a) |
| Fait faux, infidèle ou contredit en sortie de Qwen | Refusé par le juge amont, gardé dans le relais (4.4, couche 1) ; s'il passe, servi au plafond de 5, hors des tiers poussés, puis archivé par le juge nightly s'il est contredit par une mémoire d'agent (4.6) | **Exposition résiduelle** : servi aux recherches pertinentes jusqu'au passage nightly suivant |
| Store d'agent plus de 8 h après la lignée, texte quasi identique à une publication de transcript | Absorbé par la ligne de transcript (`created=false`) ; le fait reste servi, avec l'importance 5 et la source du transcript | Aucune perte ; sous-classement nommé en 4.4, fréquence mesurée au lot 5 |
| Store d'agent plus de 8 h après la lignée, reformulation | Deux lignes ; à au moins 0,9 la dédup de fond garde l'agent ; entre 0,80 et 0,90, la bande grise du relais archive la copie de transcript (4.8) | Aucune ; doublon servi jusqu'au tick ou à la nuit suivante |
| Session sans space exploitable (`cwd` personnel ou racine) | Faits gardés dans le relais, non publiés (4.5) | Faits non publiés, consignés |
| Juge amont en panne ou budget épuisé | Aucune publication ; candidats gardés dans le relais jusqu'au retour du juge | Aucune ; retard |
| Secret non reconnu par la rédaction (révisions 2 à 5) | Stocké chiffré au relais, au plus 14 jours ; présenté au modèle seulement s'il tombe dans la vue A+ | **Exposition résiduelle, limite nommée (1.4)** |
| Secret non reconnu par la rédaction (révision 6) | N'atteint le relais que s'il tombe dans la vue A+ (200 premiers caractères ou ligne d'erreur d'un `tool_result`, texte user ou assistant) ; stocké chiffré au plus 14 jours ; présenté au modèle ; **s'il ressort dans un fait candidat, le détecteur de la couche 0 (4.4) le bloque s'il reconnaît un format, sinon il atteint Kleos** | **Exposition résiduelle, limite nommée (1.4)** : les deux barrières partagent leurs formats ; la surface A+ et la passe d'entropie sur les faits courts la réduisent sans l'annuler |
| Secret détecté dans un fait candidat (révision 6) | Non publié, gardé au relais avec le type détecté, consigné au rapport | Un fait vrai retenu sur faux positif ; jamais une ligne entière effacée (rédaction au jeton) |
| Mise à jour de Claude Code qui change la structure des lignes (révision 6) | Le filtre structurel de 3.3 est en échec fermé : du texte user tapé peut être retiré ; l'alerte de dérive (part gardée qui double ou tombe à la moitié sur 100 sessions d'une nouvelle `cc_version`) le signale au rapport ; correction par une nouvelle `wire_purge_version` puis montée de vue (2.8) sur les fichiers encore sur le poste | Texte user non extrait entre la mise à jour et la montée de vue ; rien n'est publié à tort |
| Injection du harnais non listée et non répétée dans 5 sessions (révision 6) | Passe pour du texte user ; le juge amont (couche 1) la traite comme tout texte ; le compteur de préfixes la classe dès la cinquième session | Un fait extrait d'un texte de hook, au plafond de 5, jusqu'au classement |
| `/rewind` (révision 6) | Branche abandonnée détectée par la chaîne des parents depuis la dernière feuille (3.3) ; ses faits reçoivent `annule` ; s'ils étaient publiés, archivage réversible (4.3) | Aucune ; un fait publié entre la stabilisation et un rewind tardif reste servi jusqu'à la stabilisation suivante |
| Ligne complète illisible (JSON invalide terminé par `\n`) (révision 6) | Marqueur `{unparsable}` à son offset, comptée, le curseur avance (2.3) | Le contenu de cette seule ligne |
| Enregistrement A+ de plus de 256 Kio (révision 6) | Part seul dans son segment, plafond dur de 4 Mio avec `truncated` au-delà (2.3) | Au-delà de 4 Mio, la fin du texte de cet enregistrement |
| Montée de vue (`view_upgrade`) sur un fichier supprimé du poste ou poste éteint (révision 6) | Non retraité ; la lignée garde ses faits de l'ancienne vue ; `file_key` listé au rapport | Pas de retraitement pour ce fichier, limite assumée (2.8) |
| Montée de vue sur une lignée déjà publiée (révision 6) | État recalculé, comparé aux publications de la lignée (2.8) : rien, version ou publication neuve ; aucun archivage par absence | Un doublon sous 0,9 et hors Hamming 3 possible, traité par la bande grise (4.8), qui archive la plus ancienne |
| `send_message` identique envoyé à N pairs, ou `<channel>` dont l'original est transmis (révision 6) | Dédupliqué dans la session émettrice ; corps retiré de la vue à la stabilisation réceptrice si l'original est dans `sent_index` (3.3, 3.4) | Aucune ; si le segment émetteur arrive plus de 8 h après celui du récepteur, le corps est extrait deux fois et la dédup de Kleos ou la bande grise absorbe |
| Fichier réécrit ou tronqué | Détection 2.5, nouvelle époque `rewrite`, renvoi complet ; uuid connus ignorés | Aucune |
| `/compact` | Même fichier, ajout simple ; résumé de compaction retiré, marqueur gardé | Aucune |
| Ligne en cours d'écriture | Jamais envoyée avant son `\n` (2.3) | Aucune |
| Disque plein, relais | `507`, aucune écriture partielle ; alerte | Aucune ; retard |
| Disque plein, poste | Échec d'écriture de l'état, donc pas d'avance de curseur ; renvoi ensuite | Aucune |
| Deux postes, même projet | `file_key` inclut `host_id` ; l'index uuid est par propriétaire, pas par poste | Aucune |
| Horloge ou timestamps non monotones | Ordre par offset ; `T_settle` mesuré à l'horloge du relais (F4) | Aucune |
| Bump de kleos-lib qui casse la compilation ou le test jetable | Le relais reste sur l'ancien rev, qui continue de parler à Kleos par l'API HTTP ; le bump attend une adaptation | Aucune ; retard du bump. Répété à chaque pull, ce cas rouvre le verdict 1.3 |
| Changement de vue, de prompt ou de modèle (révisions 2 à 5) | `model_view_version` ou `contract_version` rouvre les lignées encore en rétention ; les écarts avec la correspondance passent par 4.3 | Au-delà de 14 jours : pas de retraitement |
| Changement de prompt ou de modèle (révision 6) | `contract_version` rouvre les lignées encore en rétention au relais, sans renvoi ; écarts par 4.3 | Au-delà de 14 jours : pas de retraitement sans renvoi |
| Changement de vue (révision 6) | Nouvelle `wire_purge_version` au démon, montée de vue par renvoi depuis le poste (2.8), lignées rouvertes `reopen_for_view` ; écarts par la table de 2.8 | Au-delà de 180 jours (F20), fichier supprimé ou poste éteint : pas de retraitement |

---

## 7. Sort de kleos-ingest et des cartes

**kleos-ingest : gardé dans l'arbre, non déployé, non étendu.** C'est un crate upstream
(Patch 58, 69 et 70 y sont de petits deltas, candidats PR). Le supprimer ou le réécrire
sur place créerait le plus gros site de conflit possible (niveau « réécriture »).

> **Révision 7.** Les deux raisons ci-dessus (conflit au rebase, candidats PR) sont
> caduques depuis le gel upstream du 2026-10-22 (Kleos #32360). La conclusion tient pour
> une autre raison : hypomnema reprend tout ce que kleos-ingest devait faire, donc le crate
> n'a plus d'usage, et le retirer est un ménage sans urgence. Les idées reprises dans la
> table ci-dessous le sont désormais par `hypomnema-agent`. Cartes : `f18dc3c4` recentrée
> le 2026-10-24 sur le consommateur Kleos ; `6322672e` porte le hook d'étiquette de space ;
> `20d18577` reste en `wontfix`.

| Morceau | Sort |
|---|---|
| Curseur par offset et offset durable (`tailer.rs:61-119`) | Idée reprise, réécrite : fin de ligne obligatoire (2.3) et acquittement par le relais |
| Watcher `notify`, portage Windows (Patch 69) | Idée reprise ; `notify` reste un accélérateur, le scan par stat est la vérité |
| Ledger (`ledger.rs:28-43`) | Remplacé par la table 2.2 (époque, empreintes) |
| Extracteur lexical, résumé de 3 000 caractères | Abandonnés (F7, F8) |
| Signature PIV et repli `X-API-Key` du writer | Non repris : démon vers relais par jeton par poste (1.4) ; relais vers Kleos par `Authorization: Bearer`, déjà lu par le serveur |
| `parse_session_path` (`tailer.rs:13-24`) | Non repris : le projet vaut `projects` sur le chemin réel (append 20d18577, DÉDUIT) |

Cartes (recommandations, non appliquées par cette étude) :

- **20d18577 : caduque si la cible est acceptée.** Son option A corrige un démon qui ne
  sera pas mis en service ; la reprise exacte est un invariant du nouveau protocole
  (2.4 à 2.7). Proposition : `priority = wont`, triage `wontfix`, renvoi vers cette étude.
- **f18dc3c4 : à recentrer** sur la cible (lots de la section 8). Le verrou « X-API-Key
  non lu par le serveur » reste un bug upstream réel de kleos-ingest, à garder comme
  candidat PR, mais ne bloque plus la cible.
- **6ee2297f : archivée en wontfix** le 2026-10-21, la porte n'étant jamais activée
  (F13) ; la cible ne dépend plus d'aucun correctif de la dédup.
- **3f45e0f8** et **19671bc8** : livrés, sans suite dans la cible.

---

## 8. Lots ordonnés et portes mesurables

> **Révision 7.** Cette table décrit la cible de la révision 6. Ses lots passent ainsi :
> F (1) et F (5) au lot 1 d'hypomnema (contrat du fil, fichier de motifs) ; F (2), F (3)
> et F (4) restent à Kleos (lot K0 de 11.6) ; 0 au lot 0 d'hypomnema, déjà exécuté pour
> (d), (i), (j), mesures F21 à F26 ; 1, 2 et 3 aux lots 1 à 3 d'hypomnema ; 4 au lot 6
> d'hypomnema (map), dont la porte de qualité des faits est reprise comme porte de
> publication de Kleos (11.5) ; 5 et 6 restent à Kleos, réécrits en lots K1 à K4 (11.6).

| Lot | Contenu | Porte mesurable |
|---|---|---|
| **F. Gel avant tout code** (verdict 1.3) | (1) contrat HTTP démon vers relais, sections 2.3 et 2.4, versionné dans le dépôt du relais (révision 6 : avec `epoch_reason`, `stats`, `cc_version`, les marqueurs de 2.3, les réponses `same_view` et la route `stale`) ; (2) contrat du juge amont : prompt partagé, question de fidélité, schéma de verdict et effets (4.4, couche 1) ; (3) rev git épinglé de kleos-lib sur le fork VOCSAP et règle de bump écrite (à chaque merge upstream du fork, après un build vert) ; (4) liste des champs de `StoreRequest` et des fonctions de kleos-lib utilisés (révision 6 : trois fonctions plus le chargeur du lexique, 1.3), avec un test d'intégration jetable ; (5) **révision 6** : contrat du crate de rédaction partagé démon et relais : liste des formats de 1.4, `secret_patterns_version`, sémantique « valeur masquée, ligne conservée » | (1) schéma JSON du segment et table des réponses figés, relus par le lead ; (2) schéma de verdict figé et fixtures écrites : un fait inventé, un fait déformé, un doublon d'agent, une contradiction d'agent, un fait fidèle et distinct ; (3) le relais compile contre le rev épinglé, hors des `members` du `Cargo.toml` racine (`git diff Cargo.toml` vide) ; (4) le test jetable passe sur le rev épinglé ; (5) liste des formats relue par le lead contre F17 et F19 : chacun des cinq comptes de F17 (3, 4, 12, 51, 0) a un format qui le couvre, et `key`, `privatekey`, `api-key` (F19) y figurent |
| **0. Mesures opérateur** (F9 : geste opérateur) | (a) second passage ajout seul : sha256 du préfixe `[0, taille_T0)` à T0 puis T0 + 24 h sur tous les fichiers ; (b) taille A+ et taille B sur les 7 Go, dont la vue sous-agent de 3.4 à part ; (c) part des enregistrements gardés sans uuid, par type ; (d) présence du rapport de sous-agent dans le `tool_result` parent, 20 sessions (révision 6 : informative, les sous-agents sont envoyés dans tous les cas, 3.4) ; (e) lignes partielles observées en fin de fichier ; (f) rappel de la rédaction : nombre de secrets connus de l'opérateur (inventaire `cred list`, noms seulement) retrouvés en clair après rédaction, sur 14 jours ; (g) les deux points non vérifiés par le consultant : `GET /api/ps` sur l'Ollama 192.168.10.50, et chiffrement de `kleos.db` en production ; (h) activation en production de l'auto-linker, de la dédup de fond et de la consolidation (`KLEOS_AUTO_LINK_ENABLED`, `consolidation_enabled`), et distribution des similarités des liens `similarity` par paires de sources ; **révision 6** : (i) distribution des préfixes de 40 caractères des blocs `text` user par nombre de sessions, pour calibrer le seuil de 5 (3.3) ; (j) forme exacte d'un `/rewind` dans le fichier (lignes abandonnées en place, `parentUuid` de reprise), 5 cas ; (k) volume et nombre des `<channel>` dont l'émetteur est `operator`, le Deck ou un pair hors parc (3.4) ; (l) part de texte user gardée par le filtre structurel de 3.3, par version de Claude Code présente dans le corpus, comme base de l'alerte de dérive | (a) 0 préfixe modifié hors `/compact` documenté, sinon la réécriture (2.5) devient le chemin nominal et passe en tête du lot 2 ; (b) A+ au plus 3 % du brut ; (c) chiffre consigné ; (d) chiffre consigné ; (e) chiffre consigné ; (f) 0 valeur du coffre retrouvée en clair ; (g) `/api/ps` rend la liste des modèles chargés, sinon la garde de 4.1 passe à un repli (fenêtre horaire fixe) ; état de chiffrement de `kleos.db` consigné, pour fixer E7 ; (h) le rapport du dreamer montre des ticks `auto_link` et `deduplicate` réussis sur 24 h, sinon le niveau déterministe de 4.8 manque et la mise en service attend ; distribution consignée pour borner la bande grise ; (i) au seuil retenu, 0 préfixe tapé par l'opérateur classé injection sur un échantillon de 50 préfixes classés, relu par lui ; (j) forme consignée, la règle de 3.3 ajustée si la chaîne des parents ne suffit pas ; (k) chiffre consigné ; (l) base consignée |
| **1. Contrat de purge et rédaction** (révision 6 : contrat du fil A+, fixtures de secrets et d'injections) | Spécification de la vue A+ (3.3, 3.4), de la rédaction au jeton (1.4) et de `wire_purge_version` 1 côté démon ; crate de rédaction partagé (lot F (5)) ; détecteur de secrets du relais (4.4, couche 0) ; fixtures synthétiques : injections du harnais (`<system-reminder>`, `<channel>`, corps de skill « Base directory for this skill », `<task-notification>`, blocs `=== … ===`, refus de hook `PreToolUse:… hook error` dans un `tool_result`, préambule de compaction, bloc `isMeta`), `send_message` simple et identique à 3 pairs, stores Kleos avec et sans id lisible, compaction, `/rewind`, ligne JSON invalide, enregistrement de 300 Kio et de 5 Mio ; **fixtures de secrets** : sorties `env`, `Get-ChildItem Env:`, `docker inspect`, `cat .env`, `kleos.env`, `sqlite3` en colonnes `|`, URL `scheme://user:secret@host/path`, `PrivateKey =`, une valeur de chaque format de 1.4 (`kleos_<hex32>`, `kleos_<hex64>`, `gho_`, `ghu_`, `ghs_`, `ghr_`, `ghp_`, `github_pat_`, `sk-`, `xox`, `AKIA`, JWT, PEM, `Bearer`), et un **jeu de 50 faits sans secret** contenant des nombres, `token`, `key`, `secret` en prose (« 9 084 tokens », « la clé `file_key` », « le secret de la vitesse ») ; la vue sous-agent de 3.4 | **Portes chiffrées :** (1) injections : 100 % des 8 familles de fixture retirées de la vue A+, 0 caractère de texte user tapé perdu sur les fixtures (égalité au fichier de référence) ; (2) rédaction : 100 % des valeurs de fixture masquées, **100 % des lignes porteuses conservées hors de la valeur** (la ligne « 9 084 tokens » est intacte), 0 des 50 faits sans secret modifié ; sur le corpus du poste (geste opérateur, F9), après rédaction : **0 des 3 clés `kleos_`, 0 des 4 valeurs `KLEOS_*_KEY`, 0 des 12 jetons `gh[ousr]_`, 0 des 51 secrets d'URL** de F17 retrouvés en clair, par les mêmes comptes que #32381 ; (3) détecteur du relais : 100 % des faits de fixture porteurs d'un secret bloqués, 0 des 50 faits sans secret bloqué ; (4) marqueurs : 100 % des stores de fixture avec id lisible rendus `{stored: id}`, le `send_message` triple rendu un original plus deux `{sent_to}` ; (5) `/rewind` de fixture : 0 fait de la branche abandonnée au statut valide ; (6) la ligne invalide produit un `{unparsable}` et le fichier continue ; l'enregistrement de 5 Mio est tronqué avec marqueur, celui de 300 Kio passe entier ; (7) sortie identique au fichier de référence pour `wire_purge_version` 1 |
| **2. Réception seule** | Endpoints 2.4 (révision 6 : avec `same_view`, `stale`, `epoch_reason`), jeton par poste, base ouverte par `connect_encrypted` (feature `sqlcipher` de kleos-lib), `uuid_index` avec `purge_version`, `lineage`, `sent_index`, curseur ; aucun traitement | Banc : copie de 50 transcripts, démon tué 20 fois au hasard puis relancé, réception redémarrée 5 fois ; contenu identique octet pour octet à un passage unique ; 0 enregistrement dupliqué ; 1 réécriture et 1 fork injectés traités selon 2.5 et 2.6 ; **révision 6** : 1 montée de vue injectée (bump de `wire_purge_version` sur 10 fichiers) traitée selon 2.8 : les 10 époques précédentes `superseded_by_view`, 10 lignées `reopen_for_view`, 0 `copy_of` posé sur les uuid du même `file_key`, un renvoi à version égale reçoit `409 same_view` ; une requête sans jeton reçoit `401` ; la base est illisible sans la clé |
| **3. Démon poste** | Scan par stat, `notify`, rédaction au jeton, calcul A+ (3.3), table `user_prefixes`, montée de vue au démarrage (2.8), tâche planifiée à l'ouverture de session, sans console, jeton par `cred exec` | Scan stat de 4 751 fichiers en moins de 2 s ; régime permanent sous 1 % CPU sur 1 h ; le banc du lot 2 passe avec le vrai démon ; **révision 6** : après un bump de `wire_purge_version`, seuls les fichiers listés par `stale` **et** présents sur le disque sont renvoyés, sous `epoch_reason = view_upgrade`, après les fichiers actifs, à 4 en parallèle au plus ; un fichier supprimé entre la liste et le renvoi est consigné, jamais bloquant ; le calcul A+ sur le fichier de 60 Mo (F1) tient en moins de 30 s (SUPPOSÉ atteignable) |
| **4. Traitement en ombre** | Fenêtrage, map, état de lignée et instantanés, stabilisation, `think: false` vérifié ; écriture dans une table du relais seulement, aucune écriture Kleos | Sur le corpus de calibration (section 5), jugement aveugle d'un tirage de 40 faits publiables par un juge indépendant (autre modèle que le juge amont, sans voir son verdict ; un échantillon opérateur reste facultatif, 4.7) : au moins 60 % utiles (contre environ 10 % aujourd'hui, F7) ; JSON valide après relance au moins 99 % ; 0 réponse acceptée avec `done_reason = length` ; une journée de flux traitée en moins de 2 h d'Ollama, mesurée en conditions réelles de partage ; au moins 95 % des faits annulés ou révisés dans la lignée absents de l'état final ; sur les sessions du corpus reprises après plus de 45 min, au moins 95 % des révisions faites dans la reprise appliquées à l'état hérité |
| **5. Juge amont, publication et juge nightly** | Détecteur de secrets (4.4, couche 0, révision 6), juge amont (4.4, couche 1), `source=kleos-transcript`, importance au plus 5, révision par chaîne de versions (4.3), règle d'archivage 4.6, règle de montée de vue (2.8), arbitrage des `<channel>` par `sent_index` (3.4). Aucune écriture en `pending`. Dépend de l'étape 2 de l'étude nightly 4.10 pour la couche 4 | Space : 100 % des sessions de fixture reçoivent le space de leur fichier `space_name`, ou celui de la règle 2 du hook en son absence, et 0 publication pour un `cwd` personnel ou racine (4.5) ; bande grise : bornes recalées sur la distribution du lot 0 (h), au moins 90 % des doublons de fixture entre 0,80 et 0,90 archivés côté transcript et 0 mémoire d'agent archivée ; fixtures du lot F : 100 % des faits inventés ou déformés non publiés, 100 % des faits fidèles et distincts publiés ; sur le corpus de calibration, au plus 5 % de faits publiés jugés faux par le juge indépendant aveugle sur un tirage de 40 ; second passage : 0 nouvelle mémoire ; 0 mémoire de transcript dans le tier important de `/recall` ni dans les statiques de `/context` ; sur une fixture où l'agent a stocké X et le transcript contient X, au moins 90 % des copies non publiées ou absorbées ; sur une fixture où un agent contredit après coup une mémoire de transcript, elle est archivée au passage nightly suivant et la mémoire d'agent est intacte ; une lignée rouverte produit une version 2 et aucune seconde tête ; une coupure simulée entre l'appel et l'inscription ne produit aucun doublon ; fréquence du sous-classement de 4.4 consignée ; **révision 6** : (1) **0 secret publié** : sur le corpus de calibration publié, les comptes de #32381 rejoués sur les mémoires `source=kleos-transcript` rendent 0 pour chacun des cinq formats, et 100 % des faits de fixture porteurs d'un secret sont retenus au relais ; (2) montée de vue sur une lignée de fixture avec 5 faits publiés, dont 1 révisé et 1 nouveau sous la vue large : 0 seconde publication d'un fait identique, 1 version 2, 1 publication neuve, 0 archivage par absence ; (3) `<channel>` : un message dont l'original est reçu par un autre démon de fixture n'est extrait que dans la session émettrice, un message d'émetteur `operator` est extrait dans la session réceptrice ; (4) `/rewind` : 0 fait d'une branche abandonnée publié valide, et un fait publié puis « rewindé » est archivé à la stabilisation suivante ; (5) liste négative : 100 % des faits de fixture recouvrant un `{stored: id}` rendus `doublon` sans publication |
| **6. Rétention et rattrapage borné** | Purge à 14 jours, index et correspondance (avec contenu publié) conservés, rattrapage de 14 jours | Après purge, `claude --resume` d'une session purgée : 0 réextraction ; volume disque du relais sous 1,5 Go (révisions 2 à 5, B) ; **révision 6 : sous 300 Mo** (A+ environ 130 Mo sur 14 jours, E6, plus index, correspondance et corps de `<channel>`) ; une montée de vue sur une lignée dont le contenu A+ a expiré mais dont la correspondance subsiste suit la table de 2.8 sans erreur |

---

## 9. Questions ouvertes pour l'opérateur

> **Révision 7 : état des questions.**
>
> | Question | État au 2026-10-24 |
> |---|---|
> | Q1 Niveaux de purge | Sans objet : la vue est calculée par `hypomnema-core` |
> | Q2 Autorité d'archivage sur les mémoires de transcript | **Ouverte**, reprise telle quelle par le consommateur (11.4) ; recommandation (a) inchangée |
> | Q3 Résumés de session | Tranchée par la topologie : les résumés vivent dans hypomnema (UI, CLI, MCP) ; aucune mémoire Kleos de résumé |
> | Q4 Rattrapage de l'historique | Tranchée (#33968) : team-lead et sans agent, du plus récent au plus ancien, `backfill_days` |
> | Q5 Emplacement du traitement serveur | Tranchée autrement : traitement dans `hypomnema-server`, consommateur Kleos séparé (11.1) |
> | Q6 Rétention de A+ au relais | Tranchée (#33942) : sans limite côté hypomnema ; pas de rotation des secrets de F17 (#33978) ; rebalayage hors v1 |
> | Q7 Arbitrage des `<channel>` | Tranchée : corps gardé à l'affichage, retiré de la vue présentée au modèle quand l'original est connu, par hypomnema |
> | Q8 Seuils du filtre structurel | Tranchée par mesure (F24) : 5 sessions et ratio au plus 0,2 ; table locale au poste |
> | Q9 Vue des sous-agents | Tranchée par mesure (F23) : `tool_result` d'agent du parent gardé entier |

**Q1. Niveaux de purge.** Stockage : (a) **B rédigé, recommandé** (idée du challenger,
environ 620 Mo sur 14 jours, surface de secrets traitée en 1.4) ; (b) A+ seulement, plus
petit mais figé au démon. Vue du modèle : **A+, recommandé**, environ 1 à 2 h d'Ollama
par jour ; B coûterait 4 à 8 h (3.2). Raison : le calcul serveur décide la vue, la
réversibilité décide le stockage.
**Tranchée par l'opérateur le 2026-10-22 : (b), A+ calculé au démon** (révision 6,
3.2). La réversibilité que (a) apportait est reprise par la montée de vue depuis la
source (2.8), bornée aux fichiers encore sur le poste, poste allumé. La vue du modèle
reste A+.

**Q6. Rétention de A+ au relais (révision 6).** (a) **14 jours, recommandé** : la borne
vient de la surface de secrets résiduels, pas du volume (environ 130 Mo) ; au-delà, le
retraitement passe par un renvoi depuis le poste, qui en garde 180 jours (F20). (b) 60
à 180 jours : un changement de prompt ou de modèle se rejoue sans renvoi ni dépendance
au poste, au prix d'une copie plus longue des secrets manqués par la rédaction. Question
liée : faut-il, à l'ajout d'un format de secret, rebalayer une fois les mémoires
`source=kleos-transcript` déjà publiées (lecture par `source_filter`, archivage
réversible des porteuses) ? Recommandation : oui, comme geste d'exploitation déclenché
par le bump de `secret_patterns_version`, pas comme passe nightly.

**Q7. Arbitrage des `<channel>` au relais (révision 6, 3.4).** Décision prise dans
cette révision sans mesure : le démon envoie le corps de tout `<channel>` dont
l'émetteur n'est pas `operator`, et le relais le retire de la vue à la stabilisation si
l'original est dans `sent_index`. (a) **Cette règle, recommandée** : aucun message sans
original n'est perdu, au prix du stockage 14 jours des corps de `<channel>` (volume
mesuré au lot 0 (k)). (b) Règle au démon seule, par liste d'émetteurs connus : plus
simple, mais le démon ne connaît pas le parc et un pair sans démon serait perdu. (c)
Marqueur seul pour tout émetteur sauf `operator` : perd les messages du Deck et des
pairs hors parc.

**Q8. Seuils du filtre structurel (révision 6, 3.3).** Préfixe de 40 caractères répété
dans au moins 5 sessions classé injection ; alerte de dérive quand la part de texte
user gardé double ou tombe à la moitié sur 100 sessions d'une nouvelle version de
Claude Code. Les deux seuils sont SUPPOSÉS ; le lot 0 (i) et (l) les calibre. Question :
la table des préfixes classés doit-elle être partagée entre postes par le relais (un
hook identique sur deux postes est classé deux fois plus vite), ou rester locale (un
préfixe propre à un poste n'en contamine pas un autre) ? Recommandation : locale en v1,
liste remontée au rapport.

**Q9. Vue des sous-agents (révision 6, 3.4).** Dernier message assistant et `tool_use`
Bash réussis, au minimum. Faut-il y ajouter le texte user initial du sous-agent (le
brief du parent, déjà présent dans le `tool_use` d'agent du parent, donc une copie) ?
Recommandation : non, par la règle « extraire là où c'est écrit ».

**Q2. Autorité d'archivage sur les mémoires de transcript (sans porte).**
(a) **Recommandé** : le juge nightly archive une mémoire de transcript contredite ou
remplacée par une mémoire d'agent plus récente (4.6), et le relais archive ses propres
publications annulées dans la même lignée (4.3). Deux autorités nouvelles, bornées à
`source=kleos-transcript`, réversibles par `unarchive`, sans aucun retrait de mémoire
d'agent.
(b) Annotation seulement : principe 4 appliqué aussi aux transcripts ; comme aucune
relecture humaine n'est possible (4.7), un fait faux qui a passé le juge amont reste
servi au plafond de 5 indéfiniment.
(c) Archivage sans juge, dès qu'une mémoire d'agent du même voisinage est plus récente :
moins cher, mais retire des faits distincts.
Sous-question liée : faut-il rendre la provenance visible dans le contenu lui-même (par
exemple un suffixe court), pour les rendus qui n'affichent que la catégorie (4.4,
couche 3) ? Le coût est un léger écart de simhash et d'embedding face à un store
d'agent identique ; recommandation : non en v1, à revoir si le lot 5 montre des agents
qui citent des mémoires de transcript comme des faits établis.

**Q3. Résumés de session.** (a) **Restent dans le relais, recommandé** ; (b) une mémoire
`state` par session, au risque de concurrencer les faits au classement (F7) ; (c) un
handoff Kleos par session, ce qui recouvre les handoffs écrits par les agents.

**Q4. Rattrapage de l'historique.** (a) Aucun ; (b) **14 jours, d'abord en calibration,
recommandé** (9 à 17 h d'Ollama étalées) ; (c) les 7 Go : environ 20 à 35 h de plus et
un afflux d'anciens faits au juge, pour un gain non mesuré.

**Q5. Emplacement du traitement serveur : tranché par le consultant (section 1.3).**
R : relais séparé dans un dépôt séparé, dépendant de kleos-lib par un rev git épinglé
sur le fork VOCSAP, hors des `members` du `Cargo.toml` racine. **Validé par l'opérateur
le 2026-10-21**, ainsi que la règle de bump : après chaque merge du fork et un build
vert, le rev épinglé suit `418cdd26` ou un commit postérieur dès que le relais existe,
avec le test d'intégration jetable de 1.3. Il reste antérieur à `f1db1cf7` tant que le
gel ELv2 décidé le 2026-10-22 tient (Kleos #32284). Voir [Gel upstream](../dev-notes/local-patches.md#gel-upstream).
La variante (a') de la dédup,
validée un temps, est abandonnée avec la porte (F13, carte 6ee2297f en wontfix). S et
la réécriture sur place de kleos-ingest sont écartés.

---

## 10. Références

- Étude nightly : `docs/studies/2026-10-20-kleos-nightly-pipeline.md`, sections 3, 4.1 à
  4.10, 6, 8 et 10.
- Kleos : #31653, #31639, #31658, #31675, #31676, #31682, #31699, #31706, #31707,
  #31716, #31720, #31722, #31725, #31729, #31730, #30326, #18425, #30938 ; révision 6 :
  #32359 (lexique de `scrub_credentials` et remplacement de ligne), #32381 (comptes de
  secrets et d'injections sur le corpus du poste), #32284 (gel upstream).
- Poste : `~/.claude/settings.json:3` (`cleanupPeriodDays: 180`, F20).
- Dédup de fond et space : `kleos-lib/src/intelligence/duplicates.rs` ;
  `kleos-lib/src/intelligence/scheduler.rs:1-60`, `:200-270`, `:330-370` ;
  `kleos-lib/src/intelligence/linker.rs:1-60` ; `kleos-server/src/dreamer.rs:170-230`,
  `:265-300` ; `kleos-config/src/config.rs:146-175`, `:735-750`, `:900-912` ;
  `kleos-cli/src/space.rs` (signature) ; `~/.claude/hooks/lib-kleos-space.sh` ;
  `~/.claude/hooks/session-start-kleos.sh:71-86`.
- Code lu : `kleos-ingest/src/{tailer,writer,ledger}.rs` ;
  `kleos-lib/src/conversations.rs` ; `kleos-server/src/routes/conversations/mod.rs` ;
  `kleos-server/src/routes/memory/mod.rs:60-147` ;
  `kleos-lib/src/memory/mod.rs:560-660`, `:775-836`, `:850-895` ;
  `kleos-lib/src/memory/types.rs:300-342` ; `kleos-lib/src/inbox.rs` (inbox, cliente :
  interface web GUI) ; `kleos-lib/src/memory/search.rs:1090-1125`, `:1535-1575` ;
  `kleos-lib/src/memory/mod.rs:1255-1360` ; `kleos-server/src/routes/memory/mod.rs:40-63`,
  `:681`, `:760-775` ; `kleos-lib/src/context/mod.rs:590-603` ;
  `kleos-lib/src/context/deps.rs:117` ; `docs/dev-notes/local-patches.md:6594-6621`
  (Patch 60) ;
  `kleos-lib/src/prompts.rs:255-289` ; `kleos-lib/src/llm/mod.rs:40-50` ;
  `kleos-lib/src/llm/prompts.rs:95-100` ; `kleos-lib/src/db/mod.rs:55-66` ;
  `kleos-lib/src/lib.rs` (modules publics) ; `kleos-lib/Cargo.toml:30` ;
  `kleos-ingest/Cargo.toml:26` ; `Cargo.toml:3` ; `docs/dev-notes/local-patches.md:125` ;
  `kleos-lib/src/db/schema_sql.rs:95` (index `sync_id` non unique : le relais ne peut pas
  s'appuyer sur `sync_id` pour l'idempotence des publications, DÉDUIT).
- Tables upstream `conversations` / `messages`, écartées comme stockage de travail en
  révision 1 et toujours écartées : `upsert_conversation` ajoute chaque message sans clé
  d'idempotence (DÉDUIT, `kleos-lib/src/conversations.rs:712-774`) et `memorize` fait UNE
  mémoire par conversation (DÉDUIT, `kleos-server/src/routes/conversations/mod.rs:274-313`).
- Révision 7 : conception hypomnema, `VOCSAP/hypomnema`,
  `docs/superpowers/specs/2026-10-24-hypomnema-design.md` (commit `009d6b6`, puis
  `7db51d0`, puis `ef02cee`, puis `0b069bb`, puis `e99b9a2`, version du contrat du §10
  citée par la section 11), sections
  3, 4, 5, 6, 7.1, 7.4, 10, 12 et annexe A ;
  Kleos #33908 (préfixe forké), #33942 (topologie), #33945 (dépôt), #33962 (space côté
  poste), #33968 (portée du map), #33978 (pas de rotation), #33981 (lot 0 d'hypomnema) ;
  hook `~/.claude/hooks/session-start-kleos-label.sh` et son test
  `hooks/tests/test-session-start-kleos-label.py` (dépôt claude-config) ;
  `kleos-lib/src/auth.rs:62`, `:159-161`, `:335-343` (portées de clés) ;
  `kleos-lib/src/llm/mod.rs:45`, `:62`, `:76` (coupure du raisonnement).

---

## 11. Consommateur Kleos (révision 7)

Cette section fait foi pour tout ce qui reste à Kleos. Le contrat d'entrée est le §10 de
la conception hypomnema, au commit `e99b9a2` ; en cas d'écart entre les deux textes, le §10 d'hypomnema
l'emporte pour la forme du flux, et cette section pour ce que Kleos en fait.

Depuis `ef02cee`, l'annexe A de la conception hypomnema reformule les sections 1.4, 2.1 à
2.8, 3.3, 3.4 et 4.1 à 4.3 de cette étude, qu'hypomnema reprenait jusque-là par renvoi.
Pour tout ce qui relève d'hypomnema, la conception fait foi, notamment sur quatre points
où elle diverge de cette étude :
- balayage `stat` de repli toutes les 60 s, au lieu de 10 min ;
- fenêtre du map calculée par le tokenizer du modèle branché, au lieu de 36 000 caractères fixes ;
- un changement de modèle ou de prompt ne vaut que pour les fenêtres à venir, sauf `remap`
  sur commande, au lieu de rouvrir les lignées ;
- préfixe classé injection : 5 sessions **et** un ratio d'au plus 0,2, au lieu de 5 sessions.

Le lot 3 d'hypomnema (`0b069bb`) précise quatre points du contrat, repris ci-dessous :
- chaque étiquette de `session_meta` porte sa `source`, `hook` ou `marker_file` (11.2, 11.3) ;
- toute session qui copie une autre ouvre sa propre branche, et un changement de
  propriétaire du bloc commun émet `branch_root_changed` (11.2, 11.4) ;
- une époque remplacée n'est jamais supprimée tant qu'aucun consommateur n'est enregistré
  (11.2) ;
- le fichier de motifs livré s'appelle `patterns/redaction-patterns.toml` (11.6).

Les fixtures du flux écrites au lot K0 (`kleos-transcript-consumer/tests/fixtures/feed/`)
sont copiées octet pour octet dans le dépôt hypomnema, où un test de conformité compare
aux leurs les clés de chaque événement émis et les valeurs de `source`.

### 11.1 Place et forme

- **Processus séparé** de kleos-server et de `hypomnema-server`, sur LXC 121, binaire
  Rust qui dépend de kleos-lib (`think_setting`, `load_prompt`, `connect_encrypted`,
  chargeur du lexique). Nom provisoire : `kleos-transcript-consumer`.
- **Base propre**, SQLCipher : curseur du flux, état candidat des faits par
  `(lineage_id, fact_id)`, la branche en attribut (Q12), correspondance vers `(memory_id, version)`,
  conflits retenus, file `to_dedup` (4.8), refus de la couche 0. La correspondance est
  gardée sans limite ; le reste suit la rétention d'hypomnema.
- **Deux jetons**, tous deux dans un `EnvironmentFile` 0600 : un jeton de lecture
  hypomnema dédié au consommateur, et une clé API Kleos à portée écriture, envoyée en
  `Authorization: Bearer`.
- **Le verdict R de 1.3, relu.** Sa force décisive, E9 (le dreamer modifié à chaque merge
  upstream), est annulée par le gel du 2026-10-22. Le processus séparé reste retenu pour
  une autre raison : une panne ou une fuite du consommateur ne touche pas le processus
  qui sert les agents. La condition de renversement 2 (transaction SQLite partagée entre
  le juge et le stockage) reste non remplie : le consommateur ne partage aucune base.
- **Emplacement du code : crate membre du workspace Kleos** (Q10, tranchée par
  l'opérateur le 2026-10-25). Le gel retire E10 (conflit sur la ligne `members`) et la
  règle de bump d'un rev épinglé disparaît ; le crate dépend de kleos-lib par chemin.

### 11.2 Entrée : le flux d'hypomnema

`GET /v1/feed?after=<seq>&limit=N`, puis `POST /v1/feed/ack {consumer, seq}` après le
traitement **transactionnel** de chaque événement dans la base du consommateur. Un
événement rejoué rend le même contenu, et son traitement est idempotent : un arrêt
brutal entre le traitement et l'acquittement produit au pire un rejeu sans effet.

| Événement | Effet côté Kleos |
|---|---|
| `session_meta` | Instantané complet : étiquettes avec leur `source` (`hook`, `marker_file`), `git_root`, `host_id`, `file_key`, versions, `parent_session` ; résolution du space (11.3) |
| `epoch_opened`, `epoch_superseded` | Trace seulement. Hypomnema supprime une époque remplacée quand tous les consommateurs enregistrés ont acquitté son `epoch_superseded`, ou 30 jours après le remplacement (`HYPOMNEMA_SUPERSEDED_GRACE_DAYS`), et jamais tant qu'aucun consommateur n'est enregistré. Un consommateur arrêté plus longtemps que ce délai perd la correspondance des faits de ces époques |
| `facts_updated` | Mise à jour de l'état candidat : `fact_id` stable, `branch_id`, version, statut (`proposé`, `décidé`, `mesuré`, `abandonné`), catégorie Kleos, importance proposée, uuid des tours sources, drapeau `abandoned` (`/rewind`), `map_version` |
| `lineage_stabilized {lineage_id, branch_id}` | Publication de la branche (11.4) ; une branche sœur encore vivante n'est pas touchée |
| `lineage_reopened` | La branche passe en révision : ses faits publiés seront comparés à la prochaine stabilisation |
| `lineage_merged {from, into}` | Réécriture de la correspondance de `from` vers `into` ; aucune republication |
| `branch_root_changed {lineage_id, from, to}` | Le bloc commun change de propriétaire, par exemple quand un ancêtre arrive après sa copie : la racine passe de `b0` (la copie) à `b1` (l'ancêtre). Ne changent de branche que les faits dont toutes les sources sont dans le bloc commun ; ceux de la partie vivante de `from` y restent. Réécriture de leur attribut de branche de `from` vers `to`, aucune republication ; aucun `facts_updated` ne suit, leur contenu et leur version ne changent pas. Au lot 6 d'hypomnema, l'événement gagne `moved_fact_ids`, qui les liste ; avant, le consommateur ne sait pas lesquels bougent (Q12) |
| `remap {motif}` | Comparaison selon la table de 2.8 : rien pour un fait identique, version pour un fait révisé, publication pour un fait distinct, **aucun archivage pour un fait absent** |

**Garde de version, échec fermé.** Un événement `facts_updated` dont la
`secret_patterns_rev` est inférieure à la révision minimale configurée n'entre pas dans
l'état candidat : il est consigné et acquitté, pour que le flux avance. La ré-extraction
par hypomnema produit ensuite un `remap` de motif `patterns_upgrade` sous une révision
suffisante. `secret_patterns_rev` numérote l'ensemble de motifs **livré**, celui que le
consommateur épingle ; `secret_patterns_hash`, empreinte de l'ensemble effectif (livré
plus motifs de l'utilisateur), n'entre pas dans la garde : un motif utilisateur produit
une empreinte que le consommateur ne peut pas connaître.

La route `GET /v1/feed` reste fermée jusqu'après le lot 6 d'hypomnema ; depuis le lot 3,
hypomnema écrit déjà dans sa table `feed` les événements `session_meta`, `epoch_opened`,
`epoch_superseded`, `lineage_merged` et `branch_root_changed`. Le flux vit dans la base
de chaque locataire : un consommateur lit le flux d'un seul locataire.

**Lecture des tours sources** pour le juge amont :
`GET /v1/sessions/{id}/read?around=<uuid>`. Le consommateur ne lit jamais les
enregistrements A+ en masse.

### 11.3 Space d'une session

Le consommateur ne voit pas le disque du poste (Kleos #33962). Ordre de résolution, sur
les seules données reçues :

1. **Étiquette `kleos-space`**, émise par le hook dédié
   `~/.claude/hooks/session-start-kleos-label.sh` (carte `6322672e`). Forme figée avec
   hypomnema : l'`additionalContext` entier du hook vaut `<<label kleos-space=<nom>
   /label>>`, motif `^<<label kleos-space=[a-z0-9_-]{1,64} /label>>$`. Hypomnema le lit
   par égalité stricte (espaces de bord retirés) dans une seule structure : une ligne
   `attachment` de type `hook_success`, `hookEvent = SessionStart`, dont le `command`
   désigne ce hook et dont le `stdout` JSON porte `hookSpecificOutput.additionalContext`.
   Aucun repli sur un élément de `hook_additional_context` : sur 3 807 transcripts,
   aucune de ces lignes ne partage son `toolUseID` avec un `hook_success` (Kleos #36070),
   et un tel élément ne peut donc pas être attribué à un hook. Une session sans
   `hook_success` n'a pas d'étiquette de hook et passe à l'étape 2. Dans `session_meta`,
   cette étiquette porte `source = hook`. Un hook dédié,
   et non une ligne de plus dans `session-start-kleos.sh`, parce que ce dernier réinjecte
   des mémoires : une mémoire qui cite le format donnerait une fausse étiquette avec une
   recherche par ligne. Le nom est celui de `resolve_project_name`, la même règle que le
   hook principal, sans créer de marqueur.
2. **Étiquette de fichier marqueur** `.kleos-space`, lue sur le poste par
   `hypomnema-agent`, pour les sessions antérieures au hook, normalisée comme
   `normalize_space_name` ; `source = marker_file` dans `session_meta`. Le format
   d'échange la prévoit depuis le lot 3 d'hypomnema ; son producteur, l'agent qui lit
   `.kleos-space`, arrive avec le lot 2.
3. **Nom du dépôt** de `git_root`, normalisé de même.
4. Sinon, aucun space.

Une session de sous-agent prend le space de sa `parent_session`. Un nom qui désigne le
dossier personnel ou une racine, ou l'absence de space : faits gardés, non publiés
(4.5). Une étiquette qui diverge du marqueur : l'étiquette l'emporte, l'écart est
consigné.

### 11.4 Publication et révision

**Ce qui devient une mémoire** (repris de 4.3) : un fait de l'état stabilisé d'une
branche, au statut `décidé` ou `mesuré`, non `abandoned`, qui passe la couche 0 puis la
couche 1 (4.4). La **liste négative** du juge amont est désormais construite par le
consommateur : les marqueurs `{stored: id}` de la session, résolus par
`GET /memory/{id}` ; un fait qui recouvre un store de l'agent est rendu `doublon` sans
publication.

**Champs de la publication** (4.5) : `source=kleos-transcript` ; catégorie du fait ;
importance `min(proposée, 5)` ; `created_at` = `ts` du tour source ; `session_id` ;
`space` (11.3) ; tags `transcript`, le projet, `turn:<uuid>`,
`fact:<lineage8>-<fact_id>` (sans la branche, depuis la révision 7.2 : Q12).

**Révision par branche** (point 3 de 4.3). F21 rend les branches sœurs fréquentes : le
bloc commun appartient à une branche racine, et chaque branche révise ses faits de son
côté. Une révision d'une branche sur un fait publié depuis la racine suit la chaîne de
versions si la tête connue est intacte ; la seconde branche qui révise le même fait
rencontre le refus de parent périmé (F12) et publie une mémoire neuve, que le juge
nightly annote (4.6). Ce cas, nommé « jamais observé » en révision 6, est désormais un
chemin nominal, à mesurer au lot K3.

Depuis `0b069bb`, toute session qui copie une autre ouvre sa propre branche, même quand
elle l'inclut strictement : une reprise ou un fork ne prolonge jamais la branche de
l'ancêtre. La session racine porte `b0`, et un `branch_id` émis ne change jamais. Toute
révision d'un fait du bloc commun par une session reprise passe donc par le chemin
ci-dessus, et pas seulement celle d'une branche sœur.

**Inchangés :** idempotence par l'état `intention` et le tag `fact:` (4.3) ; règle
d'autorité, le consommateur ne révise et n'archive que ses propres publications ; couches
2 à 5 de 4.4 ; juge nightly (4.6) ; dédup de fond (4.8) ; question Q2 ouverte.

### 11.5 Porte de publication

La qualité de l'extraction ne dépend plus de Kleos : le modèle du map est branché par
l'utilisateur d'hypomnema (LiteLLM ou Ollama, aucun défaut). Le consommateur tourne donc
**en ombre**, sans écrire dans Kleos, tant que sa porte n'est pas passée, et y retourne
après tout changement de `map_version` qui change de modèle. Porte, reprise des lots 4 et
5 de la section 8 :

- au moins 60 % de faits publiables jugés utiles, au plus 5 % jugés faux, sur un tirage
  de 40 jugé à l'aveugle par un juge indépendant (autre modèle que le juge amont) ;
- au moins 99 % de JSON valide après relance côté hypomnema, et 0 réponse tronquée
  acceptée (lu dans le flux et dans `/v1/health`) ;
- 0 secret publié sur le corpus de calibration, par les comptes de #32381 ;
- les fixtures du lot F (2) : 100 % des faits inventés ou déformés non publiés, 100 %
  des faits fidèles et distincts publiés.

### 11.6 Lots Kleos

| Lot | Contenu | Porte mesurable | Dépend de |
|---|---|---|---|
| **K0 Gel** | Contrat du juge amont (lot F (2)) ; liste des champs de `StoreRequest` et test d'intégration jetable (lot F (4)) ; fixtures d'événements du flux, relues contre le §10 d'hypomnema ; squelette du crate dans le workspace (Q10) | Schéma de verdict et fixtures écrits ; test jetable vert ; chaque type d'événement du §10 a au moins une fixture | Rien |
| **K1 Lecture du flux, en ombre** | Enregistrement du consommateur (Q11), curseur, acquittement dont celui de `epoch_superseded`, état candidat, `lineage_merged`, `branch_root_changed`, garde de version, résolution du space | Flux de fixture rejoué deux fois : état identique ; 20 arrêts brutaux au hasard : 0 perte, 0 doublon ; 100 % des sessions de fixture reçoivent le space attendu par l'ordre de 11.3, 0 pour un `cwd` personnel ou racine ; après `branch_root_changed` de fixture, la correspondance du bloc commun est sous `to` et 0 republication | Lot 3 d'hypomnema (stockage, livré à `0b069bb`) ; route de flux, après le lot 6 |
| **K2 Couches 0 et 1, en ombre** | Détecteur de secrets sur le fichier de motifs épinglé (`patterns/redaction-patterns.toml` d'hypomnema et son fichier de verrou) ; juge amont avec liste négative et tours sources lus par `read?around` | Fixtures de secrets : 100 % bloqués, 0 des 50 faits sans secret bloqué ; porte de 11.5 sur le corpus de calibration | K1 ; lot 6 d'hypomnema (map) |
| **K3 Publication** | Écriture dans Kleos, révision par branche, `remap`, idempotence | Portes du lot 5 de la section 8 qui restent à Kleos : second passage, 0 nouvelle mémoire ; 0 mémoire de transcript dans les tiers poussés ; lignée rouverte : une version 2 et aucune seconde tête ; coupure simulée : 0 doublon ; `remap` : 0 archivage par absence ; branches sœurs de fixture : une version et une mémoire neuve annotée | K2 passé |
| **K4 Nettoyage a posteriori** | Juge nightly borné aux mémoires de transcript (4.6) ; bande grise (4.8) | Celles du lot 5 pour la bande grise et la contradiction après coup | K3 ; étape 2 de l'étude nightly |

**Ordre.** K0 peut commencer tout de suite. K1 attend la route de flux d'hypomnema.
Décision opérateur du 2026-10-25 : cette route peut passer juste après le lot 6
d'hypomnema plutôt qu'au lot 10 ; sa place exacte se fixe au fil des lots, selon ce
qu'ils découvrent.

### 11.7 Questions ouvertes (révision 7)

**Q10. Emplacement du consommateur. Tranchée le 2026-10-25 : (a).** (a) **Crate du
workspace Kleos** :
dépendance à kleos-lib par chemin, plus de bump à tenir, et le gel retire le risque de
conflit sur `members` (E10). (b) Dépôt séparé sur un rev épinglé, comme le prévoyait la
révision 3 : isole davantage, au prix du bump et du test jetable à chaque changement de
kleos-lib. (c) Module d'hypomnema : écarté, hypomnema ne doit dépendre ni de l'API de
Kleos ni de ses décisions de publication.

**Q2** (autorité d'archivage) reste ouverte telle quelle.

**Q11. Enregistrement d'un consommateur. Tranchée par hypomnema le 2026-10-26** (choix
du concepteur, écrit dans son §10 à `e99b9a2`). L'inscription se fait seulement par
la commande d'administration `hypomnema-server feed consumer add --tenant <t> <nom>`,
jamais implicitement : ni par un premier `POST /v1/feed/ack`, ni par la création d'un
jeton. La purge ne change donc de comportement que par un acte de l'opérateur. Un
consommateur inscrit part d'un acquittement à 0 ; l'ack d'un consommateur non inscrit
reçoit `404 unknown_consumer`, et le refus est compté. Commande et route arrivent avec
l'ouverture du flux, après le lot 6. Pour K1 : l'inscription est une étape du
déploiement, et un `404 unknown_consumer` arrête le consommateur au lieu de boucler.

**Q12. Faits touchés par `branch_root_changed`. Tranchée par hypomnema le 2026-10-26**
(choix du concepteur). L'identité d'un fait est `(lineage_id, fact_id)` : les `fact_id`
sont attribués par lignée, sans collision entre branches, et `branch_id` est un attribut
du fait (la branche qui détient ses sources), pas une part de son identité. Ne bougent
que les faits dont toutes les sources sont dans le bloc commun ; `moved_fact_ids` les
listera au lot 6 ; aucun `facts_updated` n'est réémis. Conséquence pour Kleos :
l'état candidat est indexé par `(lineage_id, fact_id)` (11.1). Le tag figé au lot K0,
`fact:<lineage8>-<branch_id>-<fact_id>`, change à chaque changement de racine : un fait
déplacé aurait un tag différent de celui de sa mémoire déjà publiée, et l'idempotence
par le tag (4.3) le republierait. Décision Kleos : le tag devient
`fact:<lineage8>-<fact_id>`, sans la branche (spec agent-forge `spec_f4900e5a`).
