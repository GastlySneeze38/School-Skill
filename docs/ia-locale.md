# Passage à l'IA locale (8 Go de VRAM)

Machine cible : RTX 4060 8 Go, Ryzen 7 5700G, 28 Go de RAM. Pas de toolkit CUDA ni de CMake installés.

## Principe : le code fait le travail, le modèle écrit seulement

Un petit modèle local est mauvais quand on lui donne tout le cours, tous les documents et une consigne de 3 pages
(c'est ce qu'on faisait avec Claude). Il devient bon quand on lui donne **une petite tâche précise avec juste ce
qu'il faut**. Donc :

| Avant (Claude) | Après (local) |
|---|---|
| Claude lit tout et choisit quoi demander | Le code choisit la notion, le passage du cours et le type de question |
| Une grosse requête pour 8 questions | Une requête courte par question (~1 500 jetons en entrée) |
| Claude ouvre les PDF avec l'outil Read | Les documents sont convertis en texte **une fois**, à la synchro |
| Consignes générales | Un gabarit par type de question (« interpréter ce passage », « pourquoi… ») |
| JSON « en espérant » | JSON imposé par grammaire (llama.cpp `json_schema`) : toujours valide |

## Les données, organisées une fois pour toutes

À chaque synchro Pronote, en arrière-plan :
1. **Texte des documents** : PDF → texte (Rust, sans IA). Images et PDF scannés → texte par le modèle vision, une seule fois.
2. **Découpage en passages** de 100 à 300 mots, rangés en base (table `passages` : matière, cours, date, source, texte).
3. **Index plein texte SQLite FTS5** sur les passages : retrouver « le passage qui parle de X » sans IA ni VRAM.
4. **Fiche de leçon** (une par cours, générée une fois et mise en cache) : notions, mots-clés, exemples, personnages,
   dates, textes étudiés. C'est elle qu'on donne au modèle, pas le cours brut.

## Génération d'une question

1. Le code choisit : matière → notion (suivi et répétition espacée) → **type** de question (réflexion par défaut,
   rappel seulement en échauffement) → **passage** du cours (FTS5) qui servira de support.
2. Prompt court : consignes fixes (toujours identiques, donc mises en cache par llama.cpp) + fiche de la leçon + passage + type.
3. Sortie contrainte par schéma JSON. Le code valide et passe à la suivante.

Les questions de la séance se préparent en arrière-plan dès l'ouverture : la première est prête en quelques secondes,
les autres arrivent pendant qu'on répond.

## Architecture multi-modèles : une chaîne pilotée par le code, pas un agent

### Pourquoi pas un « vrai » agent (un modèle qui décide qui appeler)
- Les étapes sont **connues à l'avance** : lire les documents → résumer → choisir quoi demander → écrire → vérifier
  → corriger. Quand les étapes sont prévisibles, une chaîne fixe est plus fiable et moins chère qu'un agent qui
  décide à chaque tour. Les comparatifs de routage le confirment : l'essentiel des gains (50 à 70 %) vient d'un
  partage fixe des tâches et du cache, pas d'un routeur intelligent.
- Les petits modèles se trompent souvent dans les appels d'outils. Chaque tour d'agent ajoute des jetons.
- **Avec 8 Go, un seul gros modèle tient en mémoire.** Si plusieurs modèles se passent la main à chaque question,
  on recharge 5 à 7 Go à chaque fois (2 à 5 s) et on perd le cache. Il faut donc **regrouper le travail par modèle**.
- La bonne idée de l'agentique, on la garde : chaque tâche va au plus petit outil qui sait la faire, et une boucle
  « écrire → vérifier → refaire » (2 essais maximum).

### Qui fait quoi

| Étape | Quand | Qui | Où |
|---|---|---|---|
| Texte des PDF | synchro, une fois | Rust, **aucune IA** | CPU |
| Images et scans → texte | synchro, une fois | petit modèle OCR (GLM-OCR, ~1 Go) | GPU, puis déchargé |
| Découpage + index plein texte | synchro, une fois | Rust + SQLite FTS5, **aucune IA** | CPU |
| Recherche par le sens | synchro et séance | petit modèle d'embeddings (EmbeddingGemma 300M ou bge-m3) | **CPU** (pas de VRAM) |
| Fiche de chaque leçon (notions, exemples, personnages, dates, textes) | synchro, une fois par cours | Qwen 3.5 4B (fort en extraction structurée) | GPU, en lot, puis déchargé |
| Plan de séance : matière, notion, type de question, passage | séance | **code** (répétition espacée, évals, quotas de réflexion) | CPU |
| Rédiger la question | séance, en lot dès l'ouverture | gros modèle : Gemma 4 12B (lettres, langues, HG, SES) ou Qwen 3.5 9B (sciences) | GPU, reste chargé |
| Vérifier la question | séance | **code** d'abord : support présent, citation présente mot pour mot dans le passage, choix distincts. Le gros modèle relit seulement en cas de doute | CPU / GPU |
| Corriger un QCM, vrai/faux, calcul | séance | **code** | CPU |
| Corriger une réponse courte | séance | embeddings (similarité de sens), petit modèle sur CPU si c'est limite | CPU |
| Corriger un développement | séance | gros modèle (déjà chargé) | GPU |

### Le calendrier de la VRAM
1. **Synchro** (arrière-plan) : OCR → déchargé → extracteur 4B, qui fait toutes les fiches d'un coup → déchargé.
2. **Ouverture de la séance** : le gros modèle de la matière choisie est chargé et écrit toutes ses questions.
   Il reste chargé pour corriger les développements.
3. **En permanence sur le CPU** (Ryzen 7, 28 Go de RAM) : les embeddings et le petit juge. Ils n'enlèvent jamais
   le gros modèle de la carte graphique.

`llama-server` a un mode routeur (`--models-dir`, `--models-max 1`, `--models-memory-margin`) qui charge et décharge
les modèles à la demande. Il sert pour le GPU ; une 2e instance « CPU seulement » sert les petits modèles.

### Ce que ça change
- Le gros modèle ne lit plus le cours brut : il reçoit une fiche et un passage (~1 500 jetons au lieu de 6 000 à
  10 000), donc environ 5× moins de lecture par question.
- Le travail lourd (lire, résumer) n'est fait qu'**une fois par cours**, par un modèle 3× plus petit, pendant la synchro.
- Les questions sont meilleures parce que la fiche et le passage sont choisis par le code, pas laissés au hasard.

## Moteur : llama.cpp dans l'app

- `llama-server` (build CUDA officiel, précompilé) lancé par l'app en processus « sidecar » Tauri : pas besoin
  d'installer CUDA ni CMake, API locale sur `127.0.0.1`, prompt cache, sortie par grammaire, déchargement automatique.
- Réglages : quantification Q4_K_M, contexte 8 192, flash attention, cache KV en q8_0, toutes les couches sur le GPU,
  un seul modèle chargé à la fois, déchargé quand l'app est en veille.
- Réflexion du modèle (« thinking ») coupée pour générer, activée seulement pour corriger un développement.

## Choix du modèle (recherche du 26/11/2026)

Sources : benchmarks officiels compilés (gemma4all.com), mesures sur GPU 8 Go (dev.to, RTX 3050), comparatifs de
quantification (Unsloth, llama.cpp, bartowski / modelfit.io), r/LocalLLaMA.

### Forces et faiblesses des candidats qui tiennent en 8 Go

| Benchmark | Gemma 4 12B | Qwen 3.5 9B | Gemma 4 E4B | Qwen 3.5 4B |
|---|---|---|---|---|
| MMMLU (raisonnement multilingue, le plus proche du français) | **83,4** | 81,2 | 76,6 | 76,1 |
| MMLU-Pro (connaissances, raisonnement) | 77,2 | **82,5** | 69,4 | 79,1 |
| GPQA Diamond (sciences) | 78,8 | **81,7** | 58,6 | 76,2 |
| TAU2 (agents, outils) | 69,0 | **79,1** | 42,2 | 79,9 |

- **Gemma 4** : meilleur en multilingue à toutes les tailles, et préféré en aveugle pour la qualité de rédaction
  (Arena : +40 à +50 Elo sur Qwen pour les gros modèles). Google publie des versions **QAT** : entraînées pour être
  compressées en 4 bits, donc presque sans perte. Il existe aussi un petit modèle « brouillon » (MTP) qui accélère la
  génération de 1,35 à 1,6×.
- **Qwen 3.5 9B** : meilleur en connaissances, en sciences et en raisonnement, ainsi qu'en suivi d'outils. Son cache
  de contexte est léger. Il est plus orienté anglais/chinois pour l'écriture.
- **Ministral 3 8B** (Mistral) : bon sur le papier pour le français, mais presque aucune mesure comparable publiée.
  À garder en réserve.
- **Gemma 4 26B-A4B** (MoE, experts en RAM) : tourne sur 8 Go mais obtient le même score que le 12B, en 1,6× plus lent.
  Inutile ici.

### Recommandation

1. **Modèle principal : Gemma 4 12B, QAT Q4_0** (~7 Go avec la vision). Rédaction en français, histoire-géo, SES,
   langues, correction des développements. Mesuré à ~55 jetons/s sur une RTX 3050 8 Go ; la 4060 est plus rapide.
   Une question (~250 jetons) prend environ 4 s.
2. **Pour les maths, la physique-chimie et la SVT : Qwen 3.5 9B, Unsloth UD-Q4_K_XL** (~6 Go). Il est plus fort en
   sciences et en calcul. On change de modèle quand on change de matière (2 à 5 s de chargement depuis le SSD).
3. Si la VRAM est trop juste : la vision (mmproj) n'est chargée que pendant l'ingestion des images, et le cache KV
   passe en q4_0.

### Mesures sur la RTX 4060 (26/11, llama.cpp b12780, CUDA 13.5, flash attention, cache KV q8_0)

| Modèle | Couches sur GPU | Lecture (prompt, jetons/s) | Écriture (jetons/s) | Une question (~250 jetons) |
|---|---|---|---|---|
| Qwen 3.5 9B UD-Q4_K_XL (5,55 Gio) | toutes | 2 360 | **40** | ~7 s |
| Gemma 4 12B QAT Q4_0 (6,48 Gio) | toutes (`-ngl 99`) | 40 (!) | 32 | déborde de la VRAM : inutilisable |
| Gemma 4 12B QAT Q4_0 | 48 | 1 524 | **28** | ~10 s |
| Gemma 4 12B QAT Q4_0 | 44 | 1 219 | 19 | ~13 s |

| Qwen 3.5 4B UD-Q4_K_XL (2,70 Gio) | toutes | 4 013 | **70** | — |
| Qwen 3.5 4B, sur CPU seulement | 0 | — | 8 | trop lent pour l'interactif |

Petits modèles de la chaîne :
- **GLM-OCR Q8_0** (0,9 Go + 0,45 Go de vision) : document d'histoire réel (« transition démographique », 4 phases)
  retranscrit sans faute en **6,6 s, chargement compris**.
- **EmbeddingGemma 300M QAT Q8_0** (0,3 Go), **sur CPU** : 8 textes en 0,27 s ; 4 questions sur 4 retrouvent le bon
  passage (préfixes `task: search result | query:` et `title: none | text:`).

Fichiers : `C:\Users\natpe\.revisions\ia\` (`bin/` llama.cpp b12780, `modeles/`).

Windows occupe déjà ~0,9 Go de VRAM. Avec toutes ses couches sur le GPU, Gemma 12B dépasse donc les 8 Go : le pilote
déborde en mémoire partagée et la lecture du prompt tombe à 40 jetons/s. Avec `-ngl 48`, une couche reste sur le
CPU et tout redevient normal. L'app doit **calculer le nombre de couches selon la VRAM libre** au lancement
(llama.cpp a `--fit` pour ça), plutôt que d'écrire « toutes ».

### Banc d'essai sur les vrais cours (26/11) : `outils/banc_ia/banc.py`

20 cas (17 cahiers de textes + 3 documents des profs : activité SVT, exercice d'italien, document d'histoire OCR)
× 4 types (interpréter un document, pourquoi, appliquer, QCM de réflexion). Résultats dans `donnees/banc/` (local).

| | Qwen 3.5 9B (80 questions) | Gemma 4 12B (32 questions, 8 cas les plus durs) |
|---|---|---|
| JSON valide | 80/80 | 31/32 (une boucle jusqu'à 900 jetons) |
| Questions « bateau » | 0 | 0 |
| Temps moyen par question | **8,4 s** | 12,2 s |
| VRAM max | 7,2 Go | 7,8 Go (à la limite : au moindre débordement, 10 jetons/s) |
| Erreurs de fond, sur les 8 cas durs | ~7/32 | ~6/32 |
| Style | riche, parfois alambiqué et trop long | plus simple et plus clair, niveau seconde |

- **Les erreurs viennent toujours des mêmes endroits, pour les deux modèles** : calculs et raisonnements de maths
  (mauvaise réponse attendue, ou prémisse fausse : « explique pourquoi g est paire » alors qu'elle ne l'est pas),
  règles de grammaire en langue (articles devant les possessifs italiens), et le type « appliquer ».
  Les questions « document » et « pourquoi » sont bonnes et collent au cours.
- Le LaTeX est cassé dans le JSON (`\b`, `\t` deviennent des caractères de contrôle) : il faut l'interdire et
  utiliser l'Unicode (ℝ, ∞, x²).
- La langue des questions varie en anglais/italien, et le modèle mélange tu et vous : à fixer dans les consignes.

**Conclusion** : Gemma 12B ne vaut pas son coût sur cette carte (1,5× plus lent, VRAM au bord, même taux
d'erreurs). On prend **un seul modèle d'écriture, Qwen 3.5 9B** : pas de changement de modèle, une seule mise en
cache. Le problème à régler est la **justesse**, pas le style :
1. consignes corrigées (pas de LaTeX, langue fixe, tutoiement) ;
2. pour les maths, les sciences et la grammaire des langues : activer la réflexion (« thinking »), et faire
   **vérifier chaque question** par une 2e passe qui la résout sans voir la réponse attendue. Si les deux réponses
   diffèrent, la question est jetée ;
3. donner des références sûres (manuels, fiches de grammaire) : voir les sources de données dans `TODO.md`.

### Test v2 : réflexion + vérification (4 cas les plus durs : 2 maths, SVT, italien ; 16 questions)

Réflexion plafonnée à 800 jetons (`--reasoning-budget 800`). Sans plafond, Qwen réfléchit plus de 3 000 jetons (75 s)
et n'écrit jamais le JSON. Vérification : un QCM est résolu à l'aveugle ; les autres questions sont relues de façon
critique ; une question rejetée est réécrite une seule fois.

| | v1 (sans réflexion) | v2 |
|---|---|---|
| Temps par question gardée | 8,4 s | **72 s** (8,5×) |
| Questions avec une erreur de fond | 6/16 (37 %) | 4 des 13 gardées (~30 %) ; 3 jetées |
| Rejetées au 1er essai | — | 7/16 |

Ce qu'on apprend :
- **La résolution à l'aveugle des QCM est fiable** : les 4 QCM gardés sont justes. Les calculs faux sont bien
  attrapés (exemple : le signe de (x²−1)/x).
- **La relecture critique laisse passer les erreurs de concept** : « l'étude des variations donne le tableau de
  signes », « suo remplace l'article », une coquille « nostru ». Elle se trompe même en rejetant
  (« sui » au lieu de « i suoi »).
- La réflexion rend les questions plus prudentes, mais aussi **plus faciles** : « f(x) = −x² + 4 est-elle paire ? ».
- Hors programme : une dérivée est apparue, alors qu'elle se voit en première.
- Sans grammaire JSON imposée, 5 réponses sur 16 avaient des clés inattendues (énoncé vide), et le LaTeX revient
  malgré la consigne.

**Conclusion** : un modèle de 9B, même en réfléchissant, ne garantit pas la justesse en maths et en grammaire.
Faire réfléchir plus fort coûte 8× le temps pour un gain modeste. Ce qui marche, c'est de **vérifier par une méthode
fiable plutôt que par le jugement du modèle** :
1. **Maths et sciences** : des formats vérifiables. QCM résolu à l'aveugle, et calcul dont la réponse est recalculée
   par le code (évaluation d'expression en Rust). Pas de réponse libre en maths.
2. **Langues et grammaire** : des fiches de règles sûres, écrites une fois (par Claude ou à partir du manuel), données
   au modèle comme référence. Il applique la règle au lieu de s'en souvenir.
3. **Niveau** : la liste des notions du programme de seconde, par matière, dans les consignes. Interdire ce qui est
   hors programme (dérivées, etc.).
4. Garder la **réflexion seulement pour la vérification à l'aveugle des QCM et des calculs**, où elle sert vraiment.
   La génération reste rapide (8 s), sans réflexion, avec grammaire JSON.
5. Lettres, histoire-géo, SES, documents : la v1 suffit déjà (questions justes et ancrées dans le cours).

### Niveau de compression

- **4 bits, c'est le bon point.** Sous 4 bits la qualité s'effondre : de Q4_K_M à Q2_K, la perplexité augmente
  de plus de moitié. Q5 et Q6 ne tiennent pas avec de la marge, et sont environ 25 % plus lents.
- **Gemma : QAT Q4_0**, celui de Google, pas un Q4 refait par la communauté : le modèle a appris à être compressé.
- **Qwen : UD-Q4_K_XL** (Unsloth, précision variable selon les couches). Environ 29 % de dérive en moins que Q4_K_M,
  pour environ 0,7 Go de plus. À défaut, Q4_K_M. **Éviter IQ4_XS** : 35 % de dérive en plus pour 2 Go gagnés.
  Ne jamais compresser les couches d'attention : l'architecture hybride de Qwen 3.5 y est très sensible.
- **Cache KV en q8_0**, avec flash attention. q4_0 seulement si nécessaire.

### À vérifier sur nos cours avant de figer le choix
Ces benchmarks sont surtout en anglais. Banc d'essai : 20 cours réels × 4 types de questions, à noter sur le français,
la réflexion, la fidélité au document, la validité du JSON, la vitesse et la VRAM.

## Étapes

1. Banc d'essai des modèles (script à part, ne touche pas à l'app).
2. Ingestion : texte des documents, passages, FTS5, fiches de leçon.
3. Sidecar llama-server + module `ia_locale.rs` (remplace `claude.rs`).
4. Génération question par question avec gabarits, puis correction.
5. Retrait de Claude Code de l'app et du skill.
