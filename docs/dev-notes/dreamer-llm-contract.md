# Dreamer / Intelligence layer -- contrat avec Ollama

Notes consolidees apres l'enquete du 2026-05-13 sur les phases dream muettes.
Sert de reference pour tout switch de modele LLM cote serveur, ajout de tests,
ou eventuelle migration vers un proxy (LiteLLM).

## Qui appelle Ollama cote kleos-server

Trois consommateurs distincts dans le dreamer (`kleos-server/src/dreamer.rs`),
tous declenches a chaque tick du cycle (toutes les 5 minutes par defaut) :

1. **`intelligence::growth::reflect()`** -- self-observation periodique.
   Genere une observation 1 a 3 phrases sur l'activite recente du serveur.
   Stockee en `memories.category='growth' importance=7 source='dreamer-growth'`.
   Probabilite de declenchement : `GROWTH_REFLECT_CHANCE = 0.2` (20 %), gate
   suplementaire `>= 50 nouvelles memoires dans la derniere heure`.

2. **`intelligence::reflections::generate_reflections_with_llm()`** -- scan des
   memoires high-importance jamais recallees (>= 7 jours, `recall_hits=0`).
   Le LLM propose une action `enrich` / `reconsolidate` / `archive` plus un
   rationale court. Fallback heuristique si LLM indisponible.

3. **`brain::dream::cycle()`** -- consolidation algorithmique (replay, merge,
   prune, discover, decorrelate, resolve). N'appelle pas Ollama directement,
   mais son output (`DreamCycleResult`) est passe en contexte a `growth::reflect()`
   via `build_dream_context()`.

## Contrat exact avec Ollama

Code : `kleos-lib/src/intelligence/llm.rs::call_llm()`.

**Endpoint** :
```
POST $KLEOS_LLM_URL    (defaut: http://localhost:11434/api/generate)
```

**Important : appel en format Ollama NATIVE, pas OpenAI.** Le serveur attend
`/api/generate`, pas `/v1/chat/completions`.

**Body de requete** :
```json
{
  "model": "<KLEOS_LLM_MODEL>",
  "system": "<system prompt>",
  "prompt": "<user prompt>",
  "stream": false,
  "options": {
    "temperature": 0.7,
    "num_predict": 300
  }
}
```

**Reponse lue** :
```json
{
  "response": "<texte utile>"
}
```

Le code lit **uniquement** le champ JSON top-level `response`. Il **n'inspecte
jamais** `thinking`, `message.content`, ni aucun autre champ. Si `response` est
vide ou absent, l'appel est traite comme un echec silencieux (retour `None`,
log info `growth_nothing_observed`).

## Validation de la sortie

Code : `kleos-lib/src/intelligence/growth.rs::validate_observation()`.

```rust
let trimmed = text.trim();
if trimmed.len() < 10 || trimmed.len() > 500 { return false; }
if trimmed.to_uppercase() == "NOTHING" { return false; }
if trimmed.starts_with("I don't") || trimmed.starts_with("There is nothing") { return false; }
true
```

Tout output qui sort de ces clous est jete sans erreur visible. La cellule de
recolte cote DB ne fait `INSERT` que si la validation passe.

## Profil du modele attendu

| Critere | Valeur |
|---|---|
| Mode reasoning / thinking | **Non** (sortie directe dans `response`) |
| Taille | 2 a 4 B parametres suffisent (`num_predict=300`) |
| Suit des instructions courtes (1 a 3 phrases) | Oui |
| Latence cible | < 5 s par appel (cycle toutes les 5 min) |
| VRAM | < 3 GB pour coresidence avec embeddings |
| Compatible Ollama `/api/generate` | Oui (modeles standards le sont tous) |

**Default code** : `llama3.2:3b` (~2 GB, non-thinking, instruction-following
classique). Le code a ete dimensionne pour ce profil par les auteurs.

## Modeles a eviter

Tous les modeles **reasoning / thinking / chain-of-thought** :
- `qwen3.5:*` (toute taille -- mode thinking active par defaut)
- `deepseek-r1:*`
- toute variante `*-thinking`, `o1-style`, `*-r1`

Symptome typique : `response = ""`, tout le contenu dans le champ `thinking`,
`done_reason = "length"` avec un budget `num_predict=300` consomme avant que
le modele commence a ecrire sa reponse. La validation echoue silencieusement
et les phases dream produisent **zero** observation stockee.

## Modeles compatibles teste / recommandes

| Modele | VRAM | Latence type | Notes |
|---|---|---|---|
| `llama3.2:3b` | ~2 GB | < 2 s | Default code, recommande |
| `qwen2.5:7b-instruct` | ~5 GB | ~3 s | Non-thinking (qwen 2.5, **pas** 3.5) |
| `gemma2:2b` | ~1.6 GB | < 2 s | Ultra leger |
| `mistral:7b-instruct` | ~4 GB | ~3 s | Eprouve, non-thinking |

## Variables d'environnement cote serveur

Dans `/etc/kleos/kleos.env` (LXC 192.168.10.21) :

```env
# Endpoint Ollama native pour le dreamer / intelligence layer
KLEOS_LLM_URL=http://192.168.10.16:11434/api/generate
KLEOS_LLM_MODEL=llama3.2:3b

# Alias OpenAI-compatible utilise par d'autres modules (embeddings, sidecar)
OLLAMA_URL=http://192.168.10.16:11434/v1/chat/completions
OLLAMA_MODEL=llama3.2:3b
```

Le pont `KLEOS_*` -> `ENGRAM_*` est fait au demarrage par
`kleos_lib::config::migrate_env_prefix()`, donc les deux prefixes sont
equivalents.

Autres vars optionnelles (tunes du dreamer) :

| Variable | Defaut | Effet |
|---|---|---|
| `KLEOS_EIDOLON_GROWTH_INTERVAL` | 3600 s | Periode de self_reflect |
| `KLEOS_EIDOLON_GROWTH_OBSERVATION_LIMIT` | 100 | Observations fetchees pour anti-repeat |
| `KLEOS_DREAM_IDLE_THRESHOLD_SECS` | 60 | Skip si serveur recu requete < N s |
| `OLLAMA_TIMEOUT_BG_MS` | 60000 | Timeout des appels LLM background |

## Bug observe 2026-05-13 (qwen3.5:4b muet)

Symptome : phases dream silencieuses depuis le 2026-05-07, derniere
`growth_observation` stockee = #89 datant de cette date.

Cause racine en deux temps :
1. Modele en config : `qwen3.5:4b` = thinking model. `response = ""`,
   `validate_observation()` echoue sur 100 % des appels.
2. Hang transitoire d'Ollama sur le worker chat `qwen3.5:4b` (worker bloque,
   modele en VRAM mais inference jamais initiee, TTFB = 0 s sur 120 s).
   Resolu par decharge / recharge automatique apres `keep_alive=0`. Root
   cause precise non capturee (suspect : GPU OOM partiel, file de requetes
   saturee, ou hang specifique au modele apres charge prolongee).

Symptome identique deja vu sur `crawl4ai-rag-mcp` Bug III.1 (Kleos memory #427)
ou le rerank `qwen3.5:4b` timeoutait a 300 s sur 2 snippets. Pattern : Ollama
0.20.6 sur 192.168.10.16, modeles famille `qwen3.5`, hang ou comportement
degrade.

Fix applique : switch vers `llama3.2:3b` dans `KLEOS_LLM_MODEL` + `OLLAMA_MODEL`
sur LXC 192.168.10.21, puis `systemctl restart kleos-server`.

Validation : `tail -F /var/log/kleos-server.log | grep -iE 'growth|reflect'`
doit afficher des `growth_observation_stored` apres un cycle dreamer (max 5 min).

## Carence tests identifiee

`kleos-lib/src/intelligence/growth.rs` (lignes 441+) contient 4 tests unitaires
sur `validate_observation` pure, **aucun** test d'integration qui :
- Appelle reellement Ollama avec la config en cours.
- Verifie que `response` n'est pas vide.
- Verifie que la response passe `validate_observation()`.

Consequence : un switch de modele vers un thinking model (ou un modele dont
le prompt format est incompatible) n'est detecte que par observation manuelle
de l'absence de `growth_observation_stored` dans les logs.

**TODO ouvert** : ajouter au choix
- (a) Un script bash de smoke-test deployable sur le LXC, qui lit `kleos.env`,
  envoie le prompt EXACT que `growth::reflect` enverrait, et verifie le
  `response` non-vide + validation. Exploitable en cron ou healthcheck
  pre-deploy.
- (b) Un test d'integration Rust `kleos-lib/tests/llm_contract.rs` marque
  `#[ignore]`, lance via `cargo test -- --ignored`, necessite Ollama
  joignable sur `KLEOS_LLM_URL`.

Recommandation : implementer (a) en priorite pour pouvoir verifier la prod
sans recompilation, (b) si on veut bloquer un release CI sur regression.

## Question ouverte : LiteLLM en proxy

LiteLLM Proxy expose uniquement des endpoints OpenAI (`/v1/chat/completions`,
`/v1/embeddings`, `/v1/completions`). Il **ne reproduit pas** l'endpoint Ollama
natif `/api/generate`.

Comme `kleos-lib/src/intelligence/llm.rs` hardcode l'usage de `/api/generate`
avec lecture de `response` (top-level), il n'est pas possible de glisser
LiteLLM devant Ollama sans modifier le code.

Deux options si on veut LiteLLM :

**(a) Statu quo cote kleos-server** : garder Ollama direct pour l'intelligence
layer, mettre LiteLLM en facade uniquement pour les autres consommateurs
(Claude Code, kleos-sidecar, agents externes). Pas de refactor cote Kleos.

**(b) Refactor `intelligence/llm.rs` vers format OpenAI** (~50 lignes a
changer) : `POST /v1/chat/completions` avec `{model, messages:[{role:system,...},
{role:user,...}], temperature, max_tokens}`, lire `choices[0].message.content`.
Une fois fait, LiteLLM devient transparent et on peut swapper Ollama / OpenAI /
Anthropic par config. Note : le repo a deja un usage OpenAI-compatible pour
les embeddings (`OLLAMA_URL=/v1/chat/completions` est lu par
`kleos-lib/src/llm/types.rs`), donc la stack est deja hybride. Une migration
unifiee vers OpenAI partout serait coherente.

Decision a prendre quand on aura besoin d'un cas d'usage concret (fallback
inter-provider, observabilite centralisee, A/B modeles).

## References Kleos memory

- #423 : pause point session rebase v1.1.0 (etat des patches locaux)
- #427 : crawl4ai-rag-mcp Bug III.1 (premier signal qwen3.5:4b instable)
- #431 : decouverte initiale phases dream muettes (cycle ok=0 failed=1)
- #435 : double bug qwen3.5:4b (hang transitoire + thinking model)
- #436 : contrat exact `/api/generate` + carence tests
