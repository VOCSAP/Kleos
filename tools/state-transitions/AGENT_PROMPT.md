# Prompt pour un agent Claude Code sur le poste (dépôt Kleos)

À coller tel quel dans une session Claude Code ouverte à la racine du dépôt Kleos, après
avoir récupéré la branche `claude/grimoire-agent-memory-rxqqta`.

---

Tu travailles sur le dépôt Kleos (fork VOCSAP de Ghost-Frame/Kleos). Ta mission porte sur les
**étapes 0 et 1** du plan `tools/state-transitions/PLAN.md` : détecter qu'une mémoire récente
met fin à l'état décrit par une mémoire plus ancienne (exemple : « problème X rencontré le
01/01/2026 » puis « X résolu le 21/01/2026 »), en français, en anglais et entre les deux langues.

## Avant de commencer

1. Lis entièrement `CLAUDE.md`, `CLAUDE.local.md` (hôtes, chemins de la base, procédure
   serveur), `tools/state-transitions/PLAN.md` et `tools/state-transitions/mine_pairs.py`.
2. Vérifie que tu es sur la branche `claude/grimoire-agent-memory-rxqqta`, à jour avec l'origine.
3. Si une information nécessaire manque (chemin de la base, clé SQLCipher, user_id à analyser,
   modèle LLM à utiliser comme juge), demande-la. N'invente aucune valeur.

## Règles

- Les étapes 0 et 1 ne modifient **aucun fichier suivi par upstream**. Tout ce que tu écris va
  sous `tools/state-transitions/` (code) ou `docs/dev-notes/state-transitions/` (données et
  résultats, gitignoré).
- Les données extraites contiennent le contenu des mémoires : **jamais de commit**, jamais
  d'envoi vers un service autre que le LLM déjà configuré dans `KLEOS_LLM_URL`. Supprime la
  copie déchiffrée de la base à la fin et dis-le-moi.
- Toujours travailler sur une **copie** de la base, jamais sur le fichier utilisé par
  `kleos-server`. N'arrête pas et ne redémarre pas le serveur sans me demander.
- Exécute Python avec `python -I`.
- Ne passe pas une porte du plan à ma place : à chaque porte, présente les chiffres et attends
  ma décision.
- Réponds-moi en français ; code, logs et commentaires en anglais.

## Étape 0 -- extraction et mesure

1. Obtiens une copie lisible de la base de l'utilisateur concerné (procédure SQLCipher au
   §5 du plan si elle est chiffrée ; chemins dans `CLAUDE.local.md`). Installe `numpy` si besoin.
2. Lance `python -I tools/state-transitions/mine_pairs.py mine --db <copie> --user-id <id>`.
   Commence avec les paramètres par défaut. Si `candidate_pairs` dépasse environ 20 000 ou
   descend sous 300, propose-moi un autre `--min-sim` avant de relancer.
3. Montre-moi `stats.json` résumé : nombre de mémoires, répartition des langues, paires par
   tranche de similarité, par paire de langues et selon la présence d'un marqueur.
4. **Arrête-toi ici** : c'est moi qui étiquette `to_label.csv`. Tu peux m'aider à lire une
   paire si je te le demande, mais ne pré-remplis pas la colonne `label` : l'objectif est un
   jeu de référence humain, non biaisé par un modèle.
5. Quand je te dis que l'étiquetage est fini, lance `report`, puis présente :
   - le taux de transitions par tranche, par langue et par marqueur ;
   - la précision et le rappel de la règle par marqueur ;
   - les quantiles de cosinus des paires positives ;
   - ce que dit la porte de l'étape 0 et ta recommandation, avec le chiffre qui la justifie.

   Rappelle que l'échantillon est stratifié et donne l'estimation pondérée du volume réel.

## Étape 1 -- juge LLM hors ligne (seulement après mon feu vert)

1. Sépare le jeu étiqueté en 70 % pour la mise au point et 30 % pour l'évaluation finale, par
   tirage avec graine fixe. Ne regarde pas le jeu final avant la dernière évaluation.
2. Rédige le prompt `vocsap/state_transition` (`system.txt` et `user.txt`, variables
   `{{older_date}}`, `{{older}}`, `{{newer_date}}`, `{{newer}}`, réponse JSON stricte décrite
   au plan). Rédige-le **en français**, avec les définitions d'étiquettes du §4 du plan.
   Place-le pour l'instant dans `tools/state-transitions/prompts/vocsap/state_transition/`.
   Il n'ira dans `prompts-overrides/` qu'à l'étape 2.
3. Écris `tools/state-transitions/eval_judge.py` :
   - il lit le CSV étiqueté et appelle `KLEOS_LLM_URL` (`/v1/chat/completions`) avec le même
     modèle que `KLEOS_LLM_MODEL`, sauf si je t'en indique un autre ;
   - il gère le JSON invalide, le délai dépassé et les étiquettes hors liste, en les
     comptant comme des erreurs ;
   - il sort la matrice de confusion, la précision et le rappel sur {`resout`, `remplace`}
     puis sur `contredit`, la ventilation fr-fr / en-en / fr-en, le taux de JSON invalide et
     la latence p50 / p95 ;
   - il met en cache les réponses par (pair_id, hash du prompt) pour ne pas repayer un appel
     identique.
4. Itère au plus 3 fois sur le jeu de mise au point, puis évalue une fois sur le jeu final.
5. Présente les résultats face à la porte de l'étape 1 (précision ≥ 0,85 sur les transitions
   du jeu final, écart entre langues ≤ 0,15) et ta recommandation. Si la porte n'est pas
   atteinte, propose un juge plus fort sans le lancer.

## Fin de mission

- Commit uniquement le code et les prompts sous `tools/state-transitions/` (jamais les
  données), avec un message clair, sur la branche `claude/grimoire-agent-memory-rxqqta`,
  puis push.
- Mets à jour la ligne « Statut » de `PLAN.md` avec les chiffres obtenus (sans contenu de
  mémoire).
- Rappelle-moi de supprimer la copie déchiffrée si ce n'est pas déjà fait.
- Ne commence pas l'étape 2 : elle touche le code Rust et le schéma, et fera l'objet d'une
  autre session.
