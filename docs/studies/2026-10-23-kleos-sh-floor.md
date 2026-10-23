# Plancher local minimal de kleos-sh, révision 4

Statut : conception révisée après verdict transmis « IMPLÉMENTABLE AVEC CORRECTIONS », carte `c6a836e6`, 2026-10-23 ; aucun code livré.
Étiquettes : **MESURÉ** = commande/outil et sortie ; **DÉDUIT** = lecture référencée ; **SUPPOSÉ / cible** = proposition non implémentée. Les résultats du challenger non reproduits sont signalés comme tels.

## 1. Cadre, structure actuelle et recommandation

**MESURÉ**, `kleos-cli context "plancher modifié registre 24h" --limit 5` : mémoire `32881`, « cree OU modifie par un agent d'une session du meme workspace » et « Droit de suppression valable 24 h ». B1-B3 remplacent les propositions de révision 2 : exception dans V1, aucun helper de création imposé, registre local autoritaire. A1/A2/A4 maintenus : agent maladroit, Claude Code seul V1/Codex V2, settings inchangé jusqu'à activation. Le risque de supprimer un dossier sensible effectivement touché sous ces racines est accepté.
**DÉDUIT**, `kleos-sh/src/main.rs:402-484` : le hook actuel rejoint clé, HTTP et `gate::check_remote` ; isoler le nouveau mode avant ce chemin. `main.rs:97-123` extrait déjà `tool_input.command`, mais accepte aussi des objets incomplets ; `:415-435` permet une ouverture sur erreur via environnement.
**DÉDUIT**, `kleos-sh/Cargo.toml:13-19` : aucune dépendance déclarée à `kleos-lib` ; un module local `floor` garde la frontière. `kleos-cli/src/hook.rs:566-603` collecte des chemins présents dans l'entrée d'outil, sans preuve de création : ce mécanisme ne constitue pas un registre de propriété.
**DÉDUIT**, `~/.claude/rules/hook-design.md:18` : la convention générale est `claude-hooks` ; le choix explicite de `kleos-sh` est une dérogation. Profils, approbations, `gate_requests`, crate partagée et sort de `kleos-cli hook` restent exclus.

**SUPPOSÉ / cible corrigée :** une V1 complète, unité de grant = enfant répertoire direct D et ses sous-répertoires ; expiration 24 h après dernier constat attribuable au workspace. Inventaire plat des racines pour les créations, chemins Write/Edit pour les écritures profondes, parcours récursif des **seuls D candidats** pour Bash. Le texte sélectionne les candidats, jamais le grant. Hypothèse de travail du lead, réversible par l'opérateur : même une création Bash exige D candidat ; dossiers temporaires aléatoires non nommés manqués.
**SUPPOSÉ / limite :** fail-closed concerne les erreurs du moteur répondant et la politique chargée, pas un confinement complet. Une commande hors familles ciblées peut s'abstenir ; un hook absent/expiré peut laisser le harness poursuivre. A1 accepte les limites contre les contournements délibérés.

## 2. Réponse au challenge

| Correction | Réponse dans cette révision |
|---|---|
| 1. Opacité | §4 : redirections, heredocs et affectations traités sans refus global ; substitutions récursives ; opacité ciblée rm/git. SUPPOSÉ ici : mesure transmise « ≥44,8 % de 2066 commandes » non reproduite. |
| 2. Verbe/wrappers | §4 : casse, quotes, échappements, basename/.exe ; grammaires fermées et dispatch rtk, pas simple suppression du mot rtk. |
| 3. Git | §4 : options courtes groupées, options bénignes avec arité, `--prune` classé delete. |
| 4. Codex | Hors V1 ; V2 nécessite capture de payload et lexique PowerShell. SUPPOSÉ ici : alternance bash.exe WSL/pwsh.exe rapportée par le challenger, non reproduite. |
| 5. Auto-protection | §3/5 : quatre règles minimales obligatoires ; refus de jetons désignant politique/dossier/binaire ; installation hors `.cargo/bin`. |
| 6. Payload | §5 : `cwd` facultatif pour les denies ; `tool_input.command` chaîne confirmée par artefacts T1 Bash lus, les autres sondes restent préalables au code. |
| 7. Migration | §8 : les quatre deny ET `autoMode.soft_deny` suppression récursive sont inventoriés ; aucun changement avant le lot d'activation. |

### Réponse au challenge de faisabilité

**SUPPOSÉ ici / provenance :** chiffres ci-dessous transmis par le lead comme mesures du challenger, non rejoués ici et sans commande/artefact fourni ; ce ne sont pas nos mesures. Le point 8 est une hypothèse de travail du lead, pas un nouvel arbitrage opérateur.

| Correction | Intégration fermée |
|---|---|
| 1. Scans | §6 : inventaire plat avec file id/mtime, cap propre 100 000 entrées par racine ; récursif seulement D candidats, aucun cap 10 000 sur l'inventaire. |
| 2. mtime de D | Jamais une preuve profonde ; le challenger rapporte D inchangé lors de création/modification de `D/a/b.txt`. |
| 3. Orphelines | §6 : clôture PostToolBatch, sinon Stop/UserPromptSubmit suivant, sinon TTL dur ; incertitude locale au workspace, conflit inter-workspaces levé. |
| 4. Background | Post de lancement ferme sans grant, y compris passage en arrière-plan après timeout ; contamination ultérieure explicitement limitée. |
| 5. Workspace | Paire volume/file index du git-common-dir résolu au SessionStart, jamais identité par chaîne ; chemin relatif remesuré ci-dessous. |
| 6. T1 | Sonde headless `claude -p --settings <fichier>` avant tout code, sans toucher `settings.json`. |
| 7. Faux négatifs | Hypothèse de couverture ≥20 % de créations Bash manquées ; 20,1 % de fenêtres concurrentes transmis, pas une mesure directe du taux par création ; reprise par Write dans D. |
| 8. Candidats pour créations | Exigence ajoutée par le lead : pas de grant pour un D créé hors candidats ; 4 `kory-slice-*` du Deck en 30 s auraient sinon été attribués à une fenêtre Bash étrangère. |

## 3. Grammaire V1 de floor.toml

**SUPPOSÉ / cible :** fichier V1 constitué des deux blocs TOML ci-dessous ; identifiants et verbes imposés par le chargeur.

```toml
schema = 1
[[deny]]
id = "rm-recursive"
verb = "rm"
flags_any = ["recursive"]
[[deny]]
id = "git-rewrite"
verb = "git push"
flags_any = ["force", "delete"]
[[deny]]
id = "git-reset-hard"
verb = "git reset"
flags_any = ["hard"]
[[deny]]
id = "git-clean-force"
verb = "git clean"
flags_any = ["force"]
```

**SUPPOSÉ / cible :** champs exacts `{schema, deny, scratch, exception}`, entrées deny exactes `{id, verb, flags_any}` ; version/clé inconnue, doublon, liste vide, règle absente, verbe changé ou drapeau minimal retiré invalident **tout** le fichier. Pas de cache du dernier valide ni fallback embedded. Les quatre tuples ci-dessus sont obligatoires ; une extension restrictive n'est admise qu'avec drapeaux reconnus par le verbe.
**SUPPOSÉ / cible :** vocabulaire canonique : rm `{recursive, force}` ; push `{force, delete}` ; reset `{hard}` ; clean `{force, directories, ignored, only_ignored}`. `flags_any` est une intersection non vide ; tous les hits sont conservés. Aucun `allow`, regex, profil, include ou exception lexicale dans schema 1.
**SUPPOSÉ / cible :** suite du même fichier `schema = 1` ; les révisions précédentes n'étaient pas des formats déployés. Champs scratch et exception exactement ceux présentés ; `ttl_hours` vaut 24, `proof` et les autres constantes sont fermés.

```toml
[scratch]
roots = ["C:/Users/Olivier/AppData/Local/Temp", "C:/Users/Olivier/.agent-forge/scratch"]
ttl_hours = 24
[[exception]]
id = "workspace-touched-directory"
rule = "rm-recursive"
verb = "rm"
flags_allowed = ["recursive", "force"]
proof = "workspace_touch_registry"
```

**SUPPOSÉ / cible :** `scratch` et l'exception sont atomiques ; une racine est absolue locale, sans joker/variable/`..`, ni chevauchement avec une autre racine, politique, registre ou binaire. Racine elle-même jamais supprimable. Seule cette exception rm est admise ; elle exige le constat §6 pour **chaque** opérande. Un registre absent/invalide n'invalide pas les commandes sans hit rm ; il refuse seulement l'exception. Le retrait conjoint de scratch/exception désactive les grants sans affaiblir les quatre denies.

## 4. Normalisation POSIX bornée, non universelle

**DÉDUIT**, `claude-config/hooks-rs/src/hooks/bash_write_target.rs:95-123,181-184` : le poste dispose d'un précédent de masquage d'expansions inertes et de saut des affectations ; ce n'est pas un parseur universel à recopier tel quel.
**SUPPOSÉ / options :** A, refus de toute opacité, réduit le parseur mais étend le rayon de refus à des outils hors cible ; B, scanner structuré puis refus d'opacité seulement rm/git, coûte plus en corpus mais limite cette friction. B recommandé, conformément au challenge ; les deux restent réversibles dans ce seul module.

1. **SUPPOSÉ / cible :** séparer hors quotes `;`, `&&`, `||`, `|`, `|&`, `&`, LF ; conserver provenance des quotes/échappements. Extraire récursivement `$()` et backquotes, **aussi dans les doubles quotes**, jamais dans les simples quotes ; profondeur maximale 4. Examiner toutes les sous-commandes extraites.
2. **SUPPOSÉ / cible :** analyser puis retirer de la liste argv les redirections et leur cible, les corps de heredoc et les affectations de tête. Avant masquage, conserver les jetons de chemins pour §5 et extraire les substitutions exécutables, y compris affectation/redirection et heredoc non quoté. Le texte d'un heredoc n'est pas du code ; si un interpréteur le consomme, traiter son corps comme code inline opaque. Heredoc à délimiteur quoté : aucune expansion shell de son corps.
3. **SUPPOSÉ / cible :** dans une zone syntaxique non comprise, une limite de récursion ou du code inline, rechercher des jetons normalisés `rm` ou `git`, casse ignorée : présence = deny opaque, absence = abstention de cette zone. Ne pas chercher ces mots dans les simples données d'une commande comprise (`echo 'rm -rf /etc'`). Une erreur de transport/payload ou une borne globale dépassée reste deny, distincte de l'opacité de langage.
4. **SUPPOSÉ / cible :** verbe = quotes/échappements retirés, basename POSIX/Windows reconnu, `.exe` retiré, casse ignorée. Conserver la casse des options et des opérandes. Alias utilisateur, expansions dynamiques, binaires renommés et scripts externes ne sont pas résolus. Un jeton opaque n'est pas interprété avec un shell.
5. **SUPPOSÉ / cible :** aucune interdiction générale de `submodule foreach`, `rebase --exec`, `git config alias`, `find`, python, etc. Les trois premières formes git sont explicitement hors analyse de code porté en V1 conformément à A1 ; seule l'auto-protection s'y applique. Autres exécutions inline reconnues : filtre opaque rm/git, pas simulation du langage.

**SUPPOSÉ / cible :** wrappers transparents en boucle, chacun avec parseur d'options fermé ; forme inconnue = zone opaque selon le point 3, pas suppression aveugle d'arguments.

| Wrapper | Grammaire reconnue avant la commande portée |
|---|---|
| `nohup`, `time`, `command` | `nohup [--]` ; `time [-p] [--]` ; `command [-p] [--]`. `command -v/-V` est introspection, pas exécution du token suivant. |
| `timeout` | `[--preserve-status] [--foreground] [-v] [-s signal|--signal=signal] [-k durée|--kill-after=durée] [--] durée` ; variantes longues séparées acceptées, arités obligatoires. |
| `nice`, `stdbuf` | `nice [-n entier|--adjustment=entier] [--]` ; `stdbuf` consomme `-i/-o/-e mode`, formes jointes et `--input/--output/--error=mode`, puis `[--]`. |
| `sudo`, `doas` | Seulement `[--] commande` dans V1 ; autres options = opaque ciblé. Aucune exception V1.1 via privilège. |
| `rtk` | Options globales `-v` groupé, `--ultra-compact`, `--skip-env` ; `git args` devient `git args`. `run -c chaîne` reparse la chaîne ; `run args`, `proxy`, `err`, `test`, `summary` analysent la commande portée après options connues/`--`. Autres sous-commandes hors cible sauf auto-protection. |

**MESURÉ**, `rtk --help` : `run            Execute a shell command via sh -c (raw, no filtering or tracking)` ; `rtk run --help` : `Usage: rtk.exe run [OPTIONS] [ARGS]...` et `-c, --command <COMMAND>` ; `rtk summary --help` : `Usage: rtk.exe summary [OPTIONS] [COMMAND]...`. Les arités des autres dispatchers doivent être verrouillées par fixtures avant implémentation.
**SUPPOSÉ / cible :** rm reconnaît `-r/-R/--recursive`, `-f/--force`, groupes/options en ordre libre, `--`, et **tous** les opérandes ; autres options = opaque ciblé donc deny. Git décompose ses groupes courts avant classification (`-qf`, `-fdx`) et consomme les valeurs d'option avant de chercher flags/refspecs.
**SUPPOSÉ / cible :** options globales git reconnues : `-C chemin`, `-c clé=valeur`, `--git-dir[=]chemin`, `--work-tree[=]chemin`, `--no-pager`, `--paginate`, `--literal-pathspecs`. Ne pas classer les valeurs comme commandes ou refspecs. Hors push/reset/clean, abstention après auto-protection ; pour ces trois verbes, toute option inconnue est opaque ciblée.

| Sous-verbe | Drapeaux refusés et liste initiale d'options bénignes, à arité explicite |
|---|---|
| `push` | force : `-f`, `--force`, `--force-with-lease[=valeur]`, `--force-if-includes`, refspec `+x[:y]` ; delete : `-d`, `--delete`, `--prune`, `:y` ; `--mirror` donne les deux. Bénins sans valeur : `-q/-v/-n/-u`, `--quiet/--verbose/--dry-run/--set-upstream/--all/--tags/--follow-tags/--atomic/--porcelain/--progress/--no-verify/--thin`. Avec valeur : `-o/--push-option`, `--repo`. |
| `reset` | `--hard` refusé ; bénins sans valeur `-q`, `--quiet`, `--soft`, `--mixed`, `--merge`, `--keep`, `--patch`, `-p`, `--no-refresh` ; `--pathspec-from-file` prend une valeur, `--pathspec-file-nul` non. |
| `clean` | chaque `-f`/`--force` donne force ; `-d/-x/-X` canonicalisés. Bénins `-n/--dry-run`, `-q/--quiet`, `-i/--interactive` ; `-e/--exclude` prend une valeur. |

**SUPPOSÉ / cible :** pour tous : séparateur `--` respecté, option longue avec `=valeur` seulement si son arité l'autorise ; refspec force/delete même en fin de push. Un dry-run n'efface pas un hit dangereux. Ni tri de branches, ni appels Git/configuration réseau pour décider.

## 5. Mode local, auto-protection et contrat Claude Code

**SUPPOSÉ / cible :** `kleos-sh --floor-only --floor-file <absolu>` lit stdin et termine sans exécuter la commande ; incompatible avec les anciens modes `-c`, `--gate-only`, `--claude-hook`, laissés inchangés. Aucune dépendance à une clé API, au sidecar, à `KLEOS_SH_FAIL_OPEN` ou au serveur.
**SUPPOSÉ / T1, prérequis avant tout code :** sonde headless `claude -p --settings <fichier>` isolée, sans toucher `settings.json`. T1 Bash déjà réalisée par le lead (version 2.1.309 transmise), artefacts lus ci-dessous ; ne pas la refaire sans manque identifié. Restent à verrouiller chemins Write/Edit, SessionStart/replis, timeout maximal effectif, passage automatique en arrière-plan ; annulation Échap non mesurée en headless.
**DÉDUIT des artefacts T1**, `~/.agent-forge/scratch/hook-probe-t1/events_run1.jsonl:1-5,8,10,13` : `tool_use_id` relie Pre/Post/Failure, command est une chaîne, cwd Windows de session ; échec/timeout donnent Failure (`error`, `is_interrupt=false`, sans tool_response). Batch porte `tool_calls[].tool_use_id`, pas d'id racine ; tool_response **Bash** chaîne dans Batch, objet dans Post. Refus dans Batch : `PreToolUse:Bash hook error: probe deny hook` ; background : `backgroundTaskId`, `duration_ms:8`. `events.jsonl:1-4` situe agent_id/type dans le sous-agent, pas à exiger pour tous les appels ; ignorer les appels Batch non pertinents.
**SUPPOSÉ / cible :** ordre Pre Bash : entrée bornée -> chargeur intégral -> auto-protection -> analyse/agrégation -> exception sur les grants **déjà acquis** -> deny ou snapshot Pre puis silence. Jamais de grant produit et consommé dans le même appel. Payload décision : objet, `hook_event_name=PreToolUse`, `tool_name=Bash`, `tool_input.command` chaîne non vide ; `cwd` facultatif pour les denies. Session/corrélation manquante désactive observation/exception, pas les denies. Entrée décision mal formée, lecture impossible ou politique absente/illisible/invalide = deny.
**SUPPOSÉ / cible :** auto-protection légère : tout jeton chemin, y compris opérande, cible de redirection et valeur de chemin reconnue, désignant politique, dossier de politique (ou descendant) ou binaire installé est refusé, même en lecture. Comparer par composants normalisés/casse Windows, avec alias `C:/` et `/c/`, home non quoté ; pas de `contains`. Un relatif ne se résout que si cwd est disponible ; les alias dynamiques/non résolus restent une limite A1. Ne pas transformer cette règle en contrôle de toutes les écritures.
**SUPPOSÉ / cible :** clone opérateur `%USERPROFILE%/.kleos/gate-rules/`, politique `floor.toml` ; binaire installé à `%LOCALAPPDATA%/Programs/kleos-floor/kleos-sh.exe`, **pas** `.cargo/bin` ni un worktree. Chemins absolus dans le hook ; relecture de la politique à chaque invocation, sans TTL/daemon/serveur. Mise à jour opérateur atomique ou hors activité ; état intermédiaire invalide = refus temporaire.
**SUPPOSÉ / cible :** même binaire, modes locaux distincts décision/observation/session : PreToolUse `Bash` compose décision puis observation ; Pre `Write|Edit|MultiEdit` observe sans décider ; PostToolUse/PostToolUseFailure des mêmes outils finalisent. SessionStart lie l'identité physique ; PostToolBatch, Stop/UserPromptSubmit et SessionEnd ferment les orphelines selon §6, **sans révoquer globalement les grants 24 h**. Hooks synchrones pour que la baseline précède l'outil et que le grant soit visible au suivant ; aucun ordre supposé entre hooks parallèles. Exec `command` + `args`, timeout **5 s** ; deny stdout JSON/exit 0, abstention silence/exit 0, jamais `allow`/`ask`. Erreur d'émission du refus : stderr/exit 2 ; erreur d'observation : diagnostic borné, aucun grant, pas de refus de Write/Edit.

```json
{"hookSpecificOutput":{"hookEventName":"PreToolUse","permissionDecision":"deny","permissionDecisionReason":"floor: rm-recursive"}}
```

**MESURÉ documentaire**, `perform_rag_query(query="Claude Code hooks timed-out hook doesn't block tool call cwd common input fields PreToolUse deny exit code 2", match_count=3)` : `code.claude.com/docs/en/hooks`, « A timed-out `command`, `http`, or `mcp_tool` hook doesn't block the tool call. » Pas une mesure du harness installé.
**SUPPOSÉ / budgets :** stdin décision 256 Kio, politique 64 Kio, 64 règles, 256 commandes/4096 tokens ; dépassement décision = deny. Inventaire plat : **100 000 entrées par racine**, indépendant du récursif candidats (10 000 entrées/250 ms par phase, total candidats) ; scan incomplet = aucun nouveau grant concerné. Estimation 50-60 ms par phase en cas courant, pas p95 garanti ; deadline globale 1 s avant timeout 5 s, incluant SQLite `busy_timeout ≤200 ms`. Mesure SQLite WAL transmise : p50 2,4 ms, non remesurée. Crash/kill/I/O bloquée restent sans garantie absolue ; banc latence **et couverture** obligatoire.

## 6. A3 V1 : registre des répertoires créés OU modifiés

**DÉDUIT**, `kleos-sh/src/observe.rs:29-39` : l'observation actuelle poste au sidecar, donc ne pas la réutiliser pour ce registre local. `kleos-cli/src/hook.rs:566-603` fournit des indices de chemins, pas un constat avant/après. Cible : modules locaux `floor` (décision pure), `scratch_observe` (filesystem/hooks), `scratch_registry` (transactions), sans dépendance à `kleos-lib` ni changement de `kleos-cli hook`.
**SUPPOSÉ / granularité recommandée :** pour chaque racine R, un enfant **répertoire direct D** est l'unité de grant ; écrire `R/D/a/b.txt` touche D. Grant sur D et ses sous-répertoires, jamais sur R ni sur un frère de D. Un fichier directement sous R ne donne aucun grant de répertoire. Ce choix borne le nombre d'entrées ; il autorise aussi les autres contenus de D, conformément au risque accepté de répertoire sensible touché puis supprimé. Pas de propagation vers les ancêtres au-delà de D.

| Mécanisme d'observation implicite | Coût, limites, réversibilité, rayon |
|---|---|
| Inventaire récursif global de révision 3 | **SUPPOSÉ ici / réfuté par mesures transmises :** Temp 21 447 enfants directs, 3,2 M entrées, 137 s ; 198 enfants dépassent seuls 10 000 entrées. Scratch 2 446 enfants, 91 470 entrées, 4,7 s. Coût/rayon incompatibles avec le hook, retrait simple avant implémentation. |
| Inventaire plat seul | **SUPPOSÉ ici :** liste/stat transmis : 34 ms Temp, 4-6 ms scratch ; coût faible, réversible ; repère créations, mais mtime de D insuffisant pour les écritures profondes. Trop de faux négatifs si utilisé seul. |
| Inventaire plat + Write/Edit + candidats Bash | **SUPPOSÉ / retenu :** coût O(enfants directs + entrées des candidats), sans daemon ; couverture réduite mais rayon borné. Candidature n'accorde aucun droit sans constat ; retour au deny pour un candidat non complètement observé. |

**SUPPOSÉ / Write/Edit/MultiEdit :** Pre extrait les chemins déclarés, les résout avec cwd si relatifs, rejette pour l'observation tout chemin hors racines/exclu/résolution ambiguë, conserve identité de D et état du fichier. Post succès corrélé confirme que le fichier a été écrit/modifié (contrat de résultat à sonder), ou un delta de métadonnées ; échec sans delta = aucun grant. Une écriture réussie explicitement confirmée peut renouveler même à contenu identique ; lecture/listing, mention du chemin et `mkdir -p` sans changement ne renouvellent pas. Création de D par Write reconnue si absent avant et présent après.
**SUPPOSÉ / inventaire Bash :** Pre/Post listent seulement les enfants directs des deux racines avec type, file id et mtime, sans descendre. Différence d'appartenance/identité avec listes complètes = constat de création/remplacement de D ; mtime de D **jamais** preuve de modification profonde, même s'il change. Supprimer le cap global 10 000 sur cette liste : sa borne propre est celle de §5. Le constat de création reste soumis à la candidature ci-dessous.
**SUPPOSÉ / candidats Bash :** au Pre, sélectionner les D contenant le cwd, un jeton chemin statiquement résoluble ou une cible de redirection ; ramener chaque chemin à son enfant direct D, y compris D futur absent. Un token égal à R ne sélectionne pas tous ses enfants ; aucun glob/expansion exécuté pour découvrir des candidats. Scanner récursivement **ces seuls D** avant/après : tuples `(chemin relatif, type, identité, taille, last_write, attributs)`, sans last_access/contenu. Texte = sélection seulement, delta = constat. Hypothèse lead : création Bash accordée uniquement si D candidat et réellement apparu, jamais à tous les nouveaux D de l'inventaire.
**SUPPOSÉ / complétude :** modification d'un D existant exige deux parcours candidats complets ; erreur, quota, reparse point = pas de grant dérivé, jamais approximation par mtime de D. Création candidate exige inventaires plats complets et identité post vérifiable ; elle n'exige pas l'ancien parcours d'un D absent. Gros candidats, expansions dynamiques, dossiers temporaires aléatoires non nommés et modification restaurée entre relevés sont des faux négatifs ; un Write confirmé dans D peut ensuite accorder/renouveler le droit. Pas de grant à un outil qui ne fait qu'utiliser/lire un dossier.
**SUPPOSÉ / protocole local :** Pre ouvre une fenêtre `(session_id, tool_use_id)` avant le relevé, ferme la baseline avant de rendre la main ; Post succès/échec ferme après son relevé. Aucun grant avant Post ; effets partiels d'un Bash échoué peuvent compter si un delta valide est constaté. Déduplication par cet identifiant : un Post rejoué ne décale jamais l'expiration. Fenêtre sans Pre, changement d'identité de session, réponse non corrélable = pas de grant. Un `mkdir … ; rm -rf …` dans **le même** Bash ne bénéficie pas de sa création future.
**SUPPOSÉ / concurrence :** Write/Edit attribue seulement ses chemins confirmés ; Bash conserve une fenêtre de conflit sur les deux racines, même si les scans sont candidats. Chevauchement incompatible = pas de nouveau grant Bash concerné ; même dépôt ou binding space commun = compatible. À clôture, lever le conflit pour les futures fenêtres des autres workspaces ; conserver un marqueur d'intersection pour celles déjà chevauchantes jusqu'à leur traitement. Aucun verrou pendant l'outil ; clôture d'une fenêtre n'efface pas rétroactivement un conflit déjà constaté.
**SUPPOSÉ / clôture déterministe :** traiter Post/Failure une fois ; pour Pre sans Post, PostToolBatch ferme les ids présents dans `tool_calls[]` sans accorder de grant de secours. Refus explicite reconnu = sans effet, pas d'incertitude. Sinon, premier Stop ou UserPromptSubmit suivant de la session, SessionEnd ou TTL dur ferme les orphelines ; TTL = timeout Bash maximal effectif du harness + marge 60 s, contrôlé à chaque accès, donc sans daemon. Annulation/inconnu = incertitude seulement sur les D candidats/déclarés de **son propre scope**, pas sur les autres workspaces ; aucune incertitude globale des racines. Un constat direct Write/Edit ultérieur lève cette incertitude pour D. Aucun timeout n'accorde un grant ; Post tardif après clôture ignoré.
**SUPPOSÉ / background :** `run_in_background` ou réponse de lancement/background après timeout => fermer immédiatement au Post de lancement, **sans grant**, sans attendre la fin réelle ; détecter aussi `backgroundTaskId` même si l'entrée ne demandait pas le background. Un vrai timeout Failure sans lancement reste un échec d'exécution selon T1. Échap demeure non mesuré. Contamination par une écriture ultérieure d'un processus détaché ou externe = limite, pas preuve par PID ; les candidats limitent l'attribution parasite sans l'éliminer.
**SUPPOSÉ / couverture :** prévoir **au moins 20 % de faux négatifs pour les créations Bash** comme hypothèse de fonctionnement, pas résultat direct : la mesure transmise porte sur 20,1 % de fenêtres Bash chevauchant un autre dossier projet, pas sur la population des créations. Candidats manquants/quota augmentent encore les refus possibles ; une écriture Write confirmée dans D reprend le droit. Ne pas compter une lecture ou un Write proposé mais non exécuté comme rattrapage. Le cas des 4 dossiers Deck `kory-slice-*`/30 s justifie la restriction aux candidats, hypothèse lead explicite.

**SUPPOSÉ / stockage :** `%LOCALAPPDATA%/kleos-floor/registry.sqlite`, hors racines nettoyables ; tables sessions, observations et grants par `(root_id, D canonique, identité physique, scope)`. Stocker hôte/utilisateur OS, session d'origine, repository_id/binding space, nature du constat, `observed_at`, `expires_at`, statut et révision de politique. SQLite en WAL, transactions courtes, `busy_timeout ≤200 ms`, aucune transaction pendant scans/outil ; publication atomique d'un delta complet. Registre illisible/absent/incompatible = refus de l'exception seulement ; panne Kleos sans effet, aucune synchronisation serveur nécessaire.
**DÉDUIT**, `kleos-cli/src/space.rs:56-84` : le fallback espace est un basename normalisé, donc deux dépôts homonymes peuvent partager un nom sans partager leur identité. `hook.rs:358-367` accepte aussi `KLEOS_SPACE` : ce seul texte ne doit pas accorder le partage.
**MESURÉ**, `git -C 'C:/Users/Olivier/workspace/Kleos' rev-parse --git-common-dir` => `.git` : le résultat local est relatif.
**SUPPOSÉ / portée :** SessionStart résout git-common-dir **contre le cwd de cet appel Git**, ouvre ce répertoire et enregistre `(numéro de série du volume, file index)` comme repository_id. Jamais de comparaison de chaînes : casse, `/c/` versus `C:/`, subst et jonctions d'accès au dépôt doivent rejoindre la même identité physique ; cette résolution ne lève pas les refus de jonctions sur les cibles de suppression. Partage même repository_id **OU** binding opérateur explicite `{serveur, tenant, space_id}`, même hôte/utilisateur OS, sans transitivité ni fallback de nom. Identité figée pour la session ; résolution impossible = pas d'exception, denies actifs. Nouvelle session du même workspace bénéficie des grants non expirés.
**SUPPOSÉ / durée précise :** `observed_at` = heure du Post ayant constaté création/modification ; expiration = `observed_at + 24 h`, borne exclusive. Nouveau constat du même scope renouvelle ; activité quelconque, heartbeat, lecture, contrôle du grant et tentative rm ne renouvellent pas. SessionEnd, pause, fermeture du créateur ou redémarrage n'écourtent pas ce délai. Aucun mécanisme de cohortes/10 min. Retour d'horloge détecté = suspension des grants concernés, jamais prolongation implicite ; changement de politique ne remet pas le compteur à zéro.
**SUPPOSÉ / préexistants :** D préexistant jamais touché depuis instrumentation = aucun grant. D préexistant réellement écrit/modifié par un outil observé = éligible, même sensible, comme demandé. Après expiration, une nouvelle modification constatée peut accorder à nouveau 24 h. Un autre workspace modifiant D obtient son propre scope sans renouveler ceux auxquels il n'appartient pas ; pas de transfert d'autorisation par copie du seul chemin.
**SUPPOSÉ / suppression :** chaque opérande doit être un répertoire D couvert ou un descendant, toujours sous la racine, identité du D inchangée et grant du scope valide. Drapeaux ⊆ `{recursive, force}` ; chemin explicite absolu ou `~/` non quoté ; ni glob (`*`, `/*`), ni variable, ni relatif, ni wrapper privilégié. Refuser cible/ancêtres liens, jonctions, reparse points ou résolution ambiguë ; vérifier aussi absence de reparse point dans l'arbre à supprimer, parcours borné sinon refus. Changement d'identité/renommage ne conserve pas automatiquement le grant : nouveau constat requis.
**SUPPOSÉ / exclusions absolues :** racines elles-mêmes, `%TEMP%/claude`, ses descendants et toute cible ancêtre de ce dossier, politique, registre et binaire ne sont jamais éligibles, même modifiés. Ignorer le sous-arbre harness dès l'observation. La résolution physique doit respecter ces exclusions ; une variante de casse/séparateur ne les lève pas. Aucun contrôle de « propriété de chaque ancien fichier » à réintroduire à l'intérieur d'un D effectivement touché.
**SUPPOSÉ / nettoyage :** à chaque accès, appliquer clôtures TTL puis expiration des grants, sans collecteur ; retirer opportunément grants expirés/identités disparues, garder 7 jours les diagnostics non sensibles. Après suppression observée de D, invalider son identité ; jamais consommer au Pre puisque l'outil peut être bloqué ailleurs. Courses après Pre = limite A1. Aucun nettoyage physique par le registre ; incertitude limitée au scope/D et levée par constat valide, jamais par simple délai.
**SUPPOSÉ ici / limite transmise :** un worktree Kleos est situé sous `scratch/ingest_m2/wt` ; toucher D=`ingest_m2` rend ce worktree éligible comme descendant. Conséquence du risque accepté, pas une exclusion supplémentaire des worktrees.

## 7. Tests et preuves attendus, non exécutés ici

**SUPPOSÉ / contrat :** tester les commandes comme données, jamais exécuter les suppressions/push de danger. Lancer uniquement les tests ciblés du futur module ; batch gate par le responsable des commits.

| Groupe | Attendus |
|---|---|
| C2 et Git | Deny : `git push origin +main`, `git push origin main -f`, `rm -r -f /etc`, `rm -rf "/etc"`, `rm -rf /tmp /etc`, `git push -qf`, `git clean -fdx`, push `--prune`. Silence : push ordinaire, reset --soft, clean -n. |
| Scanner/wrappers | Deny : `RM.exe -rf /etc`, `r\m -rf /etc`, chaque wrapper portant rm, `rtk git push -f`, `rtk run -c 'rm -rf /etc'`, `echo "$(rm -rf /etc)"`, backquotes. Silence : echo littéral, redirection simple, affectation inerte, heredoc de données, code inline sans jeton rm/git ; tests des exclusions A1. |
| Config/protocole | Minima affaiblis, fichier absent/tronqué, command invalide : deny ; cwd absent ne change pas les denies, session inconnue refuse seulement exception. Fixtures T1 : Batch tableau/ids/réponses Bash chaînes, Failure sans tool_response ; refus ferme sans effet, annulation/inconnu incertains dans seul scope ; replis Stop/Prompt/TTL lèvent conflit futur ; Post tardif/doublon n'accorde rien. |
| Observation V1 | Inventaires plats >10 000 et ≤100 000 autorisés ; D candidat créé vs D Deck/aléatoire non candidat : grant seulement au premier. cwd, token chemin et redirection sélectionnent sans accorder. mtime de D seul ne prouve rien ; delta profond exige parcours candidat complet. Write/Edit profond accorde D, pas R/frère ; reprise après faux négatif Bash ; quotas/reparse/I/O = pas de grant dérivé. |
| Partage/durée | P1 touche, P2 même dépôt ou binding explicite peut supprimer ; autre workspace sans contact refusé. Nouvelle session du même workspace à +23 h oui, exactement +24 h non ; modification à +20 h renouvelle à +44 h ; lecture/heartbeat/Pre rm non. Pause, créateur terminé, redémarrage, recul d'horloge, scopes séparés ; identité commune malgré casse/alias/subst/jonction du dépôt ; chemin Git relatif ; chevauchement incompatible. |
| Cibles/runtime | Préexistant intact, R, `%TEMP%/claude`, glob/relatif, jonctions internes, identité remplacée/registre cassé : aucune exception ; worktree sous D touché éligible. Background explicite/automatique : fermeture au lancement sans grant ; timeout Failure distinct. Zéro réseau/processus fils dans décision, WAL/contention ≤200 ms ; 100 lancements et couverture candidats sur volumes réels. V2 seulement : Codex/pwsh/WSL. |

## 8. Migration et découpage

**DÉDUIT**, `~/.claude/settings.json:189-192,747` : quatre deny (`rm -rf`, `rm -fr`, variantes rtk avec `:*`) et un `autoMode.soft_deny` couvrant suppression récursive/forcée y compris rtk. Une abstention du hook ne les annule pas ; ni les autres protections du harness.

1. **SUPPOSÉ / cible :** **un seul lot fonctionnel V1**, incluant chargeur/scanner, décision Claude Code, observation Pre/Post/Failure Write/Edit/Bash, registre local, sessions/scopes, exception 24 h et tests multi-peers. Ordre interne : T1 headless avant tout code (preuves Bash acquises, compléments §5) -> inventaires plats/candidats -> développement/tests -> validation ensemble ; pas de livraison intermédiaire sans A3, pas de helper de création imposé. Tracer un patch local ; anciens modes inchangés.
2. **SUPPOSÉ / cible :** lot d'activation séparé après validation d'implémentabilité = installer binaire/clone, câbler les hooks session et outils, vérifier corpus/coût/couverture ; arbitrer ensemble quatre deny et clause `soft_deny:747`. Settings intact jusque-là ; aucun retrait automatique d'autres protections. Retour arrière : restaurer d'abord ces protections, puis retirer les hooks ; données du registre non supprimées implicitement.
3. **SUPPOSÉ / cible :** V2 = Codex seulement après sonde de ses payloads/shells réels et tests de refus/timeout ; aucune hypothèse de parité POSIX dans V1.

## 9. Suite, aucune nouvelle question opérateur

**SUPPOSÉ / prochaine étape :** intégrer les preuves T1 au brief développeur, compléter les seuls cas non mesurés et tester les huit corrections en V1 ; aucun nouveau choix de politique. Restriction des créations Bash aux candidats = hypothèse lead révocable, clairement séparée d'A1-A4/B1-B3.
