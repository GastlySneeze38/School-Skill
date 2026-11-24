# Pédagogie : poser de bonnes questions et lire les erreurs

## Pourquoi tester plutôt que relire

Se forcer à retrouver une réponse (auto-test) fait beaucoup plus retenir que relire le cours, et révèle ce qu'on croit savoir sans le savoir. Espacer les révisions dans le temps (le script s'en charge) évite d'oublier avant l'éval. C'est pour ça que tout passe par des questions, même en mode « explication ».

## Les trois niveaux

| Niveau | Ce qu'on vérifie | Formes de questions | Exemples |
|---|---|---|---|
| `savoir` | Il connaît la définition, la date, la formule, le vocabulaire | Réponse courte, QCM avec distracteurs plausibles, compléter | « Qu'est-ce qu'un allèle ? » · « Formule de la dérivée de u×v ? » · « Date de la prise de la Bastille ? » |
| `comprendre` | Il sait expliquer pourquoi / comment, relier deux idées | « Pourquoi… », « Explique avec tes mots », vrai/faux justifié, trouver l'erreur dans un raisonnement | « Pourquoi une mutation peut n'avoir aucun effet ? » · « Vrai ou faux, justifie : si f'(x) > 0 alors f est positive » |
| `appliquer` | Il sait s'en servir dans une situation nouvelle, comme en contrôle | Mini exercice type, analyse de document, calcul, rédiger une phrase de conclusion | « Voici une séquence d'ADN mutée, quelle conséquence sur la protéine ? » · « Dérive f(x) = x²·eˣ » · « À partir de ce doc, montre que… » |

Règles :
- Une question = une notion et un niveau (sinon on ne sait pas quoi enregistrer).
- Formule les questions comme le ferait un prof de lycée français, avec les mots du cours de l'élève quand tu les as.
- En maths, physique-chimie et SVT, « appliquer » veut souvent dire exercice : garde-le faisable en 2 à 4 minutes.
- En histoire-géo, philo et français, « appliquer » peut être : construire un paragraphe argumenté, analyser un court document, relier un exemple à une notion.
- En langues : traduire, conjuguer en contexte, reformuler, répondre à une question de compréhension.
- Les QCM sont rapides mais trop faciles si les mauvaises réponses sont absurdes : utilise des confusions réelles comme distracteurs.

## Écrire un bon QCM à 4 choix

Un QCM trop facile mesure le hasard, pas la compréhension. Pour qu'il serve vraiment :

- **Les mauvaises réponses viennent des vraies erreurs.** Chaque mauvaise réponse correspond à une erreur précise : confondre deux notions, oublier une étape, inverser un rapport, se tromper de signe, prendre la corrélation pour une causalité. Si l'élève la choisit, tu sais *pourquoi* il s'est trompé, et c'est ce qui va dans `--erreur`.
- **Pas de réponse absurde ou hors sujet**, qu'on élimine sans réfléchir : ça transforme la question en devinette à 2 choix.
- **Les 4 options se ressemblent** en longueur et en forme. La bonne réponse ne doit pas être la seule longue, précise ou nuancée.
- **Pas de « toutes les réponses » ni de « aucune »**, ni de double négation dans l'énoncé.
- **Varie la place de la bonne réponse.**
- Pour « comprendre » : 4 explications du même phénomène, dont une seule correcte. Pour « appliquer » : un mini exercice ou un document (dans `preview`) avec 4 résultats possibles, les faux issus d'erreurs de méthode typiques.

Exemples :

> **Maths, appliquer.** Résoudre |x − 3| = 5
> A. x = 8 ou x = −2 ✔ · B. x = 8 *(oubli du 2e cas)* · C. x = 2 ou x = −8 *(erreur de signe)* · D. x = −2 *(un seul cas, mal choisi)*

> **SVT, comprendre.** Pourquoi les euglènes éclairées produisent-elles du dioxygène ?
> A. Elles respirent plus à la lumière *(confusion respiration / photosynthèse)* · B. La photosynthèse utilise l'énergie lumineuse et libère du O₂ ✔ · C. La lumière casse le CO₂ en C et O₂ *(mécanisme inventé)* · D. Elles absorbent le O₂ de l'eau puis le rejettent *(inversion des échanges)*

## Types d'erreurs (à mettre au début de `--erreur`)

- `oubli` : ne se souvient pas (→ à revoir souvent, flashcards).
- `confusion` : mélange deux notions proches (→ comparer côte à côte).
- `vocabulaire` : l'idée est là mais les mots sont faux ou flous (→ fiche de vocabulaire, reformuler).
- `incompréhension` : récite sans comprendre le mécanisme (→ réexpliquer autrement, avec une analogie ou un schéma décrit).
- `méthode` : connaît le cours mais ne sait pas l'utiliser (→ exemple guidé, puis exercice similaire).
- `calcul` : bonne méthode, erreur d'exécution (→ signaler, ne pas sur-réviser la notion).
- `consigne` : a répondu à côté de la question (→ entraîner à lire la consigne).

Écris le détail utile, par exemple `--erreur "confusion : mutation vs allèle"`. Plus le détail est précis, plus la révision ciblée et la liste « tes pièges » seront utiles.

## Corriger

- Commence par ce qui est juste, même partiellement : ça garde la motivation et montre ce qui est acquis.
- Puis ce qui manque, puis la bonne réponse complète, en 1 à 3 phrases.
- Pas de « Bravo !!! 🎉 » à chaque question : un « Oui, c'est ça » suffit. Garde l'enthousiasme pour les vrais progrès (une notion qui passe du rouge au vert).
- Si l'élève se décourage (plusieurs erreurs d'affilée), baisse d'un niveau ou passe à une notion plus solide, puis reviens-y.
