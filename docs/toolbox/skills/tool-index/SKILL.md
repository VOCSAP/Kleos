---
name: tool-index
description: Index a tool (git repo, skill, plugin, CLI, MCP server, doc page) into the Kleos toolbox catalog so any AI CLI can find it later. Use when asked to index, catalog, register or remember a tool, a repository, a local directory or a URL.
---

# Indexer un outil dans la toolbox Kleos

Tu rédiges une **fiche** décrivant un outil et tu l'envoies à Kleos. La fiche
est **partagée entre tous les utilisateurs** du serveur ; seul ton emplacement
(hôte, chemin local, tags, notes) t'est propre. Écris donc la fiche pour un
lecteur qui ne connaît ni ce poste ni ce projet.

**Entrée** : une URL, un chemin de dossier, ou rien -- dans ce dernier cas,
l'outil à indexer est le répertoire de travail courant.

## Règles non négociables

1. **N'envoie jamais de champ `tool_key`.** Le serveur calcule la clé canonique
   à partir de `key`. Un `tool_key` envoyé à la main est au mieux ignoré, au
   pire faux.
2. **Ne clone jamais un dépôt sans autorisation explicite de l'utilisateur.**
   Pas de `git clone`, pas de `pip install`, pas de `npm install`, aucune
   exécution du code de l'outil. Si tu penses qu'un clone est nécessaire,
   demande, puis attends la réponse.
3. **Ne fabrique rien.** Tout ce qui entre dans la fiche vient de ce que tu as
   lu. Ce que la source ne dit pas n'est pas écrit. Si tu n'as pas pu lire
   grand-chose, dis-le dans le compte rendu plutôt que de combler les trous.
4. **Une seule fiche par appel.** Si on te demande d'indexer plusieurs outils,
   traite-les l'un après l'autre.

## Étape 1 -- établir l'identité

### Cas A : l'entrée est un chemin local (ou le cwd)

Exécute ces quatre commandes, dans cet ordre. Les trois premières échouent
silencieusement si le dossier n'est pas un dépôt git : c'est un cas normal, tu
continues sans elles.

```bash
git -C "<chemin>" remote get-url origin
git -C "<chemin>" rev-parse HEAD
git -C "<chemin>" log -1 --format=%ct
hostname
```

- `remote get-url origin` -> `key.git_remote` (tel quel, sans le réécrire : le
  serveur normalise `https://`, `git@host:owner/repo.git`, `ssh://`, les
  identifiants intégrés, le `.git` final).
- `rev-parse HEAD` -> `commit.sha`.
- `log -1 --format=%ct` -> `commit.time` (**entier**, secondes unix, pas une
  chaîne de date).
- `hostname` -> `key.host`.
- Le chemin absolu du dossier -> `key.local_path`.

Si `remote get-url origin` ne rend rien (pas de remote, pas de dépôt git),
n'envoie simplement pas `git_remote`.

### Cas B : l'entrée est une URL

Pas de commande git, pas de clone. `key.url` = l'URL donnée, `key.host` =
`hostname` de ce poste **seulement si** l'outil est aussi présent en local
(sinon laisse `host` et `local_path` absents). Pas de `commit`.

### Composition de `key` -- priorité stricte

| Situation | Champs à envoyer |
|---|---|
| un remote git existe | `key.git_remote` (+ `key.local_path` et `key.host` si tu es sur un clone local) |
| pas de remote, l'entrée est une URL | `key.url` (+ `key.host` / `key.local_path` seulement si l'outil existe aussi sur ce poste) |
| pas de remote, pas d'URL | `key.local_path` **et** `key.host` |

`git_remote` l'emporte sur `url`, qui l'emporte sur `local_path` pour le calcul
de la clé. `local_path` et `host` restent utiles même avec un `git_remote` :
c'est ce qui remplit **ta** ligne d'emplacement, donc ce qui te sera rendu plus
tard par une recherche.

## Étape 2 -- lire la source

### Sur un chemin local

Dans l'ordre, en t'arrêtant dès que tu en sais assez :

1. `README.md` (ou `README`, `README.rst`, `docs/README.md`).
2. Les manifestes présents : `package.json`, `Cargo.toml`, `pyproject.toml`,
   `go.mod`, `.claude-plugin/plugin.json`, `SKILL.md`, `.mcp.json`,
   `docker-compose.yml`.
3. L'arborescence sur **deux niveaux de profondeur** (`ls`, `Glob`), pour voir
   les points d'entrée, les binaires, les sous-commandes.
4. Si le README est pauvre : `--help` du binaire **uniquement s'il est déjà
   installé** et que la commande est manifestement inoffensive, sinon les
   fichiers de doc (`docs/`, `CONTRIBUTING.md`).

### Sur une URL sans clone

Récupère la page avec l'outil de fetch web dont tu disposes (`WebFetch` sous
Claude Code). Pour un dépôt GitHub, vise le README rendu : la page du dépôt
suffit. Rédige la fiche **à partir de ce seul contenu** et signale dans les
`notes` que l'indexation s'est faite sans clone.

Si le fetch échoue (page inaccessible, robots, réseau), **n'invente pas de
fiche** : dis à l'utilisateur que tu n'as pas pu lire la source et propose soit
un clone autorisé, soit un chemin local.

## Étape 3 -- rédiger la fiche

Gabarit exact. Chaque champ compte : `summary` et `keywords` sont ce qui est
embedded et indexé en plein texte, donc ce qui décide si l'outil ressort d'une
recherche six mois plus tard.

| Champ | Type | Contrainte |
|---|---|---|
| `name` | chaîne | le nom propre de l'outil, 80 caractères max, pas de chemin, pas de slogan |
| `kind` | chaîne | **exactement un** de : `repo`, `skill`, `plugin`, `cli`, `mcp`, `doc`, `other` |
| `summary` | chaîne | **3 à 5 phrases**, 4 KiB max. Dense en mots-clés, **français et anglais mêlés**. Doit répondre à « à quoi ça sert » **et** « quand l'utiliser » |
| `body` | markdown | 5 sections imposées, voir plus bas. Vise 1 000 à 6 000 caractères |
| `keywords` | tableau de chaînes | **10 à 25 entrées**, minuscules, **FR + EN**, pas de doublon, pas de mot vide |
| `canonical_url` | chaîne ou absent | l'URL publique de l'outil (page GitHub, doc officielle) |
| `commit` | objet ou absent | `{ "sha": "...", "time": 1757635200 }`, `time` en secondes unix |
| `tags` | tableau de chaînes | 0 à 8, pour **ton** emplacement : `perso`, `client-x`, `infra`, `a-evaluer` |
| `notes` | chaîne | une ligne sur **ton** usage local (« clone de travail », « installé via pipx », « lu sans clone ») |
| `force` | booléen | `false` par défaut. Voir « écrasement » |

### `summary` -- ce qui marche

Trois à cinq phrases qui nomment le domaine, la technologie, l'action et le
déclencheur, dans les deux langues. Exemple :

> Convertit un PDF scanné en PDF cherchable en y ajoutant une couche de texte
> OCR. OCR a scanned PDF into a searchable PDF without touching the original
> image layer. Repose sur Tesseract et Ghostscript, en ligne de commande, avec
> traitement par lot et détection automatique de la langue. À utiliser quand un
> document est une image et qu'il faut pouvoir le chercher, l'extraire ou
> l'indexer.

Ce qui ne marche pas : « Un outil utile pour les PDF. » Aucun mot-clé, aucun
déclencheur, ne ressortira jamais d'une recherche.

### `body` -- cinq sections, dans cet ordre

````markdown
## Objet
Ce que fait l'outil, en deux ou trois phrases, plus précis que le summary.

## Cas d'usage
- trois à six puces concrètes, une tâche réelle par puce

## Prérequis
Runtime, version minimale, dépendances système, clés d'API, OS supportés.
Écris "non documenté" si la source ne le dit pas.

## Commandes clés
```bash
# deux à six invocations réellement utiles, copiées de la doc
```

## Limites
Ce que l'outil ne fait pas, les pièges connus, la maturité, la licence si elle
contraint l'usage.
````

### `keywords`

Les termes qu'une recherche future emploiera, dans les deux langues :
`["ocr", "pdf", "texte cherchable", "searchable pdf", "tesseract", "numerisation", "scan", "ligne de commande", "cli", "batch", "python"]`.
Inclus le nom de l'outil, sa techno, son domaine, et deux ou trois synonymes.
Pas de `outil`, `tool`, `logiciel`, `utile` : ces mots ne discriminent rien.

## Étape 4 -- envoyer

Appelle le tool MCP **`toolbox_index`** (nom complet sous Claude Code :
`mcp__kleos__toolbox_index`) avec exactement cette forme :

```json
{
  "key": {
    "git_remote": "git@github.com:ocrmypdf/OCRmyPDF.git",
    "local_path": "/opt/tools/OCRmyPDF",
    "host": "nuc-01"
  },
  "kind": "cli",
  "name": "OCRmyPDF",
  "summary": "...",
  "body": "## Objet\n...",
  "keywords": ["ocr", "pdf", "..."],
  "canonical_url": "https://github.com/ocrmypdf/OCRmyPDF",
  "commit": { "sha": "8e79a9f...", "time": 1757635200 },
  "tags": ["ocr"],
  "notes": "clone de travail",
  "force": false
}
```

Champs absents plutôt que vides : n'envoie pas `"commit": null` ni
`"canonical_url": ""`, omets la clé.

Si le tool MCP n'est pas disponible, le repli est `POST /toolbox/entries` sur le
serveur Kleos avec le même corps (voir `docs/toolbox/README.md`).

### Écrasement de la fiche partagée

Le serveur répond un `outcome` :

| `outcome` | Sens |
|---|---|
| `inserted` | première indexation de cette clé |
| `updated` | ta fiche a remplacé la précédente (ton commit est plus récent) |
| `kept` | la fiche stockée a gagné (son commit est plus récent, ou tu n'as pas envoyé de commit) |
| `unchanged` | contenu identique à ce qui était stocké |

Ton emplacement est enregistré dans **tous** les cas.

`force: true` ne se pose que si l'utilisateur demande explicitement d'écraser
une fiche qu'il juge mauvaise. Ne le pose jamais de ta propre initiative : la
fiche appartient à tout le monde.

## Étape 5 -- rendre compte

Trois lignes, pas plus :

```
Cle      : github.com/ocrmypdf/ocrmypdf (git)
Resultat : inserted, embedding calcule
Ou       : nuc-01:/opt/tools/OCRmyPDF
```

Ajoute une quatrième ligne **seulement** s'il y a une réserve à signaler :
lecture partielle, README absent, indexation faite sans clone, `embedded:
false` (aucun embedder côté serveur, la recherche sera en plein texte seul).
