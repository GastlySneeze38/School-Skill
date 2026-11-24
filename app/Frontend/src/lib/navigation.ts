import { useEffect, useState } from "react";
import {
  BookOpen,
  CalendarClock,
  CalendarDays,
  Home,
  LayoutDashboard,
  Layers,
  ListTodo,
  PenLine,
  Play,
  Settings,
  TrendingUp,
  type LucideIcon,
} from "lucide-react";

export type IdPage =
  | "aujourdhui"
  | "apercu"
  | "seance"
  | "flashcards"
  | "brouillons"
  | "evals"
  | "matieres"
  | "progres"
  | "devoirs"
  | "emploi"
  | "parametres";

export type Page = {
  id: IdPage;
  titre: string;
  description: string;
  icone: LucideIcon;
  couleur: string;
  /** Raccourci affiché et utilisé dans la palette (ex. « G A » = G puis A). */
  raccourci?: string;
};

/** Toute la navigation de l'app, regroupée par usage : réviser, suivre, Pronote. */
export const SECTIONS: { titre: string | null; pages: Page[] }[] = [
  {
    titre: null,
    pages: [
      { id: "aujourdhui", titre: "Aujourd'hui", description: "Ta séance du jour : comprendre la leçon, puis réviser.", icone: Home, couleur: "#f54e00", raccourci: "A" },
      { id: "progres", titre: "Progrès", description: "Tes moyennes, leur évolution, et où un peu de révision peut te faire gagner des points.", icone: TrendingUp, couleur: "#1a9b5b", raccourci: "P" },
    ],
  },
  {
    titre: "Réviser",
    pages: [
      { id: "seance", titre: "Test par matière", description: "Un test de compétences sur la matière de ton choix.", icone: Play, couleur: "#f54e00", raccourci: "S" },
      { id: "flashcards", titre: "Flashcards", description: "Les cartes à revoir aujourd'hui (répétition espacée).", icone: Layers, couleur: "#6b26d9", raccourci: "F" },
      { id: "brouillons", titre: "Brouillons", description: "Tes rédactions et dessins, pour remplacer le papier.", icone: PenLine, couleur: "#0e8fb3", raccourci: "B" },
    ],
  },
  {
    titre: "Suivi",
    pages: [
      { id: "apercu", titre: "Vue d'ensemble", description: "Évals, devoirs, cours du jour et moyennes en un coup d'œil.", icone: LayoutDashboard, couleur: "#4d4f46", raccourci: "V" },
      { id: "evals", titre: "Évaluations", description: "Les évals à venir et ton niveau de préparation pour chacune.", icone: CalendarClock, couleur: "#db2a2a", raccourci: "E" },
      { id: "matieres", titre: "Matières", description: "Tes notions par matière et chapitre : maîtrisé, en cours, à retravailler.", icone: BookOpen, couleur: "#1d4aff", raccourci: "M" },
    ],
  },
  {
    titre: "Pronote",
    pages: [
      { id: "devoirs", titre: "Devoirs", description: "Le travail à faire, récupéré sur Pronote.", icone: ListTodo, couleur: "#e07800", raccourci: "D" },
      { id: "emploi", titre: "Emploi du temps", description: "Tes cours et ce qui y a été fait.", icone: CalendarDays, couleur: "#0e8fb3", raccourci: "T" },
    ],
  },
];

export const PAGE_PARAMETRES: Page = {
  id: "parametres",
  titre: "Paramètres",
  description: "Connexion Pronote, Claude et base de données.",
  icone: Settings,
  couleur: "#4d4f46",
  raccourci: ",",
};

export const TOUTES_LES_PAGES: Page[] = [...SECTIONS.flatMap((s) => s.pages), PAGE_PARAMETRES];

export const pageParId = (id: IdPage) => TOUTES_LES_PAGES.find((p) => p.id === id)!;

const lireHash = (): IdPage => {
  const brut = window.location.hash.replace(/^#\/?/, "");
  // Ancienne page « Notes » : les moyennes sont maintenant dans Progrès.
  const id = (brut === "notes" ? "progres" : brut) as IdPage;
  return TOUTES_LES_PAGES.some((p) => p.id === id) ? id : "aujourdhui";
};

/** Navigation par l'URL (#/evals) : la page reste la même après un rechargement à chaud. */
export function usePage(): [IdPage, (id: IdPage) => void] {
  const [page, setPage] = useState<IdPage>(lireHash);
  useEffect(() => {
    const surChangement = () => setPage(lireHash());
    window.addEventListener("hashchange", surChangement);
    return () => window.removeEventListener("hashchange", surChangement);
  }, []);
  return [page, (id) => (window.location.hash = `/${id}`)];
}
