# À faire — app de révisions

## 1. Questions de développement : afficher un document
Aujourd'hui, une question de développement n'est qu'un énoncé suivi de 3 zones de texte (voir la capture du 24/11, Français, *L'École des femmes*). En éval, on développe **à partir d'un document**. Il faut donc :
- [x] Toujours fournir un `support` pour le format `developpement` : extrait du texte étudié, document du prof, carte, tableau de chiffres… (le champ `support` existe déjà dans `QuestionJour`, `app/Backend/src/revision_jour.rs`).
- [x] Afficher ce document **à côté de l'éditeur** (deux colonnes sur grand écran, document repliable au-dessus sur petit écran), avec la possibilité de surligner ou citer un passage dans la réponse (`app/Frontend/src/pages/Revision.tsx`, composant de l'éditeur).
- [ ] Gérer les documents qui sont des images ou des PDF (fichiers de `donnees/pronote/fichiers/`), pas seulement du texte.
- [x] Dans la correction, vérifier que l'élève **utilise** le document (citations, références précises).

## 2. Des questions de réflexion, pas des questions bateau
Le « prompt système » de `generer()` (`revision_jour.rs`, `REGLES_FORMATS`) produit trop de questions de cours (« donne la définition de… ») où il n'y a rien à réfléchir.
- [x] Réécrire les consignes : la majorité des questions doivent **utiliser le cours pour raisonner** (expliquer pourquoi, comparer, appliquer à un cas nouveau, interpréter un document, argumenter). Les questions de récitation ne servent qu'à un échauffement rapide.
- [x] S'appuyer sur **ce qui s'est vraiment passé en cours** : le contenu du cahier de textes, les exercices faits en classe, les documents distribués. Les questions doivent ressembler à celles du prof, pas à un manuel générique.
- [x] Faire en sorte que beaucoup plus de questions partent d'un **document du cours** (pièces jointes Pronote, textes étudiés).
- [x] Mettre à jour `references/pedagogie.md` et `SKILL.md` dans le même sens, pour que le skill et l'app posent le même genre de questions.
- [ ] Test : générer une séance par matière et compter la part de questions de réflexion / de questions appuyées sur un document.

## 3. Plus de sources de données
Le prof ne donne pas tout : il faut aller chercher ailleurs.
- [ ] **Manuels de toutes les matières.** Quand un devoir dit seulement « exo 12 p. 84 », retrouver l'exercice. Il faut savoir quel manuel la classe utilise (liste des manuels du lycée, ou à demander une seule fois), puis le récupérer (version numérique / PDF que l'élève possède) et l'indexer par page.
- [ ] **Livres étudiés en français.** Récupérer le texte intégral des œuvres au programme de la classe (ex. *L'École des femmes* : domaine public, Wikisource), et les textes des lectures analytiques, pour pouvoir citer et donner des extraits en support.
- [ ] **Documents d'histoire-géo** : sources, cartes, chronologies liées au chapitre en cours.
- [ ] **Recherche externe** (web) pour approfondir : la biographie des personnages étudiés en histoire, le contexte d'une œuvre, etc. En éval c'est un plus, et le prof ne le donne pas forcément. Le garder en « pour aller plus loin », séparé de ce qui vient du cours.
- [ ] Stocker tout ça localement (dossier `donnees/sources/<matière>/` + une table en base), avec la provenance de chaque source, pour que Claude puisse le lire (outil `Read`) pendant la génération.

## Encore non vérifié en vrai (rappel)
- [ ] La correction par Claude des réponses écrites (`corriger`).
- [ ] La série « Réviser mes points faibles ».
- [ ] La préparation d'une matière sans contenu sur Pronote (Français), et d'une matière choisie avant d'être prête.
- [ ] Les pages Flashcards et Brouillons (encore « Bientôt »).
