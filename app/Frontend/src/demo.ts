// Données de démo, uniquement pour l'aperçu de l'interface dans un navigateur
// (hors de la fenêtre Tauri, il n'y a pas d'accès aux vraies données).
import type { Analyse, EtatJour, LigneMoyenne, Moyennes, Pronote, QuestionJour, ReponseDonnee, ReponseJour, StatsRevision, Suivi, Test } from "./api";

const jour = (decalage: number, heure = "08:00") => {
  const d = new Date();
  d.setDate(d.getDate() + decalage);
  return `${d.toISOString().slice(0, 10)}T${heure}:00`;
};

export function demoPronote(): Pronote {
  const cours = (decalage: number, h: string, fin: string, matiere: string, titre: string | null, controle = false) => ({
    matiere,
    debut: jour(decalage, h),
    fin: jour(decalage, fin),
    prof: "M. Martin",
    salle: "B12",
    annule: false,
    controle,
    statut: null,
    memo: null,
    contenu: titre ? { titre, description: titre, categorie: null, fichiers: [] } : null,
  });
  return {
    synchronise_le: jour(0, "17:02"),
    eleve: { nom: "Élève Démo", classe: "2GT", etablissement: "Lycée démo" },
    periode: { nom: "Trimestre 2", debut: jour(-2), fin: jour(90) },
    controles_a_venir: [
      { matiere: "SC. ECONO.& SOCIALES", date: jour(6).slice(0, 10), source: "mot_cle_devoir", indice: "Évaluation 1H : QCM de 30 questions" },
      { matiere: "HISTOIRE-GEOGRAPHIE", date: jour(9).slice(0, 10), source: "emploi_du_temps", indice: "Éval géo : le développement" },
    ],
    devoirs: [
      { matiere: "MATHEMATIQUES", pour_le: jour(1).slice(0, 10), fait: false, description: "Exercices 42 et 46 page 58", fichiers: [] },
      { matiere: "ITALIEN LV2", pour_le: jour(1).slice(0, 10), fait: true, description: "Fiche sur l'Umanesimo", fichiers: [] },
      { matiere: "FRANCAIS", pour_le: jour(3).slice(0, 10), fait: false, description: "Relire l'acte II de L'École des femmes", fichiers: [] },
    ],
    cours: [
      cours(0, "08:00", "09:00", "MATHEMATIQUES", "Ensemble de définition d'une fonction"),
      cours(0, "09:00", "10:00", "FRANCAIS", "Lecture linéaire : acte I, scène 4"),
      cours(0, "10:15", "11:15", "SCIENCES VIE & TERRE", "La photosynthèse"),
      cours(1, "08:00", "10:00", "HISTOIRE-GEOGRAPHIE", "Méthode croquis"),
      cours(1, "10:15", "11:15", "ANGLAIS LV1", "Conscious fashion"),
    ],
    notes: [
      { periode: "Trimestre 1", matiere: "MATHEMATIQUES", date: jour(-12).slice(0, 10), note: "14", sur: "20", coef: "2", moyenne_classe: "11,2", commentaire: "DS 1h" },
      { periode: "Trimestre 1", matiere: "FRANCAIS", date: jour(-15).slice(0, 10), note: "12", sur: "20", coef: "1", moyenne_classe: "11,8", commentaire: null },
      { periode: "Trimestre 1", matiere: "ANGLAIS LV1", date: jour(-18).slice(0, 10), note: "16,5", sur: "20", coef: "1", moyenne_classe: "13", commentaire: "Test vocabulaire" },
    ],
    moyennes: [
      { periode: "Trimestre 1", matiere: "MATHEMATIQUES", eleve: "13,5", classe: "11,4" },
      { periode: "Trimestre 1", matiere: "FRANCAIS", eleve: "12,1", classe: "12,0" },
      { periode: "Trimestre 1", matiere: "ANGLAIS LV1", eleve: "15,8", classe: "12,9" },
    ],
    informations: [],
  };
}

export function demoSuivi(): Suivi {
  const essai = (ok: boolean, niveau: "savoir" | "comprendre" | "appliquer", erreur?: string) => ({
    date: jour(-1).slice(0, 10),
    niveau,
    ok,
    ...(erreur ? { erreur } : {}),
  });
  return {
    evals: [
      { matiere: "SC. ECONO.& SOCIALES", date: jour(6).slice(0, 10), type: "QCM + doc", titre: "Évaluation 1H", chapitres: ["ch1"], poids: 2 },
    ],
    matieres: [
      {
        matiere: "SC. ECONO.& SOCIALES",
        chapitres: {
          ch1: {
            titre: "Ch.1 – Comment raisonnent économistes, sociologues et politistes ?",
            sources: ["démo"],
            notions: {
              objets: { intitule: "Les objets d'étude des trois disciplines", essais: [essai(true, "savoir"), essai(true, "comprendre")] },
              correlation: { intitule: "Corrélation et causalité", essais: [essai(false, "comprendre", "confusion : corrélation vs causalité")] },
              calculs: { intitule: "Taux de variation", essais: [essai(true, "savoir"), essai(false, "appliquer", "calcul : oubli du ×100")] },
              lecture: { intitule: "Lire un tableau", essais: [] },
            },
          },
        },
      },
    ],
  };
}

export function demoTest(matiere: string): Test {
  return {
    matiere,
    chapitres: [],
    questions: [
      {
        chapitre: "ch1", notion: "correlation", niveau: "comprendre",
        enonce: "Les villes qui ont le plus de pompiers ont aussi le plus d'incendies. Que peut-on en conclure ?",
        support: null,
        choix: ["Les pompiers provoquent des incendies", "Il y a une corrélation, pas forcément une causalité", "Il n'y a aucun lien entre les deux", "Les incendies créent des emplois de pompiers, donc c'est causal"],
        bonne: 1,
        erreurs: ["confusion : corrélation vs causalité", null, "incompréhension : corrélation", "confusion : sens de la causalité"],
        explication: "Les deux augmentent avec la taille de la ville : c'est une corrélation, expliquée par un troisième facteur.",
      },
      {
        chapitre: "ch1", notion: "calculs", niveau: "appliquer",
        enonce: "Le prix passe de 80 € à 100 €. Quel est le taux de variation ?",
        support: "Taux de variation = (valeur d'arrivée − valeur de départ) / valeur de départ × 100",
        choix: ["+20 %", "+25 %", "+80 %", "−20 %"],
        bonne: 1,
        erreurs: ["calcul : divisé par la valeur d'arrivée", null, "méthode : rapport au lieu de variation", "calcul : signe"],
        explication: "(100 − 80) / 80 × 100 = 25 %.",
      },
    ],
  };
}

const ligne = (matiere: string, t1: number, t2: number | null, classe: number): LigneMoyenne => ({
  matiere,
  periodes: [
    { periode: "Trimestre 1", eleve: t1, classe },
    { periode: "Trimestre 2", eleve: t2, classe: t2 === null ? null : classe },
  ],
  actuelle: t2 ?? t1,
  periode_actuelle: t2 === null ? "Trimestre 1" : "Trimestre 2",
  classe,
  evolution: t2 === null ? null : Math.round((t2 - t1) * 100) / 100,
  historique: [],
});

export function demoMoyennes(): Moyennes {
  const matieres = [
    ligne("MATHEMATIQUES", 13.2, 14.1, 11.0),
    ligne("ANGLAIS LV1", 8.5, 9.75, 10.4),
    ligne("ITALIEN LV2", 9.0, 8.4, 10.1),
    ligne("FRANCAIS", 11.5, null, 10.6),
    ligne("SCIENCES VIE & TERRE", 11.8, 12.6, 11.2),
  ];
  return {
    periodes: ["Trimestre 1", "Trimestre 2"],
    periode_courante: "Trimestre 2",
    generale: ligne("__generale__", 10.8, 11.37, 10.66),
    generale_calculee: true,
    matieres,
    au_dessus_classe: 3,
  };
}

export function demoAnalyse(): Analyse {
  return {
    cree_le: new Date(Date.now() - 3_600_000).toISOString().slice(0, 16),
    message: "Tu es au-dessus de la classe presque partout, les maths sont un vrai point fort. La priorité : l'italien et l'anglais, où du vocabulaire régulier peut vite faire remonter ta moyenne.",
    matieres: [
      { matiere: "ITALIEN LV2", priorite: 1, potentiel: 3, objectif: 11, pourquoi: "Loin derrière la classe sur les dernières évals de vocabulaire.", a_reviser: "Le vocabulaire de base et les phrases simples." },
      { matiere: "ANGLAIS LV1", priorite: 1, potentiel: 2.5, objectif: 12, pourquoi: "Les tests de verbes irréguliers font perdre des points.", a_reviser: "Les verbes irréguliers et le vocabulaire du chapitre." },
      { matiere: "MATHEMATIQUES", priorite: 2, potentiel: 1, objectif: 15, pourquoi: "Quelques erreurs de signe évitables.", a_reviser: "La valeur absolue comme distance." },
    ],
  };
}

export function demoStats(): StatsRevision {
  const lundi = (i: number) => {
    const d = new Date();
    d.setDate(d.getDate() - ((d.getDay() + 6) % 7) - 7 * i);
    return d.toISOString().slice(0, 10);
  };
  const s = [0, 4, 0, 12, 8, 16];
  return {
    semaines: s.map((q, i) => ({ debut: lundi(5 - i), questions: q, reussies: Math.round(q * (0.5 + i * 0.07)) })),
    total_questions: 40,
    total_reussies: 27,
    notions_total: 16,
    notions_maitrisees: 5,
  };
}
// --- Séance du jour de démo (aperçu navigateur) : un exemple de chaque format.
const qj = (q: Partial<QuestionJour> & Pick<QuestionJour, "format" | "enonce">): QuestionJour => ({
  phase: "comprendre", matiere: "ITALIEN LV2", lecon: "L'umanesimo", chapitre: "umanesimo", notion: "definition", niveau: "savoir",
  support: null, choix: [], bonne: null, reponses_acceptees: [], valeur: null, tolerance: null, unite: null, plan: [], attendus: [],
  explication: "", ...q,
});

const questionsDemo: QuestionJour[] = [
  qj({ format: "reponse_courte", enonce: "En quel siècle se développe l'Umanesimo en Italie ?", reponses_acceptees: ["XVe siècle", "15e", "XV", "quattrocento"], explication: "L'Umanesimo se développe surtout au XVe siècle (le Quattrocento)." }),
  qj({ format: "qcm", enonce: "Que désigne le mot « Umanesimo » ?", choix: ["Un style d'architecture gothique", "Un mouvement culturel qui remet l'homme au centre", "Une réforme religieuse", "Une guerre entre cités italiennes"], bonne: 1, explication: "C'est un mouvement intellectuel qui place l'homme et la redécouverte des Anciens au centre." }),
  qj({ format: "vrai_faux", niveau: "comprendre", enonce: "L'Umanesimo place l'homme, et non plus seulement Dieu, au centre de la réflexion.", choix: ["Vrai", "Faux"], bonne: 0, explication: "Vrai : c'est la vision anthropocentrique." }),
  qj({ format: "calcul", matiere: "SC. ECONO.& SOCIALES", lecon: "Taux de variation", niveau: "appliquer", enonce: "Un prix passe de 80 € à 100 €. Taux de variation (en %) ?", valeur: 25, tolerance: 0.5, unite: "%", explication: "(100 − 80) / 80 × 100 = 25 %." }),
  qj({ format: "developpement", niveau: "comprendre", enonce: "En t'appuyant sur le document, explique pourquoi l'Umanesimo marque une rupture avec le Moyen Âge.", support: "Dieu dit à Adam : « Je ne t'ai donné ni place précise, ni visage propre, ni fonction particulière, afin que, selon ton envie et ton jugement, tu choisisses toi-même ta place, ton visage et ta fonction. […] Tu pourras dégénérer en formes inférieures, comme les bêtes, ou, par ta propre décision, te régénérer en formes supérieures, qui sont divines. »\n\n(Pic de la Mirandole, De la dignité de l'homme, 1486, traduction simplifiée)", plan: ["Ce que dit le document", "Ce qui change par rapport au Moyen Âge", "Conclusion"], attendus: ["l'homme au centre", "redécouverte des textes antiques", "esprit critique"], explication: "Rupture : l'homme au centre, retour aux Anciens, esprit critique." }),
];
const reponsesDemo: ReponseJour[] = [];

export const demoJour = {
  etat(): EtatJour {
    const poids = (n: string) => (n === "appliquer" ? 2 : n === "comprendre" ? 1.5 : 1);
    const pour = (matiere: string) => {
      const qs = questionsDemo.map((q, i) => ({ q, r: reponsesDemo.find((r) => r.idx === i) })).filter((x) => x.q.phase === "comprendre" && x.q.matiere === matiere);
      const total = qs.reduce((s, x) => s + poids(x.q.niveau), 0);
      const acquis = qs.reduce((s, x) => s + (x.r && x.r.statut !== "en_correction" ? poids(x.q.niveau) * x.r.score : 0), 0);
      return { nb_questions: qs.length, nb_repondues: qs.filter((x) => x.r).length, comprehension: total ? Math.round((acquis / total) * 100) : null };
    };
    const auj = new Date().toISOString().slice(0, 10);
    const lecon = (matiere: string, titre: string, description: string, h: string) => ({ matiere, titre, description, debut: `${auj}T${h}:00`, documents: [] });
    return {
      jour: auj,
      date_lecons: auj,
      matieres: [
        { matiere: "ITALIEN LV2", heure: "08h00", a_contenu: true, en_preparation: false, lecons: [lecon("ITALIEN LV2", "L'umanesimo", "Réactivation du cours. Fiche sur l'Umanesimo : l'homme au centre, redécouverte des Anciens.", "08:00")], ...pour("ITALIEN LV2") },
        { matiere: "SC. ECONO.& SOCIALES", heure: "10h00", a_contenu: true, en_preparation: false, lecons: [lecon("SC. ECONO.& SOCIALES", "Taux de variation", "Calculer et interpréter un taux de variation.", "10:00")], ...pour("SC. ECONO.& SOCIALES") },
        { matiere: "FRANCAIS", heure: "13h00", a_contenu: false, en_preparation: true, lecons: [lecon("FRANCAIS", "Cours", "", "13:00")], nb_questions: 0, nb_repondues: 0, comprehension: null },
      ],
      questions: questionsDemo,
      reponses: [...reponsesDemo],
      suite_en_cours: false,
      erreur: null,
    };
  },  suite: () => false,
  repondre(idx: number, reponse: ReponseDonnee): ReponseJour {
    const q = questionsDemo[idx];
    let statut: ReponseJour["statut"] = "faux";
    if ("choix" in reponse) statut = reponse.choix === q.bonne ? "juste" : "faux";
    if ("texte" in reponse && q.format === "calcul") statut = Math.abs(parseFloat(reponse.texte.replace(",", ".")) - (q.valeur ?? 0)) <= (q.tolerance ?? 0.01) ? "juste" : "faux";
    if ("texte" in reponse && q.format === "reponse_courte") statut = q.reponses_acceptees.some((a) => reponse.texte.toLowerCase().includes(a.toLowerCase())) ? "juste" : "partiel";
    if ("sections" in reponse) statut = "partiel";
    const r: ReponseJour = { idx, statut, score: statut === "juste" ? 1 : statut === "partiel" ? 0.5 : 0, reponse, retour: q.explication };
    reponsesDemo.push(r);
    return r;
  },
};