// Les commandes du Backend (Backend/src/lib.rs), typées.
import { invoke } from "@tauri-apps/api/core";
import { demoAnalyse, demoJour, demoMoyennes, demoPronote, demoStats, demoSuivi, demoTest } from "./demo";

export type EtatClaude = {
  installe: boolean;
  connecte: boolean;
  version: string | null;
  methode: string | null;
  abonnement: string | null;
  detail: string | null;
};

export type EtatConfig = {
  base: { chemin: string; version_schema: number; tables: string[] };
  pronote: {
    config_trouvee: boolean;
    url: string | null;
    identifiant: string | null;
    mode_connexion: string | null;
    mot_de_passe_au_coffre: boolean;
    derniere_synchro: string | null;
    nb_cours: number | null;
    nb_evals_a_venir: number | null;
  };
  claude: EtatClaude;
};

export type ResultatSynchro = {
  ok: boolean;
  /** Pas de connexion : la dernière synchro est trop récente (Pronote bloque sinon). */
  ignoree: boolean;
  message: string;
  alertes: string[];
};
export type ReponseClaude = { ok: boolean; texte: string };

// --- Séances (Backend/src/seance.rs) : QCM rédigés par Claude avec les instructions du skill
export type Question = {
  chapitre: string;
  notion: string;
  niveau: Niveau;
  enonce: string;
  support: string | null;
  choix: string[];
  bonne: number;
  erreurs: (string | null)[];
  explication: string;
};
export type Test = { matiere: string; questions: Question[]; chapitres: string[] };
export type ContenuMatiere = { matiere: string; cours: number; docs: number; caracteres: number; dernier_cours: string; suivie: boolean };

// --- Séance du jour, l'écran principal (Backend/src/revision_jour.rs)
export type Format = "qcm" | "vrai_faux" | "reponse_courte" | "calcul" | "developpement";
export type QuestionJour = {
  phase: "comprendre" | "reviser" | "echauffement";
  matiere: string;
  lecon: string | null;
  chapitre: string;
  notion: string;
  niveau: Niveau;
  format: Format;
  enonce: string;
  support: string | null;
  choix: string[];
  bonne: number | null;
  reponses_acceptees: string[];
  valeur: number | null;
  tolerance: number | null;
  unite: string | null;
  plan: string[];
  attendus: string[];
  explication: string;
};
/** Ce que l'élève répond, selon le format. */
export type ReponseDonnee = { choix: number } | { texte: string } | { sections: string[] } | { passe: true };
export type ReponseJour = {
  idx: number;
  statut: "juste" | "partiel" | "faux" | "en_correction";
  score: number;
  reponse: ReponseDonnee;
  retour: string | null;
};
export type Lecon = { matiere: string; titre: string; description: string; debut: string; documents: string[] };
/** Une matière du jour et où on en est dessus. */
export type MatiereJour = {
  matiere: string;
  heure: string;
  lecons: Lecon[];
  /** Le prof a écrit un contenu (sinon Claude part des derniers cours de la matière). */
  a_contenu: boolean;
  nb_questions: number;
  nb_repondues: number;
  comprehension: number | null;
  en_preparation: boolean;
};
export type EtatJour = {
  jour: string;
  date_lecons: string | null;
  matieres: MatiereJour[];
  questions: QuestionJour[];
  reponses: ReponseJour[];
  suite_en_cours: boolean;
  erreur: string | null;
};

// --- Moyennes et progression (Backend/src/moyennes.rs)
export type ValeurPeriode = { periode: string; eleve: number | null; classe: number | null };
export type LigneMoyenne = {
  matiere: string;
  periodes: ValeurPeriode[];
  actuelle: number | null;
  periode_actuelle: string | null;
  classe: number | null;
  /** Différence avec le trimestre précédent (positive = progrès). */
  evolution: number | null;
  historique: { jour: string; eleve: number }[];
};
export type Moyennes = {
  periodes: string[];
  periode_courante: string | null;
  generale: LigneMoyenne;
  generale_calculee: boolean;
  matieres: LigneMoyenne[];
  au_dessus_classe: number;
};
export type ConseilMatiere = { matiere: string; priorite: 1 | 2 | 3; potentiel: number; objectif: number | null; pourquoi: string; a_reviser: string };
export type Analyse = { cree_le: string; message: string; matieres: ConseilMatiere[] };
export type StatsRevision = {
  semaines: { debut: string; questions: number; reussies: number }[];
  total_questions: number;
  total_reussies: number;
  notions_total: number;
  notions_maitrisees: number;
};

// --- Données Pronote (donnees/pronote/sync.json, écrit par la synchro Rust)
export type Fichier = { nom: string; chemin?: string; lien?: string };
export type Cours = {
  matiere: string;
  debut: string;
  fin: string;
  prof: string | null;
  salle: string | null;
  annule: boolean;
  controle: boolean;
  statut: string | null;
  memo: string | null;
  contenu: { titre: string | null; description: string | null; categorie: string | null; fichiers: Fichier[] } | null;
};
export type Devoir = { matiere: string; pour_le: string; fait: boolean; description: string; fichiers: Fichier[] };
export type ControlePronote = { matiere: string; date: string; source: string; indice: string };
export type Note = {
  periode: string;
  matiere: string;
  date: string;
  note: string;
  sur: string;
  coef: string;
  moyenne_classe: string | null;
  commentaire: string | null;
};
export type Moyenne = { periode: string; matiere: string; eleve: string; classe: string };
export type Pronote = {
  synchronise_le: string;
  eleve: { nom: string | null; classe: string | null; etablissement: string | null };
  periode: { nom: string; debut: string; fin: string } | null;
  controles_a_venir: ControlePronote[];
  devoirs: Devoir[];
  cours: Cours[];
  notes: Note[];
  moyennes: Moyenne[];
  informations: { titre: string; auteur: string; date: string; lu: boolean; contenu: string }[];
};

// --- Suivi des révisions (donnees/evals.json + matieres/*.json, tenus par le skill)
export type Niveau = "savoir" | "comprendre" | "appliquer";
export type Essai = { date: string; niveau: Niveau; ok: boolean; erreur?: string };
export type Notion = { intitule: string; essais: Essai[]; palier?: number; prochaine_revision?: string };
export type Chapitre = { titre: string; sources: string[]; notions: Record<string, Notion> };
export type MatiereSuivie = { matiere: string; chapitres: Record<string, Chapitre> };
export type EvalSuivie = { matiere: string; date: string; type: string; titre: string; chapitres: string[]; poids: number };
export type Suivi = { evals: EvalSuivie[]; matieres: MatiereSuivie[] };

/** Vrai dans la fenêtre Tauri, faux si la page est ouverte dans un simple navigateur. */
export const dansTauri = () => "__TAURI_INTERNALS__" in window;

/** Hors de Tauri (aperçu dans un navigateur) on affiche des données de démo. */
const horsApp = <T,>(demo: T) => Promise.resolve(demo);

export const etatConfig = () => invoke<EtatConfig>("etat_config");
export const configurerPronote = (p: { url: string; identifiant: string; ent: string; motDePasse: string | null }) =>
  invoke<void>("configurer_pronote", { url: p.url, identifiant: p.identifiant, ent: p.ent, motDePasse: p.motDePasse });
export const synchroniserPronote = () => invoke<ResultatSynchro>("synchroniser_pronote");
export const etatClaude = () => invoke<EtatClaude>("etat_claude");
export const connecterClaude = () => invoke<void>("connecter_claude");
export const testerClaude = () => invoke<ReponseClaude>("tester_claude");
export const preparerTest = (matiere: string, nbQuestions = 8) =>
  dansTauri() ? invoke<Test>("preparer_test", { matiere, nbQuestions }) : new Promise<Test>((ok) => setTimeout(() => ok(demoTest(matiere)), 1200));
export const enregistrerReponse = (matiere: string, question: Question, choix: number | null) =>
  dansTauri() ? invoke<boolean>("enregistrer_reponse", { matiere, question, choix }) : horsApp(choix === question.bonne);
export const contenuMatieres = () => invoke<ContenuMatiere[]>("contenu_matieres");
export const lireMoyennes = () => (dansTauri() ? invoke<Moyennes | null>("lire_moyennes") : horsApp(demoMoyennes()));
export const derniereAnalyse = () => (dansTauri() ? invoke<Analyse | null>("derniere_analyse") : horsApp(demoAnalyse()));
export const analyserMoyennes = () =>
  dansTauri() ? invoke<Analyse>("analyser_moyennes") : new Promise<Analyse>((ok) => setTimeout(() => ok(demoAnalyse()), 1500));
export const etatJour = () => (dansTauri() ? invoke<EtatJour>("etat_jour") : horsApp(demoJour.etat()));
export const preparerJour = () => (dansTauri() ? invoke<boolean>("preparer_jour") : horsApp(false));
export const preparerMatiere = (matiere: string) => (dansTauri() ? invoke<boolean>("preparer_matiere", { matiere }) : horsApp(false));
export const suiteJour = () => (dansTauri() ? invoke<boolean>("suite_jour") : horsApp(demoJour.suite()));
export const repondreJour = (idx: number, reponse: ReponseDonnee) =>
  dansTauri() ? invoke<ReponseJour>("repondre_jour", { idx, reponse }) : horsApp(demoJour.repondre(idx, reponse));
export const statsRevision = () => (dansTauri() ? invoke<StatsRevision>("stats_revision") : horsApp(demoStats()));
export const lirePronote = () => (dansTauri() ? invoke<Pronote | null>("lire_pronote") : horsApp(demoPronote()));
export const lireSuivi = () => (dansTauri() ? invoke<Suivi>("lire_suivi") : horsApp(demoSuivi()));
