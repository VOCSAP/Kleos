# Étude d'architecture : cible de l'ingestion des transcripts Claude Code

Statut : **conception seulement, aucun code livré**. Révision 3 du 2026-10-21 : la
révision 2 répondait à un challenge au verdict « tombe » et à trois mesures (T1 à T3,
section 0) ; la révision 3 intègre le verdict du consultant sur l'embranchement de la
section 1.3. Décisions opérateur du 2026-10-21 : R, règle de bump et variante (a')
validées (Q5, 4.4). Cette étude détaille la section 4.3 de
l'[étude nightly](2026-10-20-kleos-nightly-pipeline.md) (« Transcripts en attente :
extension distincte ») et en **amende un point** : la zone `transcript_staging` ne vit
pas dans Kleos (overlay Patch 41) mais dans un relais à côté du serveur, dépôt séparé
dépendant de kleos-lib par un rev épinglé (tranché par le consultant, section 1.3). Le reste
de l'étude nightly (juge de voisinage, état de traitement, principe « un faux retrait
est pire qu'un retrait manqué ») s'applique tel quel aux mémoires issues des
transcripts.

Cartes liées : `f18dc3c4` (mise en service), `20d18577` (reprise après arrêt brutal),
`3f45e0f8` (portage Windows, livré), `19671bc8` (dry-run du watcher), `6ee2297f`
(dédup et porte d'attente, ouverte par T1).

### Ce que les révisions 2 et 3 changent

| Point du challenge ou du consultant | Réponse | Section |
|---|---|---|
| (1) Une ligne `pending` absorbe les stores ultérieurs de l'agent | Parade : correctif chirurgical de la dédup, candidat PR ; tant qu'il n'est pas livré, la porte reste éteinte et rien n'écrit dans Kleos | 4.4 |
| (2) La règle « non révisé après 45 min » ne tient pas à la reprise | Parade : état par lignée hérité à la reprise, publication à la stabilisation (8 h), révision des faits publiés par la chaîne de versions de Kleos | 4.2, 4.3 |
| (3) Coût du relais sous-estimé : secrets, chiffrement, authentification | Parade pour chacun, coût chiffré ; limite nommée : la rédaction des secrets est incomplète par nature | 1.4 |
| (4) Le budget Ollama ignore `KLEOS_LLM_THINK` | Parade : `think: false` explicite et rejet des réponses tronquées ; budget recalculé sur T2 | 3.2, 4.2 |
| Idée du challenger : stocker B, ne présenter que A+ au modèle | Retenue | 3.2 |
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
| **T1** | Porte active, `KLEOS_REVIEW_GATE_SOURCES=kleos-transcript` : un store `claude-session` quasi identique à une ligne `pending` rend `created=false, duplicate_of=<id pending>` ; aucune ligne approuvée n'est créée | MESURÉ (rapporté, #31707, test d'intégration jetable) |
| T1-a | Cause : la requête de dédup exclut `is_archived = 1` et `status = 'rejected'`, pas `pending` ; le chemin doublon rend l'id existant sans toucher la ligne | DÉDUIT (`kleos-lib/src/memory/mod.rs:586-590`, `:607-616`) |
| T1-b | En production, `/etc/kleos/kleos.env` ne contient aucune variable `REVIEW_GATE` ; `KLEOS_REVIEW_GATE_ENABLED` vaut `false` par défaut : **la porte est éteinte aujourd'hui** | MESURÉ (rapporté, brief du lead) ; défaut DÉDUIT (`mod.rs:777-781`) |
| **T2** | `qwen3:8b-ctx16k` par `/api/chat`, fenêtre de 36 000 caractères = 9 084 tokens, raisonnement coupé : ingestion du prompt à froid 6,2 s (environ 1 470 tokens/s), génération environ 49 tokens/s ; environ 12 s pour 300 tokens de sortie, 22 s pour 800. Un préfixe identique est servi par le cache KV (18 à 46 ms), donc un appel répété sous-estime le coût réel. Avec `think: true` et `num_predict` 800, le budget part dans le raisonnement et l'appel s'arrête en `length` | MESURÉ (rapporté, #31706) |
| T2-a | La production pose `KLEOS_LLM_THINK=0`, mais cette variable est lue par kleos-server (Patch 14) ; un processus séparé n'en hérite pas | MESURÉ (rapporté, brief du lead) ; héritage DÉDUIT (`CLAUDE.local.md`, section Cible LLM) |
| **T3** | 687 sessions actives sur 14 jours : 28,97 % ont au moins un écart de plus de 45 min suivi d'autres tours ; 18,05 % à 60 min, 7,71 % à 2 h, 0,73 % à 8 h. En ne comptant que les prompts tapés par un humain : 14,41 % à 45 min, 0,58 % à 2 h. La moitié des reprises sont déclenchées par un message injecté (canal peer, notifications). Les 355 écarts de plus de 45 min : médiane 55 min, p90 170 min | MESURÉ (rapporté, #31716) |
| T3-a | Lignée resume/fork : 6 copies sur 687 sessions ; la copie démarre p50 16 min après la source ; 2 cas sur 7 au-delà de 45 min ; aucune source n'a continué après le fork | MESURÉ (rapporté, brief du lead ; le brief donne 6 copies et 7 cas, écart non résolu ici) |
| F11 | `POST /memory` désérialise `StoreRequest` tel quel : `parent_memory_id`, `created_at`, `space`, `session_id`, `tags` et `sync_id` passent par l'API publique ; corps limité à 256 Kio | DÉDUIT (`kleos-server/src/routes/memory/mod.rs:74`, `:110-136` ; `kleos-lib/src/memory/types.rs:306-342`) |
| F12 | Un store avec `parent_memory_id` crée la version suivante, passe l'ancienne à `is_latest = 0`, saute la dédup, et **refuse** (`Conflict`) si le parent n'est plus la tête de chaîne | DÉDUIT (`mod.rs:578`, `:855-890`) |

---

## 1. Composants et frontières

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
  D -- "segments B rédigés<br/>Bearer par poste" --> R
  R -- "vue A+ <= 16k<br/>think: false" --> O
  K -- "juge nightly<br/>(étude 4.6)" --> K
```

### 1.2 Qui détient quoi

| Composant | Détient | Ne détient jamais | Écart upstream (table CLAUDE.md) |
|---|---|---|---|
| Claude Code | Les transcripts bruts, seule copie complète | | hors périmètre |
| Démon poste `kleos-transcript-agent` | Curseur par fichier (offset acquitté, époque, empreintes), version de purge du fil. **Aucune copie** du transcript : le fichier source sert de tampon | Contenu purgé persistant, mémoires, appel LLM | **Zéro delta** si dépôt séparé (Q5) |
| Relais `kleos-transcript-relay` | Enregistrements B rédigés (rétention 14 jours), index des uuid (longue durée), état de lignée, résumés de fenêtre et de session, faits candidats, correspondance fait vers id et version Kleos | Les mémoires canoniques, l'autorité de retrait sur une mémoire qu'il n'a pas écrite | Option R : **zéro delta**. Option S : voir 1.3 |
| kleos-server | Mémoires canoniques, statut `pending`/`approved`, chaîne de versions, voisinage et annotations de l'étude nightly | Le transcript, même réduit (option R) | Correctif chirurgical de la dédup requis avant la porte (4.4), dans les deux options |
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
le fenêtrage et la règle de publication sont identiques dans R et S. Le correctif de
dédup de la section 4.4 touche `memory::store` et s'impose dans les deux. Le démon est
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
| E6 | Volume stocké en B : environ 44 Mo par jour, soit environ 620 Mo sur 14 jours, plus l'index des uuid | DÉDUIT (F1, F3) | R : hors de `kleos.db` ; S : dans la base principale, sa WAL et ses sauvegardes |
| E7 | Chiffrement au repos et rédaction : `Database::connect_encrypted` applique une clé de 32 octets par `PRAGMA key` (`kleos-lib/src/db/mod.rs:57-65`), avec la feature `sqlcipher` de kleos-lib (`kleos-lib/Cargo.toml:30`) ; `prompts::scrub_credentials` est public (`kleos-lib/src/prompts.rs:262`, module `pub mod prompts`, `lib.rs:48`). L'authentification démon vers relais reste à fournir par R (1.4) | DÉDUIT (lignes lues). Chiffrement de `kleos.db` en production SUPPOSÉ, non vérifié non plus par le consultant | **Neutre** pour le chiffrement et la rédaction ; S garde l'avantage sur l'authentification seulement |
| E8 | Les routes mémoire limitent le corps à 256 Kio ; la réception de segments demande sa propre route et sa propre limite | DÉDUIT (`routes/memory/mod.rs:74`) | neutre : route neuve dans les deux options |
| E9 | **Force décisive.** Upstream a touché `kleos-server/src/dreamer.rs` dans chacun des 4 merges de juin à août (`50e03774`, `61fd2acf`, `cb1bff54`, `733a9466`), et `docs/dev-notes/local-patches.md:125` classe déjà ce fichier « ELEVE (2 blocs) ». L'API publique de kleos-lib a bougé aussi (`llm/mod.rs` : `f857f7cd`, `2a6f0f9a`, `7e207b90`), mais bien moins que l'intérieur du dreamer | Ligne `local-patches.md:125` DÉDUITE (lue) ; liste des commits MESURÉE (rapporté, consultant) | R : dépend de l'API publique de kleos-lib, pas de l'intérieur du dreamer |
| E10 | La ligne `members` du `Cargo.toml` racine liste 26 crates sur une seule ligne ; upstream l'a modifiée 8 fois en deux mois | Forme DÉDUITE (`Cargo.toml:3`) ; fréquence MESURÉE (rapporté, consultant) | R hors workspace : un crate ajouté à cette ligne serait un conflit à presque chaque pull |

**Coût de rebase.**

| | R, relais séparé | S, staging dans Kleos |
|---|---|---|
| Fichiers upstream touchés | Aucun (hors correctif 4.4, commun) ; pas d'entrée dans `members` (E10) | Enregistrement des routes dans le routeur serveur et branchement d'un job dans le dreamer : deux patchs chirurgicaux, dont un dans le fichier classé « ELEVE » (E9) |
| Tables | Base propre | Overlay Patch 41 : zéro delta par construction |
| Coût récurrent | Un bump du rev épinglé de kleos-lib après chaque merge upstream du fork ; ne casse que si l'API publique utilisée change (`think_setting`, `load_prompt`, `connect_encrypted`, `scrub_credentials`) ou si l'API HTTP `/memory` change | Chaque pull upstream qui modifie le dreamer, le client LLM ou la signature de `memory::store` casse la compilation du module, même sans conflit de texte ; coût proportionnel à la cadence upstream (E9) |
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
  2. Le correctif (a') de la dédup (section 4.4).
  3. Le rev épinglé de kleos-lib et sa règle de bump : à chaque merge upstream du fork,
     après un build vert du fork.
  4. La liste des champs de `StoreRequest` utilisés par le relais (`content`,
     `category`, `source`, `importance`, `tags`, `session_id`, `space`,
     `parent_memory_id`, `created_at`), avec un test d'intégration jetable rejoué à
     chaque bump.
- **Non vérifié par le consultant, et toujours SUPPOSÉ ici :** l'existence de `/api/ps`
  sur la version d'Ollama déployée (4.1), et le chiffrement de `kleos.db` en production
  (E7).

**Conséquence sur la réversibilité.** Comme R appelle les mêmes fonctions de kleos-lib
que S appellerait, la bascule R vers S ne réécrit plus de client LLM ni de chiffrement :
l'asymétrie en faveur de R s'accentue (DÉDUIT).

### 1.4 Sécurité et exploitation du relais (point 3 du challenge)

La décision opérateur du 2026-10-20 dit que la sécurité des transcripts n'est pas un
frein, parce que les sessions Claude Code détiennent déjà tous les secrets lisibles
(MESURÉ rapporté, #30938). Le relais ajoute pourtant **une seconde copie sur une
seconde machine**, et le stockage en B (section 3.2) garde les `tool_result`, donc des
sorties de commandes et des contenus de fichiers. Chaque manque nommé par le challenge
reçoit une parade.

| Manque | Parade | Coût, SUPPOSÉ | Limite restante |
|---|---|---|---|
| Purge des secrets | **Rédaction à la source**, dans le démon, avant tout envoi : (1) formats de jetons connus (`ghp_`, `github_pat_`, `sk-`, `xox[bpas]-`, `AKIA`, JWT `eyJ…`, blocs PEM, en-têtes `Authorization`/`Bearer`) ; (2) passe par mots-clés sur la ligne, **en appelant `kleos_lib::prompts::scrub_credentials`** par la dépendance épinglée (DÉDUIT : `kleos-lib/src/prompts.rs:262-289`, lexique `credential_keywords` multilingue, ligne entière remplacée) ; (3) candidats d'entropie sur les jetons longs. Remplacement par `[REDACTED:<type>]`. La version de rédaction et le rev de kleos-lib entrent dans `wire_purge_version` | Passes (1) et (3) à écrire, passe (2) réutilisée ; fixtures (lot 1). kleos-lib compile sous Windows MSVC, puisque kleos-ingest, qui en dépend, y a été porté (DÉDUIT, Patch 69) | **Nommée : la rédaction est incomplète par construction.** Un secret sans format connu, sans mot-clé sur sa ligne et sous le seuil d'entropie passe. Exposition bornée par la rétention de 14 jours et par le chiffrement |
| Chiffrement au repos | Base du relais ouverte par `kleos_lib::db::Database::connect_encrypted` avec la feature `sqlcipher` de kleos-lib (DÉDUIT : `kleos-lib/src/db/mod.rs:57-65`, `kleos-lib/Cargo.toml:30`) ; clé de 32 octets dans un `EnvironmentFile` systemd en 0600 root, comme les secrets de kleos-server | Une feature de build et une clé, plus rien à écrire | La clé vit sur la même machine que la base, comme pour Kleos : protège une copie volée du disque, pas un accès root |
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

---

## 2. Protocole démon vers relais

### 2.1 Principe

Le fichier transcript est le tampon. Le démon ne conserve qu'un curseur par fichier ;
tout ce qui n'est pas acquitté est relu depuis le fichier au passage suivant. Le relais
acquitte un segment seulement après l'avoir écrit dans une transaction. L'effet est
« au moins une fois » sur le réseau, « exactement une fois » dans le stockage du relais
grâce à une clé d'idempotence.

### 2.2 Table d'état du démon (SQLite poste, `%LOCALAPPDATA%\kleos-transcript\state.db`)

```sql
CREATE TABLE files (
  path            TEXT PRIMARY KEY,          -- chemin absolu normalisé
  file_key        TEXT NOT NULL UNIQUE,      -- sha256(host_id || path relatif à projects/)
  session_id      TEXT NOT NULL,             -- nom du fichier principal (F5)
  kind            TEXT NOT NULL,             -- 'main' | 'subagent'
  parent_session  TEXT,                      -- pour kind = 'subagent'
  project_dir     TEXT NOT NULL,             -- dossier <projet> sous projects/
  epoch           INTEGER NOT NULL DEFAULT 0,-- incrémenté à chaque réécriture détectée
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
```

Aucune table d'envoi en attente : un segment est reconstruit depuis le fichier entre
`acked_end` et la dernière fin de ligne.

### 2.3 Segment

```json
{
  "file_key": "…", "session_id": "…", "kind": "main", "parent_session": null,
  "project_dir": "C--Users-Olivier-workspace-Kleos", "host_id": "…",
  "epoch": 0, "start": 1048576, "end": 1310720,
  "raw_sha256": "…",            // empreinte des octets BRUTS [start, end)
  "prev_tail_sha256": "…",      // = acked_tail_sha, chaîne le segment au précédent
  "wire_purge_version": 1,      // purge B + rédaction des secrets (1.4, 3.2)
  "records": [ { "offset": 1048576, "uuid": "…", "parent_uuid": "…", "type": "user",
                 "ts": "…", "cwd": "…", "role": "user", "text": "…",
                 "tool": null, "tool_result": null, "is_error": null,
                 "marker": null } ]
}
```

Règles :

1. `end` tombe toujours juste après un octet `\n`. Une ligne en cours d'écriture n'est
   jamais envoyée ni comptée. **DÉDUIT de `kleos-ingest/src/tailer.rs:74-119` :** le
   tailer actuel compte une ligne partielle lue en fin de fichier, échoue à la parser,
   puis avance l'offset au-delà : la fin de cette ligne, écrite plus tard, est relue
   comme un fragment illisible et l'enregistrement est perdu. La fréquence réelle est
   SUPPOSÉE faible (une ligne s'écrit probablement en un seul appel), mais le nouveau
   démon ne doit pas reproduire ce défaut.
2. Taille bornée : au plus 256 Kio purgés ou 2 000 enregistrements par segment
   (paramètres, SUPPOSÉS suffisants).
3. Un seul segment en vol par fichier, au plus 4 fichiers en parallèle.
4. L'empreinte porte sur le brut : changer la purge ou la rédaction ne crée pas de
   conflit.

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
| Disque relais plein | `507` | Backoff, curseur inchangé |

`GET /v1/files/{file_key}/cursor` renvoie `{epoch, acked_end, tail_sha256}` : il
reconstruit l'état du démon si sa base est perdue ou réinstallée.

### 2.5 Réécriture détectée

Avant chaque lecture d'un fichier modifié, le démon compare :

- `size < acked_end` : troncature ;
- sha256 des 4 096 octets avant `acked_end` différent de `acked_tail_sha` : réécriture ;
- sha256 des 4 096 premiers octets différent de `head_sha` : fichier remplacé.

Dans les trois cas : `epoch += 1`, `acked_end = 0`, renvoi complet sous la nouvelle
époque. Le relais marque l'époque précédente `superseded` ; la dédup par uuid (2.6)
empêche de réextraire ce qui a déjà été traité. Coût : une lecture de 4 Kio par fichier
modifié (DÉDUIT du schéma).

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

---

## 3. Règle de purge

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

**Budget de calcul recalculé sur T2 (point 4 du challenge).** Une fenêtre de 36 000
caractères fait 9 084 tokens (T2), soit 3,96 caractères par token ; l'hypothèse de 3,5
de la révision 1 est remplacée. Fenêtre, état de session (au plus 2 000 tokens),
consignes (environ 1 000) et sortie (au plus 800) tiennent dans 16 384 tokens (DÉDUIT).
Durée par fenêtre, raisonnement coupé : **12 s à 22 s** selon la sortie (MESURÉ
rapporté, T2).

| Vue présentée au modèle | Volume par jour | Fenêtres par jour | Temps Ollama par jour, `think: false` (DÉDUIT de T2) | Ce qui est perdu |
|---|---|---|---|---|
| A | environ 6 Mo | environ 170 | 35 à 60 min | Commandes, sorties d'erreur, rapports de sous-agents, envois inter-agents, stores de l'agent |
| **A+ (recommandé)** | au plus 9 Mo (cible) | au plus 250 | **50 à 90 min**, plus environ 8 % de relances (M-N5) et environ 50 appels de réduction (687 sessions sur 14 jours, T3), soit **environ 1 à 2 h** | Contenu des lectures de fichiers et sorties volumineuses, qui restent stockés |
| B | environ 44 Mo | environ 1 220 | 4 à 7,5 h, plus relances et réductions | Presque rien |

**Conséquence sur la décision de la révision 1.** B présenté au modèle n'est plus
« impossible » (10 à 20 h estimées) : il coûte 4 à 8 h de GPU partagé par jour, soit
environ cinq fois A+. A+ reste recommandé pour la vue du modèle, parce que le gain de B
porte sur des contenus que l'assistant reformule déjà (3.1, SUPPOSÉ) ; le stockage en B
garde l'option ouverte.

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

### 3.3 Contenu de la vue A+ (dérivée côté serveur)

Gardé :

- texte user **tapé**, après retrait des injections du harnais (`<system-reminder>`,
  `<channel …>`, blocs `MANDATORY RULES`, contexte de hook, préambule de reprise après
  compaction, `<local-command-*>`, `<command-*>`) ;
- texte assistant ;
- `tool_use` réduit : nom de l'outil et arguments clés (commande Bash, chemin, motif,
  description), tronqués à 300 caractères ;
- `tool_result` réduit : `is_error`, les 200 premiers caractères, et les lignes qui
  contiennent `error`, `failed`, `panic` ou un code de sortie non nul, dans la limite de
  600 caractères ;
- **gardés en entier**, parce qu'ils sont du texte écrit par un agent : l'argument
  `message` de `send_message` (claude-peers), le résultat final d'un outil de sous-agent,
  et le contenu d'un store Kleos de l'agent (`kleos-cli store …`, `memory_store`) ;
- marqueurs : `compact_boundary`, début et fin de sous-agent, `{from_peer, sent_at}`
  pour un message reçu.

Retiré dès le démon (jamais envoyé) : `thinking`, `attachment`, `api-request*`, doublon
`toolUseResult`, images, enveloppe des lignes `message`, ligne `isCompactSummary`
(dérivée des tours déjà transmis ; F5). Retiré par la vue A+ seulement (stocké en B) :
le reste des `tool_result` et des arguments d'outils.

### 3.4 Statut des `<channel>` et des sous-agents

- **`<channel source="claude-peers">` reçu : retiré de la vue A+, remplacé par un
  marqueur** `{from_peer, sent_at}`. Le message est une **copie** : son original est
  l'argument `send_message` de la session émettrice, gardé en entier. Règle générale :
  **extraire un fait là où il est écrit, jamais là où il est recopié.** Cela évite qu'un
  brief de team-lead produise N mémoires dans N sessions réceptrices. Le marqueur
  compte : la moitié des reprises après un long écart sont déclenchées par un message
  injecté (T3), et le tour qui suit doit rester rattaché à sa cause.
- **Sous-agents : non envoyés en v1.** Leur rapport final arrive dans le `tool_result`
  de l'outil d'agent du parent, gardé en entier par A+. **SUPPOSÉ** : que le rapport soit
  bien dans ce `tool_result` ; à vérifier sur 20 sessions en lot 0 (geste opérateur, F9).
  Si la vérification échoue, les fichiers `subagents/` passent par le même protocole avec
  `kind = 'subagent'`, et seul leur dernier message assistant est extrait.

---

## 4. Traitement côté serveur

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
   stores Kleos faits par l'agent dans la fenêtre, puis la fenêtre. Sortie JSON sous
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
de traitement de l'étude nightly 4.2. Les prompts passent par le canal
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
| Fait nouveau | `POST /memory`, `source=kleos-transcript` | F11 |
| Fait révisé, mémoire en attente ou approuvée | `POST /memory` avec `parent_memory_id` = tête connue : nouvelle version, l'ancienne passe à `is_latest = 0`, la dédup est sautée | DÉDUIT (F12, `mod.rs:578`, `:873-890`) |
| Fait révisé, mais la tête a changé (une autre écriture a versionné la mémoire) | Kleos refuse (`Conflict`) ; le relais publie le fait révisé comme mémoire neuve et laisse le juge trancher (`met_a_jour` ou `contredit`, 4.5) | DÉDUIT (`mod.rs:885-889`) |
| Fait annulé, mémoire en attente | `reject_memory` | DÉDUIT (`kleos-lib/src/inbox.rs:92-132`) |
| Fait annulé, mémoire approuvée | **Proposition soumise à l'opérateur (Q2)** : archivage par `POST /memory/{id}/archive`, seulement sur une mémoire que le relais a écrite et que personne n'a versionnée depuis | Route DÉDUITE (`routes/memory/mod.rs:62`) |

Règle d'autorité : **le relais ne révise, n'annule ou n'archive que ses propres
publications**, identifiées par sa correspondance et par `source=kleos-transcript`.
Jamais une mémoire d'agent.

**Idempotence des publications.** Chaque publication est inscrite dans la
correspondance avec l'état `intention` avant l'appel, puis l'id rendu. Les mémoires
portent le tag `fact:<lineage8>-<id local>`. Après une coupure entre l'appel et
l'inscription, le relais cherche ses propres mémoires par ce tag avant de republier
(SUPPOSÉ : un filtre par tag est exposé par `/list` ou `/search` ; porte du lot 5). En
dernier recours, le correctif de dédup de 4.4 laisse la dédup absorber le renvoi
identique d'une source en attente sur une autre ligne en attente.

**Limite nommée, porte active.** Si `kleos-transcript` est en attente et que le relais
révise une mémoire déjà approuvée, la nouvelle version naît `pending` alors que
l'ancienne passe à `is_latest = 0` : le fait peut disparaître des vues qui filtrent sur
`is_latest` jusqu'à l'approbation (SUPPOSÉ : non vérifié pour chaque vue ; les tiers de
`recall` filtrent `status` et `is_superseded`, `mod.rs:1276-1358`). La fenêtre vaut au
plus un passage du juge nightly. Volume borné par T3 : la révision d'un fait approuvé
ne survient que dans les reprises après 8 h.

**Ce qui devient une mémoire :** un fait de l'état final de lignée, au statut `décidé`
ou `mesuré`, ni annulé ni révisé ensuite, et absent des stores de l'agent (liste vue en
fenêtre, puis dédup, puis juge). Un fait `proposé` ou `abandonné` reste dans le relais.
**C'est la réponse à l'inquiétude de l'opérateur :** une décision contredite trois tours
plus tard, ou dans une reprise, ne sort pas de l'état de lignée ; et si elle est déjà
sortie, sa révision suit.

### 4.4 Statut « en attente » : la porte existe, mais sa dédup absorbe (point 1 du challenge)

**DÉDUIT de `kleos-lib/src/memory/mod.rs:646-652` et `:777-835` :** `memory::store`
résout un statut initial `pending` quand `KLEOS_REVIEW_GATE_ENABLED` est actif et que la
source figure dans `KLEOS_REVIEW_GATE_SOURCES` (ou que l'importance dépasse un seuil
optionnel). **DÉDUIT de `:1052-1059`, `:1160`, `:1276-1358`, `:2222`, `:2243` :** les
lectures servies filtrent `status != 'pending'`. **DÉDUIT de `kleos-lib/src/inbox.rs:92-132` :**
`approve_memory` et `reject_memory` existent. La porte est **éteinte en production**
(T1-b).

**Le défaut (T1).** La requête de dédup (`mod.rs:586-590`) cherche les doublons parmi
les lignes `pending`. Quand un agent stocke plus tard un fait déjà publié en attente par
le relais, son store rend `created=false, duplicate_of=<id pending>` : aucune ligne
servie n'est créée, et le chemin doublon ne modifie pas la ligne existante
(`mod.rs:607-616`). L'écriture explicite de l'agent, la plus précieuse, devient
invisible jusqu'à l'approbation de la copie de transcript ; si le juge la laisse en
attente (`contredit`) ou tombe en panne, indéfiniment. Avec la publication à 8 h (4.3),
le cas n'est pas seulement intra-session : n'importe quel agent qui stocke le même fait
le lendemain y tombe.

**Lecture par la discipline de patching du dépôt.** Toutes les lectures servies
excluent `pending` ; la dédup est l'exception. Selon
`.claude/rules/kleos-patching-discipline.md`, l'exception est en général le bug, ce qui
fait du correctif un candidat PR upstream légitime (DÉDUIT des call sites ci-dessus).

**Options de la carte 6ee2297f, et une variante :**

| Option | Effet | Coût de rebase | Verdict |
|---|---|---|---|
| (a) Exclure `pending` de la dédup | Le store de l'agent crée sa ligne servie ; la copie en attente devient un doublon d'une mémoire servie, rejetée par le juge (4.5). Mais un renvoi d'une source en attente n'est plus absorbé non plus, ce qui remplit la boîte de revue de copies pour toute source en attente | Chirurgical, une condition | Correct, trop large |
| **(a') Exclure `pending` seulement quand l'écriture entrante n'est pas en attente** | Résoudre `initial_status` avant la dédup (fonction pure, déjà calculée plus bas, `mod.rs:646-652`) et ajouter `AND (status != 'pending' OR ?3 = 'pending')`. L'agent n'est plus absorbé ; une copie de transcript reste absorbée par une ligne en attente identique (renvoi) et par une mémoire servie (direction voulue, #31639) | Chirurgical, 3 à 5 lignes dans `memory::store`, aucun renommage ; candidat PR | **Recommandée** |
| (b) Promouvoir la ligne en attente en `approved` quand une source non gardée la touche | Une seule ligne, visible tout de suite ; mais elle garde la source `kleos-transcript` et son importance plafonnée à 6, alors que le chemin doublon ne relève rien (`mod.rs:607-616`) : l'attribution et l'importance de l'agent sont perdues | Chirurgical, un peu plus de lignes | Écartée |
| (c) Ne pas activer la porte | Zéro delta ; mais alors aucune attente n'existe dans Kleos, et le relais devient seul juge avant publication, en double du juge nightly | Nul | Repli si (a') est refusé, voir Q2 |

**Parade retenue :** (a'), patch numéroté chirurgical, proposé en PR upstream. **Tant
qu'il n'est pas livré, la porte reste éteinte et le lot 5 ne démarre pas** ; les lots 0
à 4 n'écrivent rien dans Kleos, donc rien n'est exposé entre-temps. **SUPPOSÉ, à
vérifier avant le lot 5 :** que kleos-cache ne réplique pas les lignes `pending`
(contrat `/list`, étude nightly 4.9 et Patch 67).

**Ce que la dédup ne couvre pas.** Elle ne voit que les quasi-identiques (distance de
Hamming inférieure à 3) ; une reformulation passe et va au juge.

### 4.5 Typage des mémoires issues des transcripts

Réponse à la question de l'opérateur : **oui, un typage distinct par la source, mais
la même taxonomie de catégories que les agents.**

| Champ | Valeur | Raison |
|---|---|---|
| `source` | `kleos-transcript` (nouvelle) | Distincte de `claude-session` (agents) et de `kleos-ingest` (ancien démon) ; clé de la porte d'attente (4.4), de l'autorité de révision (4.3), du budget et des mesures par source (M-N2) |
| `category` | Ensemble des agents : `decision`, `discovery`, `state`, `issue`, `task`, `reference`, `general` | Le juge compare des mémoires de même vocabulaire. Les catégories lexicales `infrastructure` et `session-summary` disparaissent (DÉDUIT de `kleos-ingest/src/writer.rs:198`, `extractor.rs:84`) |
| `importance` | Proposée par le modèle, **plafonnée à 6** | Un fait de transcript ne doit pas l'emporter sur le store explicite d'un agent ; le relèvement d'importance de Patch 60 n'a pas lieu dans le chemin doublon de `memory::store` (`mod.rs:607-616`), s'il existe c'est ailleurs, non relu |
| `created_at` | Timestamp du tour source | Champ accepté par l'API (F11) ; l'ordre temporel des voisinages reste juste malgré un traitement différé de plusieurs heures |
| `session_id`, `tags` | Session source ; `transcript`, projet, `turn:<uuid>`, `fact:<lineage8>-<id local>` | Provenance pour la revue, pour l'annotation du juge et pour l'idempotence (4.3) |
| `space` | Résolu depuis `cwd` des enregistrements, table de correspondance du relais | D1 : jamais d'espace croisé ; la dédup est aussi bornée à l'espace (`mod.rs:571-572`). L'ancien écrivain n'envoie aucun espace (DÉDUIT de `kleos-ingest/src/writer.rs:100-107`) |
| `status` | `pending` à l'écriture, une fois (a') livré | Section 4.4 |

**Résumés de session : pas de mémoire en v1.** Ils restent dans le relais, comme
matériau de travail. F7 montre ce que vaut un résumé servi comme mémoire ; un résumé de
session est épisodique et concurrence les faits au classement. Question Q3.

### 4.6 Branchement sur le juge, la supersession et la contradiction

Trois barrières contre le doublon agent/transcript, de la moins chère à la plus chère :

1. **Dans le prompt** : les stores de l'agent vus dans la fenêtre (gardés en entier par
   A+) sont fournis au modèle comme « déjà stocké » ;
2. **À l'écriture** : la dédup de 4.4, une fois (a') livré ;
3. **Au juge nightly** : la mémoire de transcript en attente est une racine du
   voisinage de l'étude nightly 4.6, avec les mêmes candidats (`similarity`, M1,
   déduplication), le même propriétaire et le même espace.

Proposition de règle de sortie d'attente, **qui crée une autorité nouvelle et demande
l'accord de l'opérateur (Q2)** :

| Verdict du juge pour la racine de transcript | Effet |
|---|---|
| `doublon` d'une mémoire servie | `reject_memory` sur la copie de transcript. Ce n'est pas un retrait au sens du principe 4 de l'étude nightly : la copie n'a jamais été servie |
| `distinct`, `prolonge` | `approve_memory` |
| `met_a_jour`, `partiel`, `remplace` face à une mémoire plus ancienne | `approve_memory` + annotation de relation ; le retrait de l'ancienne exige toujours le signal explicite de l'étude nightly 4.1 et 4.6, sauf si l'ancienne est une publication du même relais dans la même lignée (chaîne de versions, 4.3) |
| `contredit` | Reste en attente, conflit ouvert dans les annotations, pour l'opérateur ou l'agent. Grâce à (a'), cela ne masque plus aucun store d'agent |
| Juge en panne ou budget épuisé | Reste en attente : échec fermé, les mémoires des agents ne sont pas touchées |

**Coût, SUPPOSÉ :** à 30 mémoires de transcript par jour ouvré, environ 0,45 USD par jour
au coût M-N4 de 0,0151 USD par groupe, à ajouter au plafond quotidien de D5. Le volume
réel sort de la porte du lot 4.

---

## 5. Rétention, rattrapage, purge des copies

| Objet | Rétention proposée | Raison |
|---|---|---|
| Enregistrements B rédigés (relais) | 14 jours après la dernière stabilisation qui les couvre (X = 14, paramètre) | Retraiter après un changement de vue, de prompt ou de modèle ; couvrir une reprise tardive ; borner l'exposition des secrets résiduels (1.4) |
| Instantanés d'état de lignée | Comme les enregistrements, et au moins tant que la lignée peut être reprise dans la fenêtre de 14 jours | Héritage à la reprise (4.3) |
| `uuid_index`, `lineage` | 400 jours (paramètre) | **La dédup des reprises dépend de l'index, pas du contenu** : un `claude --resume` après la suppression du contenu ne doit pas réextraire l'historique recopié |
| Résumés et faits candidats | Comme les enregistrements | Matériau de travail |
| Correspondance fait vers id et version Kleos | Sans limite | Idempotence des publications, révision (4.3), provenance |
| Mémoires publiées | Cycle de vie Kleos ordinaire | Hors relais |
| Côté poste | Aucune copie ; le démon ne supprime jamais un transcript Claude Code | Le fichier est la source |

**Reprise après expiration du contenu.** Une reprise d'une lignée dont les
enregistrements ont expiré ne retrouve pas d'instantané : ses nouveaux tours partent
d'un état vide, mais la correspondance reste, donc une révision ne peut viser un fait
publié que par la dédup et le juge. **Limite nommée**, rare : T3-a mesure des copies à
p50 16 min de la source.

**Rattrapage des 7 Go : borné.** Recommandation : les 14 derniers jours (environ 4 Go
bruts au flux moyen, environ 600 Mo en B et environ 100 Mo en vue A+, DÉDUIT de F1 et
F3), d'abord comme **corpus de calibration** du lot 4, puis publication si la porte
passe : environ 2 800 fenêtres, soit 9 à 17 h d'Ollama étalées sur plusieurs nuits
(DÉDUIT de T2). Le reste n'est pas traité par défaut : l'historique ancien concurrencerait
des mémoires d'agent plus récentes au juge pour un gain non mesuré, et coûterait environ
20 à 35 h d'Ollama en vue A+ (DÉDUIT de T2 et de la cible de 3 %). Question Q4.

---

## 6. Modes de défaillance

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
| Store d'agent identique à une copie en attente | Avant (a') : absorbé (T1), d'où la porte éteinte ; après (a') : ligne servie créée, copie rejetée par le juge | Aucune une fois (a') livré |
| Secret non reconnu par la rédaction | Stocké chiffré au relais, au plus 14 jours ; présenté au modèle seulement s'il tombe dans la vue A+ | **Exposition résiduelle, limite nommée (1.4)** |
| Fichier réécrit ou tronqué | Détection 2.5, nouvelle époque, renvoi complet ; uuid connus ignorés | Aucune |
| `/compact` | Même fichier, ajout simple ; résumé de compaction retiré, marqueur gardé | Aucune |
| Ligne en cours d'écriture | Jamais envoyée avant son `\n` (2.3) | Aucune |
| Disque plein, relais | `507`, aucune écriture partielle ; alerte | Aucune ; retard |
| Disque plein, poste | Échec d'écriture de l'état, donc pas d'avance de curseur ; renvoi ensuite | Aucune |
| Deux postes, même projet | `file_key` inclut `host_id` ; l'index uuid est par propriétaire, pas par poste | Aucune |
| Horloge ou timestamps non monotones | Ordre par offset ; `T_settle` mesuré à l'horloge du relais (F4) | Aucune |
| Bump de kleos-lib qui casse la compilation ou le test jetable | Le relais reste sur l'ancien rev, qui continue de parler à Kleos par l'API HTTP ; le bump attend une adaptation | Aucune ; retard du bump. Répété à chaque pull, ce cas rouvre le verdict 1.3 |
| Changement de vue, de prompt ou de modèle | `model_view_version` ou `contract_version` rouvre les lignées encore en rétention ; les écarts avec la correspondance passent par 4.3 | Au-delà de 14 jours : pas de retraitement |

---

## 7. Sort de kleos-ingest et des cartes

**kleos-ingest : gardé dans l'arbre, non déployé, non étendu.** C'est un crate upstream
(Patch 58, 69 et 70 y sont de petits deltas, candidats PR). Le supprimer ou le réécrire
sur place créerait le plus gros site de conflit possible (niveau « réécriture »).

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
- **6ee2297f : trancher en (a')** (4.4) ; prérequis du lot 5.
- **3f45e0f8** et **19671bc8** : livrés, sans suite dans la cible.

---

## 8. Lots ordonnés et portes mesurables

| Lot | Contenu | Porte mesurable |
|---|---|---|
| **F. Gel avant tout code** (verdict 1.3) | (1) contrat HTTP démon vers relais, sections 2.3 et 2.4, versionné dans le dépôt du relais ; (2) correctif (a') de la dédup (4.4), patch numéroté et PR upstream ; (3) rev git épinglé de kleos-lib sur le fork VOCSAP et règle de bump écrite (à chaque merge upstream du fork, après un build vert) ; (4) liste des champs de `StoreRequest` et des quatre fonctions de kleos-lib utilisés, avec un test d'intégration jetable | (1) schéma JSON du segment et table des réponses figés, relus par le lead ; (2) test d'intégration : un store `claude-session` identique à une ligne en attente rend `created=true`, et un renvoi en attente sur une ligne en attente reste absorbé ; (3) le relais compile contre le rev épinglé, hors des `members` du `Cargo.toml` racine (`git diff Cargo.toml` vide) ; (4) le test jetable passe sur le rev épinglé |
| **0. Mesures opérateur** (F9 : geste opérateur) | (a) second passage ajout seul : sha256 du préfixe `[0, taille_T0)` à T0 puis T0 + 24 h sur tous les fichiers ; (b) taille A+ et taille B sur les 7 Go ; (c) part des enregistrements gardés sans uuid, par type ; (d) présence du rapport de sous-agent dans le `tool_result` parent, 20 sessions ; (e) lignes partielles observées en fin de fichier ; (f) rappel de la rédaction : nombre de secrets connus de l'opérateur (inventaire `cred list`, noms seulement) retrouvés en clair après rédaction, sur 14 jours ; (g) les deux points non vérifiés par le consultant : `GET /api/ps` sur l'Ollama 192.168.10.50, et chiffrement de `kleos.db` en production | (a) 0 préfixe modifié hors `/compact` documenté, sinon la réécriture (2.5) devient le chemin nominal et passe en tête du lot 2 ; (b) A+ au plus 3 % du brut ; (c) chiffre consigné ; (d) au moins 18 sur 20, sinon option `subagent` de 3.4 ; (e) chiffre consigné ; (f) 0 valeur du coffre retrouvée en clair ; (g) `/api/ps` rend la liste des modèles chargés, sinon la garde de 4.1 passe à un repli (fenêtre horaire fixe) ; état de chiffrement de `kleos.db` consigné, pour fixer E7 |
| **1. Contrat de purge et rédaction** | Spécification de la purge B, de la rédaction et de `wire_purge_version` 1 côté démon ; spécification de la vue A+ et de `model_view_version` 1 côté serveur ; fixtures synthétiques (injections du harnais, `<channel>`, `send_message`, stores Kleos, compaction, jetons de chaque format listé en 1.4) | Tests de fixtures : 100 % des motifs d'injection listés retirés de la vue A+, 100 % du texte user tapé conservé, 100 % des jetons de fixture rédigés, sortie identique au fichier de référence |
| **2. Réception seule** | Endpoints 2.4, jeton par poste, base ouverte par `connect_encrypted` (feature `sqlcipher` de kleos-lib), `uuid_index`, `lineage`, curseur ; aucun traitement | Banc : copie de 50 transcripts, démon tué 20 fois au hasard puis relancé, réception redémarrée 5 fois ; contenu identique octet pour octet à un passage unique ; 0 enregistrement dupliqué ; 1 réécriture et 1 fork injectés traités selon 2.5 et 2.6 ; une requête sans jeton reçoit `401` ; la base est illisible sans la clé |
| **3. Démon poste** | Scan par stat, `notify`, rédaction, tâche planifiée à l'ouverture de session, sans console, jeton par `cred exec` | Scan stat de 4 751 fichiers en moins de 2 s ; régime permanent sous 1 % CPU sur 1 h ; le banc du lot 2 passe avec le vrai démon |
| **4. Traitement en ombre** | Fenêtrage, map, état de lignée et instantanés, stabilisation, `think: false` vérifié ; écriture dans une table du relais seulement, aucune écriture Kleos | Sur le corpus de calibration (section 5), étiquetage opérateur d'un tirage de 40 faits publiables : au moins 60 % utiles (contre environ 10 % aujourd'hui, F7) ; JSON valide après relance au moins 99 % ; 0 réponse acceptée avec `done_reason = length` ; une journée de flux traitée en moins de 2 h d'Ollama, mesurée en conditions réelles de partage ; au moins 95 % des faits annulés ou révisés dans la lignée absents de l'état final ; sur les sessions du corpus reprises après plus de 45 min, au moins 95 % des révisions faites dans la reprise appliquées à l'état hérité |
| **5. Publication en attente et juge** | Prérequis : correctif (a') livré et déployé (lot F, 4.4). `source=kleos-transcript`, porte d'attente, règle 4.6, révision par chaîne de versions (4.3). Dépend de l'étape 2 de l'étude nightly 4.10 | Second passage : 0 nouvelle mémoire ; 0 mémoire de transcript renvoyée par `/recall` ou `/list` avant approbation ; un store `claude-session` identique à une ligne en attente rend `created=true` (inverse de T1) ; sur une fixture où l'agent a stocké X et le transcript contient X, au moins 90 % des copies rejetées ou absorbées ; une lignée rouverte après publication produit une version 2 et aucune seconde tête ; une coupure simulée entre l'appel et l'inscription ne produit aucun doublon |
| **6. Rétention et rattrapage borné** | Purge à 14 jours, index et correspondance conservés, rattrapage de 14 jours | Après purge, `claude --resume` d'une session purgée : 0 réextraction ; volume disque du relais sous 1,5 Go |

---

## 9. Questions ouvertes pour l'opérateur

**Q1. Niveaux de purge.** Stockage : (a) **B rédigé, recommandé** (idée du challenger,
environ 620 Mo sur 14 jours, surface de secrets traitée en 1.4) ; (b) A+ seulement, plus
petit mais figé au démon. Vue du modèle : **A+, recommandé**, environ 1 à 2 h d'Ollama
par jour ; B coûterait 4 à 8 h (3.2). Raison : le calcul serveur décide la vue, la
réversibilité décide le stockage.

**Q2. Sortie d'attente et autorité du relais.** (a) Revue manuelle dans la boîte de
revue : sûr, mais quelques dizaines par jour ne seront pas relues ; (b) **le juge
nightly approuve ou rejette selon 4.6, et le relais archive ses propres publications
annulées dans la même lignée (4.3), recommandé** : deux autorités nouvelles, bornées à
ce que le relais a écrit ; (c) pas de porte (option (c) de 4.4) : servables tout de
suite, le relais seul vérifie le voisinage par l'API avant publication, en double du
juge.

**Q3. Résumés de session.** (a) **Restent dans le relais, recommandé** ; (b) une mémoire
`state` par session, au risque de concurrencer les faits au classement (F7) ; (c) un
handoff Kleos par session, ce qui recouvre les handoffs écrits par les agents.

**Q4. Rattrapage de l'historique.** (a) Aucun ; (b) **14 jours, d'abord en calibration,
recommandé** (9 à 17 h d'Ollama étalées) ; (c) les 7 Go : environ 20 à 35 h de plus et
un afflux d'anciens faits au juge, pour un gain non mesuré.

**Q5. Emplacement du traitement serveur : tranché par le consultant (section 1.3).**
R : relais séparé dans un dépôt séparé, dépendant de kleos-lib par un rev git épinglé
sur le fork VOCSAP, hors des `members` du `Cargo.toml` racine. **Validé par l'opérateur
le 2026-10-21**, ainsi que la règle de bump : à chaque merge upstream du fork, après un
build vert, avec le test d'intégration jetable de 1.3. Correctif de dédup retenu par
l'opérateur : variante (a') de 4.4 (carte 6ee2297f). S et la réécriture sur place de
kleos-ingest sont écartés.

---

## 10. Références

- Étude nightly : `docs/studies/2026-10-20-kleos-nightly-pipeline.md`, sections 3, 4.1 à
  4.10, 6, 8 et 10.
- Kleos : #31653, #31639, #31658, #31675, #31676, #31682, #31699, #31706, #31707,
  #31716, #30938.
- Code lu : `kleos-ingest/src/{tailer,writer,ledger}.rs` ;
  `kleos-lib/src/conversations.rs` ; `kleos-server/src/routes/conversations/mod.rs` ;
  `kleos-server/src/routes/memory/mod.rs:60-147` ;
  `kleos-lib/src/memory/mod.rs:560-660`, `:775-836`, `:850-895` ;
  `kleos-lib/src/memory/types.rs:300-342` ; `kleos-lib/src/inbox.rs` ;
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
