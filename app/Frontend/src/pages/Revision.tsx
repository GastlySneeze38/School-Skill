import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { AnimatePresence, m } from "motion/react";
import { ArrowLeft, BookOpen, Check, ChevronDown, CircleHelp, CornerDownLeft, FileText, Flame, Loader2, Quote, Repeat, RotateCcw, Sparkles, X } from "lucide-react";
import { Matiere } from "../components/partages";
import { Pastille } from "../components/ui";
import {
  etatJour,
  preparerJour,
  preparerMatiere,
  repondreJour,
  suiteJour,
  type EtatJour,
  type MatiereJour,
  type QuestionJour,
  type ReponseDonnee,
  type ReponseJour,
} from "../api";
import { libelleJour, nomMatiere, useDonnees } from "../lib/donnees";
import { EASE_SORTIE } from "../lib/mouvement";
import type { IdPage } from "../lib/navigation";

/** Mode « réviser mes points faibles » (pas une matière du jour). */
const REVISER = "__reviser__";

/**
 * Écran principal : la séance du jour. On choisit par quelle matière du jour commencer, on vérifie
 * qu'on a compris la leçon (le % monte petit à petit), puis on passe à la matière suivante ou on révise.
 */
export function Revision({ aller }: { aller: (id: IdPage) => void }) {
  const { recharger, ia } = useDonnees();
  const [etat, setEtat] = useState<EtatJour | null>(null);
  /** Matière en cours (gardée pour la journée : on reprend où on en était). */
  const [active, setActiveBrut] = useState<string | null>(() => localStorage.getItem(`seance-active-${aujourdhui()}`));
  /** Question dont on montre la correction (en attendant « Continuer »). */
  const [corrigee, setCorrigee] = useState<number | null>(null);
  const [envoi, setEnvoi] = useState(false);

  const setActive = (m: string | null) => {
    setActiveBrut(m);
    setCorrigee(null);
    if (m) localStorage.setItem(`seance-active-${aujourdhui()}`, m);
    else localStorage.removeItem(`seance-active-${aujourdhui()}`);
  };

  const rafraichir = useCallback(async () => {
    try {
      setEtat(await etatJour());
    } catch {
      /* l'app n'est peut-être pas encore prête : on réessaiera au prochain passage */
    }
  }, []);

  // Au montage : préparation de toutes les matières du jour en arrière-plan (sans effet si c'est fait).
  useEffect(() => {
    void preparerJour().then(rafraichir);
  }, [rafraichir]);

  // Les questions de la matière active (ou de révision), dans l'ordre.
  const indices = useMemo(() => {
    if (!etat || !active) return [];
    return etat.questions
      .map((q, i) => ({ q, i }))
      .filter(({ q }) => (active === REVISER ? q.phase !== "comprendre" : q.phase === "comprendre" && q.matiere === active))
      .map(({ i }) => i);
  }, [etat, active]);
  const libres = etat ? indices.filter((i) => !etat.reponses.some((r) => r.idx === i)) : [];
  const enCorrection = !!etat?.reponses.some((r) => r.statut === "en_correction");
  const attente = !!etat && (etat.matieres.some((m) => m.en_preparation) || etat.suite_en_cours || enCorrection);

  // Tant que l'IA travaille (préparation, suite, correction), on relit l'état toutes les 3 s.
  useEffect(() => {
    if (!attente) return;
    const t = setInterval(rafraichir, 3000);
    return () => clearInterval(t);
  }, [attente, rafraichir]);

  // Mode révision : la série suivante se prépare AVANT d'arriver au bout.
  useEffect(() => {
    if (!etat || active !== REVISER || etat.suite_en_cours || libres.length > 3) return;
    void suiteJour().then((lancee) => {
      if (lancee) void rafraichir();
    });
  }, [etat, active, libres.length, rafraichir]);

  const choisir = (matiere: string) => {
    setActive(matiere);
    // Matière pas encore prête : elle passe devant les autres, tout de suite.
    const mj = etat?.matieres.find((x) => x.matiere === matiere);
    if (matiere !== REVISER && mj && mj.nb_questions === 0) void preparerMatiere(matiere).then(rafraichir);
  };

  const repondre = async (idx: number, reponse: ReponseDonnee) => {
    if (envoi) return;
    setEnvoi(true);
    try {
      const r = await repondreJour(idx, reponse);
      setEtat((e) => (e ? { ...e, reponses: [...e.reponses.filter((x) => x.idx !== idx), r] } : e));
      setCorrigee(idx);
      void rafraichir();
    } finally {
      setEnvoi(false);
    }
  };

  const continuer = () => {
    setCorrigee(null);
    void recharger();
  };

  if (!etat) return <Chargement />;

  // Matière enregistrée hier, ou qui n'est plus dans la liste : on repart du choix.
  if (!active || (active !== REVISER && !etat.matieres.some((m) => m.matiere === active))) {
    return <ChoixMatiere etat={etat} choisir={choisir} iaPrete={!!ia?.installe} aller={aller} />;
  }

  const affichee = corrigee ?? libres[0] ?? null;
  const q = affichee !== null ? etat.questions[affichee] : null;
  const reponse = affichee !== null ? etat.reponses.find((r) => r.idx === affichee) : undefined;
  const matiere = etat.matieres.find((m) => m.matiere === active) ?? null;
  // Un développement sur document prend toute la largeur : le document à gauche, l'éditeur à droite.
  const large = !!q && avecDocument(q);

  return (
    <div className={`mx-auto grid gap-4 ${large ? "max-w-6xl" : "max-w-3xl"}`}>
      <Bandeau etat={etat} matiere={matiere} faites={indices.length - libres.length} changer={() => setActive(null)} />
      {q && affichee !== null ? (
        <CarteQuestion
          key={affichee}
          q={q}
          idx={affichee}
          numero={indices.indexOf(affichee) + 1}
          total={active === REVISER ? null : indices.length}
          reponse={reponse}
          repondre={(r) => repondre(affichee, r)}
          continuer={continuer}
          envoi={envoi}
        />
      ) : matiere && matiere.nb_questions > 0 && !matiere.en_preparation ? (
        <MatiereTerminee etat={etat} matiere={matiere} choisir={choisir} />
      ) : (
        <Attente etat={etat} matiere={matiere} iaPrete={!!ia?.installe} aller={aller} reessayer={() => choisir(active)} />
      )}
    </div>
  );
}

const aujourdhui = () => new Date().toISOString().slice(0, 10);

// ---------------------------------------------------------------- 1. choix de la matière

function ChoixMatiere({ etat, choisir, iaPrete, aller }: { etat: EtatJour; choisir: (m: string) => void; iaPrete: boolean; aller: (id: IdPage) => void }) {
  const ordre = etat.matieres;
  // Raccourcis : 1, 2, 3… pour une matière, R pour réviser.
  useEffect(() => {
    const surTouche = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || (e.target as HTMLElement).closest("input, textarea")) return;
      // preventDefault : sinon la touche se tape aussi dans le champ qui reçoit le focus juste après.
      const n = Number(e.key);
      if (n >= 1 && n <= ordre.length) {
        e.preventDefault();
        choisir(ordre[n - 1].matiere);
      } else if (e.key.toLowerCase() === "r") {
        e.preventDefault();
        choisir(REVISER);
      }
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [ordre, choisir]);

  const quand = etat.date_lecons ? libelleJour(etat.date_lecons).toLowerCase() : "aujourd'hui";
  return (
    <div className="mx-auto grid max-w-2xl gap-4">
      <div className="pt-2 text-center">
        <p className="text-[22px] font-bold tracking-tight">Par quelle matière tu commences ?</p>
        <p className="mt-1 text-[13.5px] text-encre-2">
          {quand === "aujourd'hui" ? "Tes cours d'aujourd'hui" : `Tes derniers cours (${quand})`} : on vérifie que tu as compris, puis on révise.
        </p>
      </div>
      {!iaPrete && (
        <div className="carte flex items-center gap-3 border-orange/40 bg-orange/5 p-3.5">
          <CircleHelp size={18} className="shrink-0 text-orange" />
          <p className="flex-1 text-[13px]">Il manque des fichiers de l'IA locale : elle en a besoin pour préparer tes questions.</p>
          <button className="btn" onClick={() => aller("parametres")}>
            Voir ce qui manque
          </button>
        </div>
      )}
      {ordre.length === 0 && (
        <div className="carte p-5 text-center text-[13px] text-encre-2">Pas de cours récent trouvé sur Pronote. Tu peux réviser tes points faibles.</div>
      )}
      <div className="grid gap-2.5">
        {ordre.map((mj, i) => (
          <BoutonMatiere key={mj.matiere} mj={mj} touche={i + 1} choisir={() => choisir(mj.matiere)} />
        ))}
        <button onClick={() => choisir(REVISER)} className="btn h-auto justify-start gap-3 px-4 py-3.5 text-left">
          <Repeat size={18} className="shrink-0 text-bleu" />
          <span className="flex-1">
            <span className="block text-[15px] font-semibold">Réviser mes points faibles</span>
            <span className="block text-[12.5px] font-normal text-encre-3">Notions à revoir, évals proches, matières à faire remonter</span>
          </span>
          <span className="kbd">R</span>
        </button>
      </div>
    </div>
  );
}

function BoutonMatiere({ mj, touche, choisir }: { mj: MatiereJour; touche: number; choisir: () => void }) {
  const finie = mj.nb_questions > 0 && mj.nb_repondues === mj.nb_questions;
  const commencee = mj.nb_repondues > 0 && !finie;
  const titres = mj.lecons.map((l) => l.titre).filter((t) => t !== "Cours");
  return (
    <button onClick={choisir} className="btn group h-auto justify-start gap-4 px-4 py-3.5 text-left">
      <span className="w-12 shrink-0 text-[12.5px] font-medium text-encre-3 tabular-nums">{mj.heure}</span>
      <span className="min-w-0 flex-1">
        <span className="block text-[16px] font-semibold">
          <Matiere nom={mj.matiere} />
        </span>
        <span className="mt-0.5 block truncate text-[12.5px] font-normal text-encre-2">
          {titres.length ? titres.join(" · ") : "Pas de contenu sur Pronote : questions sur tes derniers cours"}
        </span>
      </span>
      {finie ? (
        <Pastille ton="vert">
          <Check size={11} className="mr-0.5" /> {mj.comprehension ?? 0} %
        </Pastille>
      ) : commencee ? (
        <Pastille ton="bleu">
          {mj.nb_repondues}/{mj.nb_questions} · {mj.comprehension ?? 0} %
        </Pastille>
      ) : mj.en_preparation ? (
        <span className="inline-flex items-center gap-1 text-[12px] font-normal text-encre-3">
          <Loader2 size={12} className="animate-spin" /> en préparation
        </span>
      ) : mj.nb_questions > 0 ? (
        <Pastille ton="neutre">prête · {mj.nb_questions} questions</Pastille>
      ) : null}
      <span className="kbd">{touche}</span>
    </button>
  );
}

// ---------------------------------------------------------------- bandeau : matière + compréhension

function Bandeau({ etat, matiere, faites, changer }: { etat: EtatJour; matiere: MatiereJour | null; faites: number; changer: () => void }) {
  const justes = etat.reponses.filter((r) => r.statut === "juste").length;
  const pct = matiere?.comprehension ?? 0;
  return (
    <section className="carte flex flex-wrap items-center gap-x-6 gap-y-3 px-4 py-3">
      <button className="btn btn-fantome h-7 px-1.5 text-encre-2" onClick={changer} title="Changer de matière">
        <ArrowLeft size={15} />
      </button>
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2 text-[12px] font-semibold text-encre-3">
          {matiere ? <BookOpen size={13} /> : <Repeat size={13} />}
          {matiere ? `Comprendre la leçon · ${matiere.heure}` : "Réviser mes points faibles"}
        </div>
        <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[14px]">
          {matiere ? (
            <>
              <Matiere nom={matiere.matiere} />
              <span className="truncate text-encre-2">· {matiere.lecons.map((l) => l.titre).filter((t) => t !== "Cours").join(" · ") || "derniers cours"}</span>
            </>
          ) : (
            <span className="text-encre-2">{faites} question{faites > 1 ? "s" : ""} faite{faites > 1 ? "s" : ""}</span>
          )}
        </div>
      </div>
      {justes > 0 && (
        <span className="inline-flex items-center gap-1 text-[12.5px] text-encre-3">
          <Flame size={13} className="text-accent" /> {justes} bonne{justes > 1 ? "s" : ""} réponse{justes > 1 ? "s" : ""} aujourd'hui
        </span>
      )}

      {/* Jauge de compréhension : elle se remplit petit à petit (scaleX : rien n'est recalculé). */}
      {matiere && (
        <div className="w-full">
          <div className="flex items-baseline justify-between text-[12.5px]">
            <span className="font-medium text-encre-2">Compréhension de la leçon</span>
            <span className="text-[15px] font-bold tabular-nums">{pct} %</span>
          </div>
          <div className="mt-1.5 h-2.5 overflow-hidden rounded-full bg-fond-2">
            <div className="h-full origin-left rounded-full bg-vert transition-transform duration-700 ease-[cubic-bezier(0.23,1,0.32,1)]" style={{ transform: `scaleX(${pct / 100})` }} />
          </div>
        </div>
      )}
    </section>
  );
}
// ---------------------------------------------------------------- la question

const PHASES = { comprendre: "Comprendre", reviser: "Réviser", echauffement: "Échauffement" } as const;

function CarteQuestion({
  q,
  idx,
  numero,
  total,
  reponse,
  repondre,
  continuer,
  envoi,
}: {
  q: QuestionJour;
  idx: number;
  numero: number;
  total: number | null;
  reponse?: ReponseJour;
  repondre: (r: ReponseDonnee) => void;
  continuer: () => void;
  envoi: boolean;
}) {
  // « Citer » dans le document insère le passage dans la partie de l'éditeur en cours.
  const citer = useRef<((texte: string) => void) | null>(null);

  // Après la correction, Entrée = continuer (sauf pendant une saisie).
  useEffect(() => {
    if (!reponse) return;
    // La touche Entrée qui vient de valider la réponse ne doit pas aussi sauter la correction :
    // on ignore Entrée pendant un court instant après l'affichage de la correction.
    const affichage = performance.now();
    const surTouche = (e: KeyboardEvent) => {
      if (performance.now() - affichage < 300) return;
      if (e.key === "Enter" && !(e.target as HTMLElement).closest("textarea")) {
        e.preventDefault();
        continuer();
      }
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [reponse, continuer]);

  return (
    <m.div
      initial={{ opacity: 0, transform: "translateY(6px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)" }}
      transition={{ duration: 0.16, ease: EASE_SORTIE }}
      className="carte p-6"
    >
      <div className="flex flex-wrap items-center gap-2 text-[12.5px] text-encre-3">
        <Pastille ton={q.phase === "comprendre" ? "vert" : q.phase === "reviser" ? "bleu" : "orange"}>{PHASES[q.phase]}</Pastille>
        <span className="font-semibold text-encre-2">
          Question {numero}
          {total ? `/${total}` : ""}
        </span>
        <span>·</span>
        <Matiere nom={q.matiere} />
        {q.lecon && q.phase === "comprendre" && <span className="truncate">· {q.lecon}</span>}
      </div>
      <p className="mt-3 text-[20px] leading-snug font-semibold">{q.enonce}</p>
      {q.support && !avecDocument(q) && <pre className="mt-4 rounded-md border border-bordure bg-fond/60 p-3.5 font-sans text-[13.5px] whitespace-pre-wrap text-encre-2">{q.support}</pre>}

      {avecDocument(q) ? (
        <div className="mt-5 grid gap-5 lg:grid-cols-2 lg:items-start">
          <Document texte={q.support ?? ""} citer={reponse ? undefined : (t) => citer.current?.(t)} />
          <Developpement q={q} cle={idx} reponse={reponse} repondre={repondre} envoi={envoi} citer={citer} />
        </div>
      ) : (
      <div className="mt-5">
        {q.format === "qcm" && <Choix q={q} cle={idx} reponse={reponse} repondre={repondre} />}
        {q.format === "vrai_faux" && <Choix q={q} cle={idx} reponse={reponse} repondre={repondre} vraiFaux />}
        {q.format === "reponse_courte" && <Texte q={q} reponse={reponse} repondre={repondre} envoi={envoi} />}
        {q.format === "calcul" && <Texte q={q} reponse={reponse} repondre={repondre} envoi={envoi} calcul />}
        {q.format === "developpement" && <Developpement q={q} cle={idx} reponse={reponse} repondre={repondre} envoi={envoi} />}
      </div>
      )}

      {!reponse && (
        <div className="mt-4 flex justify-center">
          <button className="btn btn-fantome text-encre-3" onClick={() => repondre({ passe: true })} disabled={envoi}>
            Je sais pas
          </button>
        </div>
      )}

      <AnimatePresence>{reponse && <Correction q={q} r={reponse} continuer={continuer} />}</AnimatePresence>
    </m.div>
  );
}

const LETTRES = ["A", "B", "C", "D"];

/** QCM et vrai/faux : un clic ou une touche (1-4, ou V/F). */
function Choix({ q, cle, reponse, repondre, vraiFaux = false }: { q: QuestionJour; cle: number; reponse?: ReponseJour; repondre: (r: ReponseDonnee) => void; vraiFaux?: boolean }) {
  useEffect(() => {
    if (reponse) return;
    const surTouche = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey || (e.target as HTMLElement).closest("input, textarea")) return;
      const n = Number(e.key);
      const choix = n >= 1 && n <= q.choix.length ? n - 1 : vraiFaux && e.key.toLowerCase() === "v" ? 0 : vraiFaux && e.key.toLowerCase() === "f" ? 1 : null;
      if (choix !== null) {
        e.preventDefault(); // la touche ne doit pas se retrouver tapée dans la question suivante
        repondre({ choix });
      }
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [reponse, repondre, q.choix.length, vraiFaux, cle]);

  const choisi = reponse && "choix" in reponse.reponse ? reponse.reponse.choix : null;
  return (
    <div className={`grid gap-2.5 ${vraiFaux ? "grid-cols-2" : "sm:grid-cols-2"}`}>
      {q.choix.map((c, i) => {
        const bonne = reponse && i === q.bonne;
        const fausse = reponse && i === choisi && i !== q.bonne;
        return (
          <button
            key={i}
            onClick={() => repondre({ choix: i })}
            disabled={!!reponse}
            className={`btn h-auto min-h-14 justify-start gap-3 px-4 py-3 text-left text-[15px] font-normal disabled:opacity-100 ${
              bonne ? "border-vert bg-vert/8" : fausse ? "border-rouge/60 bg-rouge/5" : reponse ? "opacity-60" : ""
            } ${vraiFaux ? "justify-center text-[17px] font-semibold" : ""}`}
          >
            {!vraiFaux && <span className="grid size-7 shrink-0 place-items-center rounded-md bg-fond-2 text-[13px] font-semibold text-encre-2">{LETTRES[i]}</span>}
            <span className={vraiFaux ? "" : "flex-1"}>{c}</span>
            {!reponse && <span className="kbd">{vraiFaux ? (i === 0 ? "V" : "F") : i + 1}</span>}
            {bonne && <Check size={16} className="text-vert" />}
            {fausse && <X size={16} className="text-rouge" />}
          </button>
        );
      })}
    </div>
  );
}

/** Réponse courte et calcul : un champ, Entrée pour valider. */
function Texte({ q, reponse, repondre, envoi, calcul = false }: { q: QuestionJour; reponse?: ReponseJour; repondre: (r: ReponseDonnee) => void; envoi: boolean; calcul?: boolean }) {
  const [valeur, setValeur] = useState(reponse && "texte" in reponse.reponse ? reponse.reponse.texte : "");
  const champ = useRef<HTMLInputElement>(null);
  useEffect(() => champ.current?.focus(), []);
  const envoyer = () => valeur.trim() && repondre({ texte: valeur.trim() });
  return (
    <div className="flex items-center gap-2">
      <div className="relative flex-1">
        <input
          ref={champ}
          value={valeur}
          onChange={(e) => setValeur(e.target.value)}
          onKeyDown={(e) => e.key === "Enter" && !reponse && (e.preventDefault(), envoyer())}
          disabled={!!reponse}
          inputMode={calcul ? "decimal" : "text"}
          placeholder={calcul ? "Ton résultat" : "Ta réponse"}
          className="h-12 w-full rounded-md border border-bordure-forte bg-surface px-3.5 text-[16px] outline-none focus:border-accent disabled:bg-fond/50"
        />
        {calcul && q.unite && <span className="pointer-events-none absolute top-1/2 right-3.5 -translate-y-1/2 text-[14px] text-encre-3">{q.unite}</span>}
      </div>
      {!reponse && (
        <button className="btn btn-primaire h-12 px-4" onClick={envoyer} disabled={!valeur.trim() || envoi}>
          Valider <CornerDownLeft size={14} />
        </button>
      )}
    </div>
  );
}

/**
 * Développement : un éditeur découpé selon le plan proposé, pour remplacer le papier.
 * Le brouillon est gardé en local à chaque frappe : rien n'est perdu si l'app se ferme.
 */
function Developpement({
  q,
  cle,
  reponse,
  repondre,
  envoi,
  citer,
}: {
  q: QuestionJour;
  cle: number;
  reponse?: ReponseJour;
  repondre: (r: ReponseDonnee) => void;
  envoi: boolean;
  citer?: React.RefObject<((texte: string) => void) | null>;
}) {
  const stockage = `brouillon-${new Date().toISOString().slice(0, 10)}-${cle}`;
  const partieActive = useRef(0);
  const champs = useRef<(HTMLTextAreaElement | null)[]>([]);
  const [sections, setSections] = useState<string[]>(() => {
    if (reponse && "sections" in reponse.reponse) return reponse.reponse.sections;
    try {
      return JSON.parse(localStorage.getItem(stockage) ?? "") as string[];
    } catch {
      return q.plan.map(() => "");
    }
  });
  const maj = (i: number, v: string) => {
    const s = sections.map((x, j) => (j === i ? v : x));
    setSections(s);
    localStorage.setItem(stockage, JSON.stringify(s));
  };
  // La citation va à la fin de la dernière partie où l'élève écrivait, puis le curseur y revient.
  useEffect(() => {
    if (!citer) return;
    citer.current = (texte) => {
      const i = partieActive.current;
      const avant = sections[i] ?? "";
      maj(i, `${avant}${avant && !/\s$/.test(avant) ? " " : ""}« ${texte} » `);
      requestAnimationFrame(() => {
        const champ = champs.current[i];
        champ?.focus();
        champ?.setSelectionRange(champ.value.length, champ.value.length);
      });
    };
    return () => {
      citer.current = null;
    };
  });
  const mots = sections.join(" ").split(/\s+/).filter(Boolean).length;
  const envoyer = () => {
    if (!mots) return;
    localStorage.removeItem(stockage);
    repondre({ sections });
  };
  return (
    <div className="grid gap-3">
      {q.plan.map((partie, i) => (
        <label key={i} className="grid gap-1">
          <span className="text-[12.5px] font-semibold text-encre-2">
            {i + 1}. {partie}
          </span>
          <textarea
            ref={(el) => {
              champs.current[i] = el;
            }}
            onFocus={() => (partieActive.current = i)}
            value={sections[i] ?? ""}
            onChange={(e) => maj(i, e.target.value)}
            onKeyDown={(e) => e.key === "Enter" && (e.ctrlKey || e.metaKey) && (e.preventDefault(), envoyer())}
            disabled={!!reponse}
            autoFocus={i === 0}
            rows={3}
            className="field-sizing-content min-h-20 w-full resize-none rounded-md border border-bordure-forte bg-surface px-3.5 py-2.5 text-[15px] leading-relaxed outline-none focus:border-accent disabled:bg-fond/50"
          />
        </label>
      ))}
      {!reponse && (
        <div className="flex items-center justify-between">
          <span className="text-[12px] text-encre-3">
            {mots} mot{mots > 1 ? "s" : ""} · <span className="kbd">Ctrl</span> + <span className="kbd">Entrée</span> pour valider
          </span>
          <button className="btn btn-primaire" onClick={envoyer} disabled={!mots || envoi}>
            Valider ma réponse
          </button>
        </div>
      )}
    </div>
  );
}

/** Un développement se fait sur document, comme en éval : on l'affiche à côté de l'éditeur. */
function avecDocument(q: QuestionJour) {
  return q.format === "developpement" && !!q.support?.trim();
}

/**
 * Le document de la question. Sélectionner un passage puis « Citer » le recopie entre guillemets
 * dans l'éditeur. Sur petit écran il passe au-dessus et se replie.
 */
function Document({ texte, citer }: { texte: string; citer?: (t: string) => void }) {
  const [ouvert, setOuvert] = useState(true);
  const [selection, setSelection] = useState("");
  const zone = useRef<HTMLDivElement>(null);
  // La source est entre parenthèses à la fin du support : on la sépare pour l'afficher en légende.
  const m = texte.trim().match(/^([\s\S]*?)\s*\(([^()]{3,160})\)\s*$/);
  const corps = m ? m[1] : texte.trim();
  const source = m ? m[2] : null;

  const lireSelection = () => {
    const s = window.getSelection();
    const dedans = s && s.rangeCount > 0 && zone.current?.contains(s.getRangeAt(0).commonAncestorContainer);
    setSelection(dedans ? s.toString().trim().replace(/\s+/g, " ") : "");
  };

  return (
    <section className="rounded-lg border border-bordure bg-fond/50 lg:sticky lg:top-4">
      <header className="flex items-center gap-2 border-b border-bordure px-4 py-2.5">
        <FileText size={15} className="text-encre-3" />
        <span className="text-[12.5px] font-semibold text-encre-2">Document</span>
        <span className="flex-1" />
        {citer && selection && (
          <button
            className="btn h-7 gap-1.5 px-2.5 text-[12.5px]"
            onMouseDown={(e) => e.preventDefault()} // garde la sélection
            onClick={() => {
              citer(selection);
              setSelection("");
              window.getSelection()?.removeAllRanges();
            }}
          >
            <Quote size={13} /> Citer
          </button>
        )}
        <button className="btn btn-fantome h-7 px-2 lg:hidden" onClick={() => setOuvert((o) => !o)} aria-expanded={ouvert} aria-label={ouvert ? "Replier le document" : "Afficher le document"}>
          <ChevronDown size={15} className={ouvert ? "rotate-180" : ""} />
        </button>
      </header>
      <div className={`${ouvert ? "" : "hidden"} lg:block`}>
        <div ref={zone} onMouseUp={lireSelection} onKeyUp={lireSelection} className="max-h-[60vh] overflow-auto px-4 py-3.5 font-serif text-[15px] leading-relaxed whitespace-pre-wrap text-encre">
          {corps}
        </div>
        {(source || citer) && (
          <footer className="flex flex-wrap items-center gap-x-3 gap-y-1 border-t border-bordure px-4 py-2 text-[12px] text-encre-3">
            {source && <span className="italic">{source}</span>}
            {citer && <span className="ml-auto">Sélectionne un passage pour le citer</span>}
          </footer>
        )}
      </div>
    </section>
  );
}

// ---------------------------------------------------------------- correction

function Correction({ q, r, continuer }: { q: QuestionJour; r: ReponseJour; continuer: () => void }) {
  const attente = r.statut === "en_correction";
  const styles = {
    juste: { icone: <Check size={16} />, titre: "Juste !", couleur: "text-vert", fond: "bg-vert/8 border-vert/30" },
    partiel: { icone: <CircleHelp size={16} />, titre: "En partie", couleur: "text-orange", fond: "bg-orange/8 border-orange/30" },
    faux: { icone: <X size={16} />, titre: "Pas tout à fait", couleur: "text-rouge", fond: "bg-rouge/5 border-rouge/25" },
    en_correction: { icone: <Loader2 size={16} className="animate-spin" />, titre: "Correction en cours…", couleur: "text-encre-2", fond: "bg-fond/60 border-bordure" },
  }[r.statut];
  const attendue =
    r.statut !== "juste" && !attente
      ? q.format === "qcm" || q.format === "vrai_faux"
        ? null // déjà visible sur les boutons
        : q.format === "calcul"
          ? `Réponse attendue : ${q.valeur}${q.unite ? ` ${q.unite}` : ""}`
          : q.format === "reponse_courte"
            ? `Réponse attendue : ${q.reponses_acceptees[0]}`
            : null
      : null;
  return (
    <m.div
      initial={{ opacity: 0, transform: "translateY(6px)" }}
      animate={{ opacity: 1, transform: "translateY(0px)" }}
      transition={{ duration: 0.18, ease: EASE_SORTIE }}
      className={`mt-5 rounded-lg border p-4 ${styles.fond}`}
    >
      <p className={`flex items-center gap-2 text-[14px] font-semibold ${styles.couleur}`}>
        {styles.icone} {styles.titre}
      </p>
      {attente ? (
        <p className="mt-1 text-[13px] text-encre-2">Tu peux continuer : la correction s'affichera dans ton bilan et compte dans ta compréhension.</p>
      ) : (
        <>
          {attendue && <p className="mt-1.5 text-[13.5px] font-medium">{attendue}</p>}
          <p className="mt-1.5 text-[13.5px] leading-relaxed text-encre-2">{r.retour ?? q.explication}</p>
          {/* La connaissance du cours qui servait : c'est elle qu'il faut retenir. */}
          {q.connaissance && (
            <p className="mt-2.5 flex gap-2 border-t border-black/5 pt-2.5 text-[13px] leading-relaxed">
              <BookOpen size={14} className="mt-0.5 shrink-0 text-encre-3" />
              <span>
                <span className="font-semibold">À retenir : </span>
                {q.connaissance}
              </span>
            </p>
          )}
        </>
      )}
      <div className="mt-3 flex justify-end">
        <button className="btn btn-primaire" onClick={continuer}>
          Continuer <span className="kbd border-accent/30">Entrée</span>
        </button>
      </div>
    </m.div>
  );
}

// ---------------------------------------------------------------- matière terminée, attente, erreur

function MatiereTerminee({ etat, matiere, choisir }: { etat: EtatJour; matiere: MatiereJour; choisir: (m: string) => void }) {
  const restantes = etat.matieres.filter((m) => m.matiere !== matiere.matiere && !(m.nb_questions > 0 && m.nb_repondues === m.nb_questions));
  const pct = matiere.comprehension ?? 0;
  return (
    <m.div initial={{ opacity: 0, transform: "translateY(6px)" }} animate={{ opacity: 1, transform: "translateY(0px)" }} transition={{ duration: 0.2, ease: EASE_SORTIE }} className="carte p-6 text-center">
      <Sparkles size={26} className="mx-auto text-accent" />
      <p className="mt-2 text-[19px] font-bold">
        {nomMatiere(matiere.matiere)} : {pct} % de compréhension
      </p>
      <p className="mt-1 text-[13.5px] text-encre-2">
        {pct >= 80 ? "Tu as bien compris la leçon." : pct >= 50 ? "C'est en bonne voie : les points à revoir reviendront dans tes révisions." : "Pas grave : les notions ratées reviendront dans tes révisions, jusqu'à ce qu'elles soient acquises."}
      </p>
      <p className="mt-5 text-[12.5px] font-semibold text-encre-3">Et maintenant ?</p>
      <div className="mx-auto mt-2 grid max-w-md gap-2">
        {restantes.map((m) => (
          <button key={m.matiere} className="btn h-11 justify-between px-4" onClick={() => choisir(m.matiere)}>
            <Matiere nom={m.matiere} />
            <span className="text-[12px] font-normal text-encre-3">{m.lecons.map((l) => l.titre).filter((t) => t !== "Cours").join(" · ") || "derniers cours"}</span>
          </button>
        ))}
        <button className={`btn h-11 justify-center ${restantes.length ? "" : "btn-primaire"}`} onClick={() => choisir(REVISER)}>
          <Repeat size={14} /> Réviser mes points faibles
        </button>
      </div>
    </m.div>
  );
}

function Attente({ etat, matiere, iaPrete, aller, reessayer }: { etat: EtatJour; matiere: MatiereJour | null; iaPrete: boolean; aller: (id: IdPage) => void; reessayer: () => void }) {
  const enPreparation = matiere ? matiere.en_preparation : etat.suite_en_cours;
  if (etat.erreur && !enPreparation) {
    return (
      <div className="carte p-6 text-center">
        <p className="text-[16px] font-semibold">Les questions n'ont pas pu être préparées</p>
        <p className="mt-1 text-[13px] text-encre-2">{etat.erreur}</p>
        <div className="mt-4 flex justify-center gap-2">
          {!iaPrete && (
            <button className="btn btn-primaire" onClick={() => aller("parametres")}>
              Voir l'IA locale
            </button>
          )}
          <button className="btn" onClick={reessayer}>
            <RotateCcw size={14} /> Réessayer
          </button>
        </div>
      </div>
    );
  }
  return (
    <div className="grid gap-4">
      <div className="carte flex items-center gap-4 p-5">
        <Sparkles size={22} className="shrink-0 text-accent" />
        <div className="flex-1">
          <p className="text-[16px] font-semibold">
            {matiere ? (matiere.nb_questions > 0 ? "La question suivante arrive" : `Préparation de tes questions de ${nomMatiere(matiere.matiere)}`) : "La suite arrive"}
          </p>
          <p className="text-[13px] text-encre-2">
            {matiere ? "Quelques secondes. Pendant ce temps, relis l'essentiel :" : "L'IA écrit des questions de révision sur tes points faibles…"}
          </p>
        </div>
        <Loader2 size={18} className="animate-spin text-encre-3" />
      </div>
      {/* Rappel du cours : on n'attend jamais à ne rien faire. */}
      {matiere?.lecons
        .filter((l) => l.description || l.titre !== "Cours")
        .map((l) => (
          <div key={l.debut + l.titre} className="carte p-5">
            <p className="text-[12.5px] text-encre-3">{libelleJour(l.debut)}</p>
            <p className="mt-1 text-[16px] font-semibold">{l.titre}</p>
            {l.description && <p className="mt-1.5 text-[14px] leading-relaxed whitespace-pre-line text-encre-2">{l.description}</p>}
          </div>
        ))}
    </div>
  );
}

function Chargement() {
  return (
    <div className="mx-auto mt-10 flex max-w-3xl items-center justify-center gap-2 text-[13px] text-encre-3">
      <Loader2 size={16} className="animate-spin" /> Ouverture de ta séance…
    </div>
  );
}