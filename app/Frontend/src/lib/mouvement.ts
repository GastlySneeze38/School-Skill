// Réglages d'animation partagés : toute l'app bouge de la même façon.
//
// Règles (Emil Kowalski) : le survol revient des dizaines de fois par jour, donc il est
// discret et rapide ; on n'anime que transform et opacity (accélérés par la carte
// graphique) ; rien ne bouge sur les actions clavier ; jamais d'ease-in.

/** Courbe de sortie franche : le mouvement démarre tout de suite. */
export const EASE_SORTIE = [0.23, 1, 0.32, 1] as const;

/** Réaction au survol : courte, sans rebond. */
export const SURVOL = { duration: 0.16, ease: EASE_SORTIE } as const;

/** Ressort pour ce qui suit le pointeur (surbrillance de la barre latérale) :
 *  il garde sa vitesse si on change de cible en cours de route. */
export const RESSORT_SUIVI = { type: "spring", duration: 0.28, bounce: 0 } as const;

/** Petit ressort vivant pour les icônes mises en avant (rebond léger). */
export const RESSORT_ICONE = { type: "spring", duration: 0.35, bounce: 0.3 } as const;
