# Toolbox -- catalogue d'outils indexés dans Kleos

La toolbox répond à une seule question, depuis n'importe quel CLI d'IA :
**« a-t-on déjà un outil pour X, et où est-il ? »**

Un poste indexe un outil (dépôt git, skill, plugin, CLI, serveur MCP, page de
doc) : l'agent lit la source, rédige une fiche (résumé + corps + mots-clés) et
l'envoie à Kleos. Plus tard, n'importe quelle session d'agent authentifiée avec
la même identité pose une requête en langage naturel et récupère les outils
pertinents **avec leurs emplacements** (URL, chemin local, hôte).

## Modèle de données (ce qu'il faut savoir avant d'écrire un prompt)

| Table | Portée | Contenu |
|---|---|---|
| `toolbox_tools` | **partagée**, agnostique de l'utilisateur | la fiche (`name`, `summary`, `body`, `keywords`) et son embedding, une ligne par clé canonique |
| `toolbox_locations` | **scopée `user_id`** | où cet utilisateur possède cet outil : `host`, `local_path`, `tags`, `notes` |

Conséquences directes :

- La recherche ne renvoie **jamais** un outil dont l'utilisateur n'a aucun
  emplacement. La fiche partagée n'est lue qu'à travers le filtre des clés
  possédées (anti-fuite entre utilisateurs).
- Oublier un outil (`DELETE /toolbox/entries/{id}`) supprime **vos
  emplacements**, jamais la fiche partagée : les autres shards ne sont pas
  visibles depuis un seul poste.
- La clé canonique (`tool_key`) est **toujours recalculée par le serveur** à
  partir des champs bruts. Un client ne l'envoie jamais.
- Aucune table ne porte `space_id`. Le bridge MCP injecte `space` / `space_id`
  dans tous les appels ; les handlers les ignorent.

### Clé canonique

Priorité `git_remote` > `url` > `local_path` :

| Entrée | Clé produite |
|---|---|
| `git@github.com:VOCSAP/Kleos.git` | `github.com/vocsap/kleos` |
| `https://user:token@github.com/VOCSAP/Kleos.git/` | `github.com/vocsap/kleos` |
| `https://github.com/a/b/tree/main/x` (champ `url`) | `github.com/a/b/tree/main/x` |
| `local_path=/opt/tools/foo`, `host=nuc-01` | `local:nuc-01:/opt/tools/foo` |

Les clés git et url sont minuscules, sans schéma, sans identifiants, sans port,
sans `.git`, sans `/` final. Le chemin d'une clé `local:` garde sa casse.

### Politique d'écrasement de la fiche partagée

Dernière indexation gagnante **si son commit est plus récent** :

| Fiche stockée | Fiche entrante | Résultat |
|---|---|---|
| commit daté `s` | commit daté `i > s` | `updated` |
| commit daté `s` | commit daté `i == s` | `kept`, sauf `force: true` |
| commit daté `s` | commit daté `i < s` | `kept` |
| commit daté `s` | pas de commit | `kept`, sauf `force: true` |
| pas de commit | n'importe quoi | `updated` |

Si le `content_hash` entrant est identique au stocké, l'issue est `unchanged`.
Dans tous les cas, **l'emplacement de l'appelant est upserté** et son
`last_seen_at` rafraîchi. Un embedding manquant est rempli même sur `kept`.

## Surface

| Route HTTP | Tool MCP (nom publié) | Rôle |
|---|---|---|
| `POST /toolbox/entries` | `toolbox_index` | indexe ou met à jour une fiche + votre emplacement |
| `POST /toolbox/find` | `toolbox_find` | recherche hybride (FTS5 + cosinus, fusion RRF) |
| `GET /toolbox/entries/{id}` | `toolbox_get` | fiche + vos emplacements (404 si vous n'en avez aucun) |
| `GET /toolbox/entries` | `toolbox_list` | vos outils (`?kind=&limit=&offset=`) |
| `DELETE /toolbox/entries/{id}` | `toolbox_forget` | supprime **vos** emplacements |
| `POST /toolbox/reindex` | `toolbox_reindex` | recalcule les embeddings manquants de vos outils |

Les noms canoniques côté `kleos-client` sont pointés (`toolbox.index`) ; le
bridge MCP les publie avec un underscore (`toolbox_index`). Dans Claude Code le
nom complet du tool est donc `mcp__kleos__toolbox_index`, en supposant que le
serveur MCP Kleos est déclaré sous la clé `kleos` (config recommandée dans
[`../MCP_CLIENT_SETUP.md`](../MCP_CLIENT_SETUP.md)). Si votre `.mcp.json` nomme
le serveur autrement, adaptez le préfixe dans
[`agents/toolbox.md`](agents/toolbox.md).

Seuls `toolbox.index`, `toolbox.find`, `toolbox.get` et `toolbox.list` sont dans
la liste des daily tools : `toolbox.forget` et `toolbox.reindex` restent
joignables en HTTP direct ou via `kleos-cli`.

## Installation dans Claude Code

Les skills et l'agent vivent ici parce que `.claude/` est gitignoré dans ce
dépôt. On les installe par copie ou par lien symbolique.

```bash
# Lien symbolique (recommandé : un git pull met les prompts à jour)
ln -s "$PWD/docs/toolbox/skills/tool-index" ~/.claude/skills/tool-index
ln -s "$PWD/docs/toolbox/skills/tool-find"  ~/.claude/skills/tool-find
ln -s "$PWD/docs/toolbox/agents/toolbox.md" ~/.claude/agents/toolbox.md

# Ou copie (postes sans droit de lien symbolique, Windows sans mode dev)
mkdir -p ~/.claude/skills ~/.claude/agents
cp -r docs/toolbox/skills/tool-index docs/toolbox/skills/tool-find ~/.claude/skills/
cp    docs/toolbox/agents/toolbox.md ~/.claude/agents/
```

Prérequis : le serveur MCP Kleos doit être déclaré (voir
[`../MCP_CLIENT_SETUP.md`](../MCP_CLIENT_SETUP.md)) et `kleos-mcp` doit démarrer
avec une auth valide. Vérification rapide :

```bash
kleos-cli health
```

Usage :

- « indexe cet outil » / « indexe le dépôt <url> » déclenche `tool-index` ;
- « a-t-on un outil pour ... ? » déclenche `tool-find` ;
- `@toolbox` délègue au sous-agent, qui exécute le skill et ne rend que le
  compte rendu final (utile pour ne pas polluer le contexte principal avec la
  lecture du dépôt).

## Équivalent Codex (ou tout autre CLI MCP)

Rien dans la toolbox ne dépend de Claude Code : la découverte passe par un
endpoint HTTP et un tool MCP.

1. Déclarer le serveur MCP Kleos dans la config du client (stdio, commande
   `kleos-mcp`, `KLEOS_URL` pointant sur le serveur ; voir
   [`../MCP_CLIENT_SETUP.md`](../MCP_CLIENT_SETUP.md) pour les formes exactes de
   `.mcp.json`, `~/.cursor/mcp.json`, `opencode.json`).
2. Concaténer le corps des deux `SKILL.md` (sans leur frontmatter YAML) dans le
   fichier d'instructions permanent du client -- `AGENTS.md` à la racine du
   projet pour Codex, `~/.codex/AGENTS.md` pour une installation globale. Les
   prompts sont écrits pour être lus tels quels : ils ne référencent aucun
   mécanisme propre à Claude Code, seulement les noms de tools MCP.
3. Remplacer `mcp__kleos__toolbox_index` / `mcp__kleos__toolbox_find` par la
   convention de nommage du client (Codex expose en général `toolbox_index`
   tout court, préfixé par le nom du serveur selon la version).

Le repli universel, si le client ne sait pas appeler un tool MCP, reste `curl`
(voir plus bas) ou `kleos-cli`.

## Variables d'environnement (côté serveur)

| Variable | Défaut | Effet |
|---|---|---|
| `KLEOS_TOOLBOX_MAX_BODY_BYTES` | `262144` | taille maximale acceptée pour `body` ; au-delà, `400`. `summary` est plafonné à 4 KiB en dur, `keywords` à 64 entrées |
| `KLEOS_TOOLBOX_RERANK` | non posée (off) | valeur par défaut du champ `rerank` de `POST /toolbox/find` (`1` / `true` -> on). N'a d'effet que si un reranker est configuré sur le serveur |
| `KLEOS_TOOLBOX_EMBEDDING_MODEL` | `unknown` | nom de modèle enregistré avec l'embedding quand le provider n'expose pas le sien. Sert au `reindex` (ré-embedder ce qui vient d'un autre modèle) |

Si aucun embedder n'est configuré, l'indexation réussit quand même : la fiche
est stockée sans vecteur (`"embedded": false`) et la recherche retombe sur la
FTS5 seule. `POST /toolbox/reindex` comble les vecteurs manquants une fois
l'embedder disponible.

## Exemples `curl`

Indexer (le serveur calcule `tool_key` ; on n'envoie jamais ce champ) :

```bash
curl -sS -X POST "${KLEOS_URL:-http://127.0.0.1:4200}/toolbox/entries" \
  -H "Authorization: Bearer ${KLEOS_API_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{
    "key": {
      "git_remote": "git@github.com:VOCSAP/Kleos.git",
      "local_path": "/home/user/Kleos",
      "host": "nuc-01"
    },
    "kind": "repo",
    "name": "Kleos",
    "summary": "Serveur de memoire augmentee pour agents IA. Stockage, recherche hybride et rappel de memoires, skills, taches et handoffs via HTTP et MCP. Memory server for AI agents: hybrid search, skills, tasks, handoffs. A utiliser quand un agent doit persister ou retrouver du contexte entre sessions.",
    "body": "## Objet\n...\n\n## Cas d'usage\n...\n\n## Prerequis\n...\n\n## Commandes cles\n...\n\n## Limites\n...",
    "keywords": ["memoire", "memory", "agent", "mcp", "recherche hybride", "hybrid search", "rust", "sqlite", "embeddings", "handoff"],
    "canonical_url": "https://github.com/VOCSAP/Kleos",
    "commit": { "sha": "8e79a9f0c1d2e3f4a5b6c7d8e9f0a1b2c3d4e5f6", "time": 1757635200 },
    "tags": ["kleos", "infra"],
    "notes": "clone de travail VOCSAP",
    "force": false
  }'
```

Réponse :

```json
{
  "id": 42,
  "tool_key": "github.com/vocsap/kleos",
  "key_kind": "git",
  "outcome": "inserted",
  "embedded": true,
  "location_id": 17
}
```

`outcome` vaut `inserted`, `updated`, `kept` ou `unchanged` (voir la table
d'écrasement plus haut).

Chercher :

```bash
curl -sS -X POST "${KLEOS_URL:-http://127.0.0.1:4200}/toolbox/find" \
  -H "Authorization: Bearer ${KLEOS_API_KEY}" \
  -H 'Content-Type: application/json' \
  -d '{
    "query": "convertir un PDF scanne en texte cherchable",
    "kind": "cli",
    "tags": ["ocr"],
    "limit": 8,
    "rerank": false
  }'
```

Réponse :

```json
{
  "results": [
    {
      "tool": {
        "id": 7,
        "tool_key": "github.com/ocrmypdf/ocrmypdf",
        "key_kind": "git",
        "kind": "cli",
        "name": "OCRmyPDF",
        "summary": "...",
        "body": "...",
        "keywords": ["ocr", "pdf", "tesseract"],
        "canonical_url": "https://github.com/ocrmypdf/OCRmyPDF",
        "indexed_commit": "...",
        "indexed_commit_time": 1756000000,
        "content_hash": "...",
        "has_embedding": true,
        "embedding_model": "nomic-embed-text",
        "indexed_by_user_id": 1,
        "created_at": "2026-09-12T08:00:00Z",
        "updated_at": "2026-09-12T08:00:00Z"
      },
      "locations": [
        {
          "id": 9,
          "user_id": 1,
          "tool_key": "github.com/ocrmypdf/ocrmypdf",
          "host": "nuc-01",
          "local_path": "/opt/tools/OCRmyPDF",
          "tags": ["ocr"],
          "notes": "",
          "last_seen_at": "2026-09-12T08:00:00Z",
          "created_at": "2026-09-01T10:00:00Z"
        }
      ],
      "score": 0.0405,
      "fts_score": -3.21,
      "vector_score": 0.78
    }
  ],
  "count": 1,
  "embedded_query": true,
  "reranked": false
}
```

`fts_score` est le bm25 brut (négatif, plus bas = meilleur), `vector_score` le
cosinus brut, `score` la fusion RRF. `rerank_score` n'apparaît que si le
reranking a tourné. Le vecteur brut n'est jamais sérialisé.

## Fichiers

| Fichier | Rôle |
|---|---|
| `skills/tool-index/SKILL.md` | prompt d'indexation (lecture de la source, rédaction de la fiche, appel de `toolbox_index`) |
| `skills/tool-find/SKILL.md` | prompt de recherche (double reformulation, fusion, présentation) |
| `agents/toolbox.md` | sous-agent Claude Code, outils restreints, ne rend que le compte rendu |

Le code est décrit dans `docs/dev-notes/local-patches.md`, section **Patch 53**.
