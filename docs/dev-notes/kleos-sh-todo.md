# kleos-sh -- TODOs et evolutions a planifier

**Date :** 2026-05-13

Ce fichier regroupe les evolutions et bugs ouverts cote `kleos-sh` (client de gate
Windows + hook PreToolUse Claude Code). Pour le contexte architectural, voir
`wiki/Gate-and-Approvals.md` et l'historique de patches dans `local-patches.md`.

---

## TODO-1 -- Message d'erreur "gate unreachable" trompeur sur timeout HTTP

### Symptome

Quand `curl --max-time` expire avant la reponse serveur (cas typique : tool dans
`TOOLS_REQUIRING_APPROVAL` et `engram-approval-tui` non lance), `kleos-sh`
affiche :

```
EIDOLON GATE DENIED: kleos-sh: gate unreachable after retries
(gate check returned http=0 curl_exit=28 body=""
 curl_trace="... Established connection ... 0 bytes received ...");
failing CLOSED.
```

### Probleme

Le terme **"unreachable"** est techniquement faux et trompeur pour l'utilisateur :
la couche TCP a fonctionne (connexion etablie, requete uploadee), c'est
uniquement la reponse HTTP qui n'est pas arrivee dans le delai imparti. Cas
typiques ou ce comportement est attendu :

1. Tool dans `TOOLS_REQUIRING_APPROVAL` (Bash, Write, Edit, WebFetch, WebSearch)
   et aucun `engram-approval-tui` actif -> serveur attend 120s puis renvoie
   denied, mais `curl --max-time 12` drop bien avant.
2. Tool dans `TOOLS_REQUIRING_APPROVAL` mais `KLEOS_SH_APPROVAL_TIMEOUT_SECS`
   inferieur a l'eventuel temps de reflexion humain.

### Reformulation proposee

Distinguer trois cas dans le message d'erreur cote `kleos-sh/src/main.rs`
(fonction `deny_and_exit` et le bloc qui construit le message dans le match
`got = None`) :

| Condition curl | Ancien message | Nouveau message |
|---|---|---|
| `curl_exit=7` (CURLE_COULDNT_CONNECT) | "gate unreachable" | "gate unreachable (TCP connect failed)" |
| `curl_exit=28` (CURLE_OPERATION_TIMEDOUT) avec stderr montrant "Established connection" et "0 bytes received" | "gate unreachable" | "gate timed out waiting for response (server may be holding for approval; ensure engram-approval-tui is running or raise KLEOS_SH_APPROVAL_TIMEOUT_SECS)" |
| `curl_exit=28` sans "Established" | "gate unreachable" | "gate connect timed out" |
| `http != 200/201` | "gate check returned http=N" | (inchange) |

Implementation : parser `stderr` (trace verbeuse curl) dans `gate.rs::send_request`
pour detecter la presence des marqueurs "Established connection" et
"0 bytes received", et renvoyer une variante du `Err(String)` plus parlante. Le
caller (main.rs) reformule le banner `EIDOLON GATE DENIED:` en consequence.

### Confiance

Confirme reproductible 2026-05-13 sur poste Windows desktop-7b2civn lors du
debug du gate (cf Kleos #402, #401).

---

## TODO-2 -- Retries en mode exec polluent les logs serveur

### Symptome

`kleos-sh.exe --gate-only -c "echo test"` (mode exec, non-hook) declenche
**4 tentatives** avec backoff exponentiel (250ms, 500ms, 1000ms) si la premiere
echoue. Cf `kleos-sh/src/main.rs:406` : `let max_attempts: usize = if
cli.claude_hook { 1 } else { 4 };`.

Chaque tentative cree un nouveau `gate_id` cote serveur. Sur un tool require-approval
sans TUI active, ca produit **4 entrees "gate: DENIED/TIMEOUT" dans
`/var/log/kleos-server.log`** par appel rate, et le serveur a fait 4 attentes de
120s en parallele (en serie, en fait, puisque kleos-sh sequence les retries).

### Pistes de fix

- **Option A** : reduire `max_attempts` a 1 quand le tool est dans
  `TOOLS_REQUIRING_APPROVAL` (le retry est inutile si l'humain n'a pas approuve
  la premiere fois).
- **Option B** : faire que le serveur deduplique les `gate_id` quand un agent
  re-envoie un check identique en moins de N secondes (memoization).
- **Option C** : laisser le client envoyer un gate_id stable (UUID v4 genere
  cote `kleos-sh`) en header et le serveur reuse le canal pending au lieu d'en
  creer un nouveau.

Option A est la plus simple et n'impacte que `kleos-sh`. Options B/C necessitent
des changements serveur.

### Confiance

Confirme 2026-05-13 par lecture du code main.rs:406 (cf Kleos #402).

---

## TODO-3 -- GUI Kleos n'expose pas les gate approvals

### Etat actuel

La GUI Svelte (`gui/src/routes/`) expose les routes `/`, `/gui`, `/graph`,
`/search`, `/inbox`, `/timeline`, `/entities`, `/projects`. Aucune ne couvre les
gate approvals : `grep -r gate gui/src` = 0 resultat.

`/inbox` traite des memories en review (workflow distinct des gates).

### Mecanisme alternatif

L'approval se fait actuellement via le crate `kleos-approval-tui` (binaire
`engram-approval-tui`, ratatui + crossterm). Voir wiki/Gate-and-Approvals.md et
wiki/Eidolon-TUI.md.

### Evolution possible

Ajouter une vue `/gates` ou `/approvals` dans la GUI Svelte qui consomme
l'endpoint approval pending et expose les boutons Approuver/Refuser. Surtout
utile pour les utilisateurs qui prefereraient ne pas avoir un terminal TUI
permanent (ex: travail multi-projets ou la TUI dediee devient encombrante).

Non bloquant tant que la TUI fonctionne. A discuter avant priorisation.

---

## Reference croisee

- `docs/dev-notes/local-patches.md` -- patches locaux deja appliques (incluant
  les patches kleos-sh Windows)
- `docs/dev-notes/credd-todo.md` -- TODOs cote credd
- `wiki/Gate-and-Approvals.md` -- documentation du gate et de l'approval flow
- `wiki/Eidolon-TUI.md` -- documentation de la TUI d'approval
- `kleos-sh/src/gate.rs` -- code du client gate (commentaire historique des
  ATTEMPTS 1..7 expliquant la decision de retirer `--local-port` sous Windows)
- `kleos-sh/src/main.rs` -- entrypoint, retry logic, fail-open policy
