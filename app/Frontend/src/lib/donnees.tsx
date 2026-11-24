import { createContext, useCallback, useContext, useEffect, useState, type ReactNode } from "react";
import {
  analyserMoyennes,
  dansTauri,
  derniereAnalyse,
  etatClaude,
  lireMoyennes,
  lirePronote,
  lireSuivi,
  statsRevision,
  type Analyse,
  type EtatClaude,
  type Moyennes,
  type Notion,
  type Pronote,
  type StatsRevision,
  type Suivi,
} from "../api";

type Donnees = {
  pronote: Pronote | null;
  suivi: Suivi | null;
  chargement: boolean;
  erreur: string | null;
  recharger: () => Promise<void>;
  /** État de la connexion à Claude (Claude Code + forfait), `null` pendant la vérification. */
  claude: EtatClaude | null;
  verifierClaude: () => Promise<EtatClaude | null>;
  moyennes: Moyennes | null;
  stats: StatsRevision | null;
  /** Conseils de Claude sur les matières où gagner des points (la dernière analyse). */
  analyse: Analyse | null;
  analyseEnCours: boolean;
  erreurAnalyse: string | null;
  lancerAnalyse: () => Promise<void>;
};

const Ctx = createContext<Donnees | null>(null);

/** Charge une fois les données Pronote et le suivi, partagés par toutes les pages. */
export function FournisseurDonnees({ children }: { children: ReactNode }) {
  const [pronote, setPronote] = useState<Pronote | null>(null);
  const [suivi, setSuivi] = useState<Suivi | null>(null);
  const [chargement, setChargement] = useState(true);
  const [erreur, setErreur] = useState<string | null>(null);

  const [claude, setClaude] = useState<EtatClaude | null>(null);
  const [moyennes, setMoyennes] = useState<Moyennes | null>(null);
  const [stats, setStats] = useState<StatsRevision | null>(null);
  const [analyse, setAnalyse] = useState<Analyse | null>(null);
  const [analyseEnCours, setAnalyseEnCours] = useState(false);
  const [erreurAnalyse, setErreurAnalyse] = useState<string | null>(null);

  // Dure jusqu'à une minute : l'état est ici pour survivre aux changements de page.
  const lancerAnalyse = useCallback(async () => {
    setAnalyseEnCours(true);
    setErreurAnalyse(null);
    try {
      setAnalyse(await analyserMoyennes());
    } catch (e) {
      setErreurAnalyse(String(e));
    } finally {
      setAnalyseEnCours(false);
    }
  }, []);

  const verifierClaude = useCallback(async () => {
    const e: EtatClaude = dansTauri()
      ? await etatClaude().catch((err) => ({ installe: false, connecte: false, version: null, methode: null, abonnement: null, detail: String(err) }))
      : { installe: true, connecte: true, version: "aperçu", methode: "claude.ai", abonnement: "pro", detail: null };
    setClaude(e);
    return e;
  }, []);

  const recharger = useCallback(async () => {
    setChargement(true);
    setErreur(null);
    try {
      const [p, s, m, st, a] = await Promise.all([lirePronote(), lireSuivi(), lireMoyennes(), statsRevision(), derniereAnalyse()]);
      setPronote(p);
      setSuivi(s);
      setMoyennes(m);
      setStats(st);
      setAnalyse(a);
    } catch (e) {
      setErreur(String(e));
    } finally {
      setChargement(false);
    }
  }, []);

  useEffect(() => {
    void recharger();
    void verifierClaude();
  }, [recharger, verifierClaude]);

  return (
    <Ctx.Provider
      value={{ pronote, suivi, chargement, erreur, recharger, claude, verifierClaude, moyennes, stats, analyse, analyseEnCours, erreurAnalyse, lancerAnalyse }}
    >
      {children}
    </Ctx.Provider>
  );
}

export function useDonnees() {
  const d = useContext(Ctx);
  if (!d) throw new Error("useDonnees hors de FournisseurDonnees");
  return d;
}

// ---------------------------------------------------------------- dates

export const aujourdhui = () => new Date().toISOString().slice(0, 10);

/** Jours entre aujourd'hui et une date AAAA-MM-JJ (négatif si passée). */
export function joursAvant(date: string) {
  const a = new Date(aujourdhui() + "T00:00:00");
  const b = new Date(date.slice(0, 10) + "T00:00:00");
  return Math.round((b.getTime() - a.getTime()) / 86_400_000);
}

export function libelleJour(date: string) {
  const j = joursAvant(date);
  if (j === 0) return "Aujourd'hui";
  if (j === 1) return "Demain";
  if (j === -1) return "Hier";
  return new Date(date.slice(0, 10) + "T00:00:00").toLocaleDateString("fr-FR", { weekday: "long", day: "numeric", month: "long" });
}

export function ilYa(dateIso: string) {
  const secondes = (Date.now() - new Date(dateIso).getTime()) / 1000;
  const rtf = new Intl.RelativeTimeFormat("fr", { numeric: "auto" });
  if (secondes < 60) return "à l'instant";
  if (secondes < 3600) return rtf.format(-Math.round(secondes / 60), "minute");
  if (secondes < 86_400) return rtf.format(-Math.round(secondes / 3600), "hour");
  return rtf.format(-Math.round(secondes / 86_400), "day");
}

export const heure = (dateIso: string) => dateIso.slice(11, 16).replace(":", "h");

/** 12.38 → « 12,38 » ; null → « — ». */
export const fr = (x: number | null | undefined, decimales = 2) =>
  x === null || x === undefined ? "—" : x.toFixed(decimales).replace(".", ",").replace(/,00$/, "");

/** « Trimestre 1 » → « T1 ». */
export const court = (periode: string) => periode.replace(/^Trimestre\s*/i, "T").replace(/^Semestre\s*/i, "S");

// ---------------------------------------------------------------- matières

/** « SC. ECONO.& SOCIALES » → « SES » : les noms Pronote sont longs et en capitales. */
const NOMS: Record<string, string> = {
  "SC. ECONO.& SOCIALES": "SES",
  "SCIENCES VIE & TERRE": "SVT",
  "HISTOIRE-GEOGRAPHIE": "Histoire-géo",
  MATHEMATIQUES: "Maths",
  FRANCAIS: "Français",
  "PHYSIQUE-CHIMIE": "Physique-chimie",
  "ANGLAIS LV1": "Anglais",
  "ITALIEN LV2": "Italien",
  "ESPAGNOL LV2": "Espagnol",
  "ALLEMAND LV2": "Allemand",
  "SC.NUMERIQ.TECHNOL.": "SNT",
  "ACCOMP.CHOIX ORIENT.": "Orientation",
  "ED.PHYSIQUE & SPORT.": "EPS",
  "ENS. MORAL & CIVIQUE": "EMC",
};

export function nomMatiere(nom: string) {
  if (NOMS[nom]) return NOMS[nom];
  const propre = nom.toLowerCase().replace(/\s+/g, " ").trim();
  return propre.charAt(0).toUpperCase() + propre.slice(1);
}

const TEINTES = ["#1d4aff", "#6b26d9", "#1a9b5b", "#e07800", "#db2a2a", "#0e8fb3", "#c2187a", "#7a5a00", "#3a7d00", "#5b5bd6"];

/** Une couleur stable par matière (même matière = même couleur partout). */
export function couleurMatiere(nom: string) {
  let h = 0;
  for (const c of nom) h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return TEINTES[h % TEINTES.length];
}

// ---------------------------------------------------------------- notions (même règle que suivi.py)

export type EtatNotion = "vert" | "orange" | "rouge" | "non_teste" | "a_revoir";

export function etatNotion(n: Notion): EtatNotion {
  const essais = n.essais ?? [];
  if (essais.length === 0) return "non_teste";
  const recents = essais.slice(-4);
  const poids = recents.map((_, i) => i + 1);
  const score = recents.reduce((s, e, i) => s + poids[i] * (e.ok ? 1 : 0), 0) / poids.reduce((a, b) => a + b, 0);
  if (score >= 0.8 && essais[essais.length - 1].ok) {
    if (new Set(essais.filter((e) => e.ok).map((e) => e.niveau)).size < 2) return "orange";
    if (n.prochaine_revision && n.prochaine_revision <= aujourdhui()) return "a_revoir";
    return "vert";
  }
  return score >= 0.5 ? "orange" : "rouge";
}

export const COULEUR_ETAT: Record<EtatNotion, string> = {
  vert: "var(--color-vert)",
  orange: "var(--color-orange)",
  rouge: "var(--color-rouge)",
  a_revoir: "#c9a400",
  non_teste: "var(--color-bordure-forte)",
};

export const LIBELLE_ETAT: Record<EtatNotion, string> = {
  vert: "Maîtrisé",
  orange: "En cours",
  rouge: "À retravailler",
  a_revoir: "À revoir",
  non_teste: "Pas encore testé",
};
