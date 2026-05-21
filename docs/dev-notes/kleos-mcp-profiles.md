# Profils d'allowlist `kleos-mcp` (Patch 18 candidate)

**Status** : Propose, non valide en runtime.
**Sources** :
- `docs/dev-notes/kleos-mcp-routes-inventory.csv` (474 routes, commit `00f9d88`)
- `docs/dev-notes/kleos-mcp-routes-classified.csv` (298 llm-runtime / 161 operator / 15 system)
- `docs/dev-notes/kleos-mcp-routes-usage-30d.txt` (14 jours d'audit LXC 121, 3814 hits, 45 paths distincts)

---

## 1. Faits saillants du cross-check usage

- **45 routes utilisees / 474 catalogues = 9.5 %** -- la registry actuelle est largement sur-dimensionnee.
- **Top 8 routes = 92 % du trafic** : `memory.recall`, `memory.store`, `memory.search`, `memory.list`, `memory.get`, `growth.observations`, `memory.mark_forgotten`, `graph.view`.
- **0 route classee "operator" appelee massivement** -- la classification est validee.
- Routes appelees mais hors `ROUTES` (drift kleos-client vs kleos-server, hors scope Patch 18) :
  - `/broca/ask` (13 hits)
  - `/broca/actions/{id}/narrate` (1 hit)
  - `/openapi.json` (3 hits)

Implication : un profil **Minimal** ciblant les top 15 tools couvrirait deja >95 % du trafic reel observe.

---

## 2. Convention de syntaxe

L'env var `KLEOS_MCP_TOOL_ALLOWLIST` recoit une liste CSV de patterns :

- Match exact : `memory.store`
- Suffix wildcard : `memory.*` (match `memory.store`, `memory.recall`, etc.)
- Pas d'autre forme (`?`, `[abc]`, `**`, prefix wildcard) -- matcher manuel volontairement minimal.

Empty / unset : comportement upstream (toutes les routes exposees).

Le filtre s'applique uniquement aux **noms canoniques** ; les aliases suivent leur canonical (donc si `memory.store` matche, `memory_store` apparait aussi dans `tools/list`).

Le filtre porte uniquement sur `registry()` (response a `tools/list`). `dispatch()` reste permissif : un client connaissant un name hors allowlist peut quand meme appeler `tools/call` (la securite reste cote `kleos-server` scope check).

### Aliases historiques surprenants

Quelques aliases upstream sont nommes avec un prefixe `memory_*` mais pointent vers des canonicals dans d'autres categories. Ils apparaissent donc dans la registry filtree lorsque leur canonical est allowlist, meme si on n'a pas explicitement liste `memory_*` :

- `memory_context` -> canonical `context.build` (matche pattern `context.*`)
- `memory_entities` -> canonical `graph.list_entities` (matche pattern `graph.list_entities`)
- `memory_projects` -> canonical `projects.list` (matche pattern `projects.list`)

Ce n'est **pas un leak** : l'alias est legitime, juste mal nomme historiquement. Validation peer 2026-05-21 (memoire Kleos #2933) a confirme ce comportement attendu.

---

## 3. Profil `Minimal` (~15 tools)

**Cible** : agent LLM avec besoins memoire + assemblage contexte uniquement. Suffisant pour un agent type "knowledge worker" sans skill execution ni graph exploration.

**Patterns** :

```
memory.*,context.*,skill.search,brain.query,graph.search,activity.report
```

**Tools effectifs** (~30) : `memory.*` couvre 24 tools ; le reste : `context.build`, `context.build_stream`, `skill.search`, `brain.query`, `graph.search`, `activity.report`. Plus les aliases backward-compat (~5 supplementaires).

**Couverture du trafic observe** : ~99 %.

---

## 4. Profil `Standard` (~60 tools) -- **recommande par defaut**

**Cible** : agent LLM productif. Memory + context + skills mature + brain + graph navigation + conversations + reporting basique.

**Patterns** :

```
memory.*,context.*,skill.*,brain.*,graph.search,graph.view,graph.communities,graph.list_entities,graph.entity_search,conversations.*,growth.observations,growth.reflect,growth.materialize,activity.report,broca.feed,broca.ask,intelligence.duplicates,projects.list,fsrs.recall_due,chiasm.generate_plan,tasks.list_tasks,search.*
```

**Tools effectifs** (~65) :
- `memory.*` (24) + aliases (~6)
- `context.*` (2)
- `skill.*` (4) -- search, execute, upload, fix, derive, stats, lineage
- `brain.*` (7) -- query, stats, absorb, dream, feedback, decay, evolution
- `graph.{search,view,communities,list_entities,entity_search}` (5)
- `conversations.*` (10)
- `growth.{observations,reflect,materialize}` (3)
- `activity.report` (1)
- `broca.{feed,ask}` (2 -- si jamais portees cote kleos-client, sinon ignore)
- `intelligence.duplicates` (1)
- `projects.list` (1)
- `fsrs.recall_due` (1)
- `chiasm.generate_plan` (1)
- `tasks.list_tasks` (1)
- `search.*` (3)

**Recommandation operateur** : **profil par defaut**. Couvre 100 % du trafic LLM observe + extensions raisonnables pour les usages futurs (graph exploration, conversations, tasks).

---

## 5. Profil `Advanced` (~140 tools)

**Cible** : agent LLM autonome / power user. Standard + skill cloud complet + intelligence pipelines + activity tracking complet + approvals (read).

**Patterns** :

```
memory.*,context.*,skill.*,skills.*,brain.*,graph.*,conversations.*,growth.*,activity.*,broca.*,intelligence.*,projects.*,fsrs.*,chiasm.*,tasks.*,search.*,ingestion.*,episodes.*,handoffs.*,sessions.*,scratchpad.*,structural.*,pack.*,batch.*,gate.check,inbox.list,approvals.list_pending,audit.list,health.get_metrics
```

**Tools effectifs** (~140) :
- Tout Standard
- `skills.*` (43) -- catalog cloud complet
- `intelligence.*` (41) -- pipelines memoire avances
- `graph.*` complet (26) au lieu du sous-ensemble
- `ingestion.*` (11) -- bulk import / upload
- `episodes.*` (6) + `handoffs.*` (7) + `sessions.*` (5) + `scratchpad.*` (5)
- `structural.*` (5)
- `pack.*` (1) + `batch.*` (1)
- Lecture seule sur system/operator :
  - `gate.check`, `inbox.list`, `approvals.list_pending` -- introspection workflow
  - `audit.list` -- introspection logs
  - `health.get_metrics` -- introspection serveur

**Volontairement exclus** d'Advanced (rester chez `operator` non expose) :
- Tout `admin.*` (95 tools) -- destructif ou intrusif (rebuild_fts, reset, migrations, etc.)
- `identity.*`, `identity_keys.*`, `identities.*`, `auth_keys.*`, `users.*`, `agents.*`, `onboard.*` -- gestion identites (27 tools)
- `security.*`, `policy.*`, `commerce.*` -- compliance / quotas (13 tools)
- `portability.*` -- export / backup (9 tools)
- `webhooks.*` -- integration externe (5 tools)
- `errors.*`, `supervisor.*` -- ops only (4 tools)
- `axon.*`, `loom.*`, `soma.*`, `thymus.*`, `personality.*`, `dispatch.*` -- services Syntheos internes (66 tools, accedes via les services superieurs comme `chiasm.*` ou `broca.*`)
- `prompts.*`, `grounding.*`, `artifacts.*` (15 tools) -- admin tooling
- `well_known.*`, `mcp_schema.*`, `docs.*`, `gui.*` (12 tools) -- exposes via les transport HTTP / SPA, pas via MCP

**Si vraiment besoin** d'Admin ponctuel, utiliser `kleos-cli` ou curl direct, pas MCP.

---

## 6. Configuration MCP type

Dans `.mcp.json` ou config equivalente :

```json
{
  "mcpServers": {
    "kleos": {
      "command": "/path/to/kleos-mcp",
      "env": {
        "KLEOS_URL": "http://192.168.10.21:4200",
        "KLEOS_API_KEY": "eg_xxxxx",
        "KLEOS_MCP_TOOL_ALLOWLIST": "memory.*,context.*,skill.*,brain.*,graph.search,graph.view,graph.communities,graph.list_entities,graph.entity_search,conversations.*,growth.observations,growth.reflect,growth.materialize,activity.report,intelligence.duplicates,projects.list,fsrs.recall_due,chiasm.generate_plan,tasks.list_tasks,search.*"
      }
    }
  }
}
```

Pour reverter au comportement upstream complet : supprimer la cle `KLEOS_MCP_TOOL_ALLOWLIST` (ou la laisser vide).

---

## 7. Decision operateur attendue

- [x] Validation des 3 profils (Minimal / Standard / Advanced).
- [x] Choix du profil par defaut applique en pratique apres deploiement : **Standard**.
- [ ] Validation par usage empirique (etape 6 du plan parent, 2-3 jours d'observation sur le poste local).

Si Standard se revele trop large ou trop restreint apres l'observation, ajuster les patterns sans rebuild (juste env var). Si une route attendue est manquante, l'ajouter au CSV. Si une route polluante reste, la signaler ; potentiellement reduire `memory.*` en `memory.{store,recall,search,list,get,update,delete,mark_archived,mark_forgotten,adjust_importance}` (10 explicites).

---

## 8. References

- `docs/dev-notes/kleos-mcp-allowlist-plan-todo.md` (plan parent 6 etapes)
- `docs/dev-notes/kleos-mcp-usage.md` (reference complete kleos-mcp)
- `docs/dev-notes/kleos-mcp-routes-inventory.csv` (etape 1)
- `docs/dev-notes/kleos-mcp-routes-classified.csv` (etape 2)
- `docs/dev-notes/kleos-mcp-routes-usage-30d.txt` (etape 3)
