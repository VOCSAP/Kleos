---
name: tool-find
description: Search the Kleos toolbox catalog for an already-indexed tool matching a task, and report where it lives (URL, host, local path). Use when asked whether a tool exists for something, which tool to use, or where a known tool is installed.
---

# Chercher un outil dans la toolbox Kleos

Tu réponds à « a-t-on déjà un outil pour X ? ». Le catalogue ne contient que les
outils que **cet utilisateur** a indexés ou possède : une recherche vide
signifie « rien d'indexé ici », pas « cet outil n'existe pas ».

**Entrée** : une description de tâche, en langage naturel, éventuellement floue.

## Règles non négociables

1. **N'invente aucun outil.** Si la recherche ne rend rien, dis-le. Ne propose
   pas un outil connu de ta mémoire d'entraînement en le faisant passer pour un
   résultat du catalogue.
2. **Ne cite jamais un emplacement qui n'est pas dans la réponse.** Les chemins
   et hôtes viennent du champ `locations`, pas d'une supposition.
3. **Cinq candidats au maximum**, même si la fusion en produit vingt.

## Étape 1 -- deux reformulations

Transforme la demande en **deux requêtes courtes** (5 à 12 mots chacune), pas
une phrase complète, pas une question :

- une **en français**, avec le vocabulaire métier ;
- une **en anglais**, avec le vocabulaire technique usuel du domaine.

Les deux doivent contenir l'action et l'objet, pas de mot vide. Exemple, pour
« il faut que je retrouve du texte dans de vieux PDF scannés » :

| Langue | Requête |
|---|---|
| FR | `ocr pdf scanne texte cherchable` |
| EN | `ocr scanned pdf searchable text extraction` |

Une troisième reformulation n'est utile que si la demande recouvre deux domaines
franchement distincts. Sinon, deux suffisent.

## Étape 2 -- interroger

Appelle **`toolbox_find`** (nom complet sous Claude Code :
`mcp__kleos__toolbox_find`) **une fois par reformulation**, avec `limit: 8` :

```json
{ "query": "ocr pdf scanne texte cherchable", "limit": 8 }
```

Filtres, à n'utiliser que si l'utilisateur les a formulés :

- `"kind": "cli"` -- restreint à une famille (`repo`, `skill`, `plugin`, `cli`,
  `mcp`, `doc`, `other`) ;
- `"tags": ["infra"]` -- ne garde que les outils dont **un de tes emplacements**
  porte tous ces tags ;
- `"rerank": true` -- reranking LLM, plus lent, sans effet si le serveur n'a pas
  de reranker configuré. Ne le pose que pour une demande ambiguë sur un gros
  catalogue.

Ne filtre pas « pour aider » : un filtre posé à tort transforme un résultat
correct en zéro résultat.

Si le tool MCP n'est pas disponible, le repli est `POST /toolbox/find` (voir
`docs/toolbox/README.md`).

## Étape 3 -- fusionner

Les deux réponses se recouvrent. Fusionne **par `tool_key`** (champ
`results[].tool.tool_key`), jamais par nom : deux fiches peuvent porter le même
nom, une même fiche peut avoir été renommée.

1. Un `tool_key` vu dans les deux réponses passe devant : deux formulations
   indépendantes qui convergent sont le signal le plus fort dont tu disposes.
2. À égalité de présence, trie par `score` (la fusion RRF ; plus haut = mieux).
   `fts_score` est le bm25 brut (négatif, plus bas = mieux) et `vector_score` le
   cosinus brut : ils servent à comprendre *pourquoi* un résultat est là, pas à
   trier.
3. Écarte les résultats manifestement hors sujet, même bien classés. Un
   catalogue étroit remonte toujours quelque chose ; le score dit « le moins
   mauvais », pas « pertinent ».
4. Garde les **5 premiers**.

## Étape 4 -- présenter

Un bloc par candidat, dans l'ordre. Quatre lignes maximum chacun :

```
1. OCRmyPDF (cli)
   Ajoute une couche de texte OCR a un PDF scanne pour le rendre cherchable.
   Ou : nuc-01:/opt/tools/OCRmyPDF | https://github.com/ocrmypdf/OCRmyPDF
   Pourquoi : "ocr" et "pdf cherchable" sont dans ses mots-cles et son resume.
```

- **Ligne 1** : `tool.name` et `tool.kind`.
- **Ligne 2** : une phrase, tirée de `tool.summary` (raccourcis-la, ne la
  réécris pas).
- **Ligne 3** : les emplacements. Pour chaque entrée de `locations` :
  `host:local_path`. Ajoute `tool.canonical_url` s'il est présent. Si
  `locations` est vide, écris `Ou : emplacement non enregistre`.
- **Ligne 4** : pourquoi ça matche, en une phrase, appuyée sur ce que tu as lu
  dans la fiche -- pas sur le score.

Termine par une ligne d'action seulement si elle est utile : quel candidat tu
retiendrais, ou quelle commande lancer ensuite. Pour obtenir la fiche complète
d'un candidat (le `body` entier), appelle `toolbox_get` avec son `tool.id`.

## Étape 5 -- le cas vide

Si les deux requêtes rendent `"count": 0`, réponds exactement dans cet esprit,
sans meubler :

> Rien dans la toolbox pour « <la demande> ». Deux requêtes essayées :
> `<requête FR>` et `<requête EN>`.
>
> Ce n'est pas une preuve d'absence : le catalogue ne contient que ce qui a été
> indexé depuis vos postes. Je peux indexer un outil maintenant (skill
> `tool-index`) si vous en avez un en tête.

Puis, **et seulement si l'utilisateur le demande**, tu peux proposer des pistes
hors catalogue -- en disant clairement qu'elles ne viennent pas de Kleos.
