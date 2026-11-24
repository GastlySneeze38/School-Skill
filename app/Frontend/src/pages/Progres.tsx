import { useState, type ReactNode } from "react";
import { m } from "motion/react";
import { ArrowDownRight, ArrowRight, ArrowUpRight, Minus, Play, RefreshCw, Sparkles, Target, Trophy } from "lucide-react";
import { Carte, Pastille, Vide } from "../components/ui";
import { Matiere } from "../components/partages";
import { court, fr, ilYa, nomMatiere, useDonnees } from "../lib/donnees";
import { RESSORT_ICONE } from "../lib/mouvement";
import type { IdPage } from "../lib/navigation";
import type { ConseilMatiere, LigneMoyenne } from "../api";

/** Ouvre la page Séance directement sur une matière (lu par Seance au montage). */
export function reviserMatiere(matiere: string, aller: (id: IdPage) => void) {
  sessionStorage.setItem("seance-matiere", matiere);
  aller("seance");
}

export function Progres({ aller }: { aller: (id: IdPage) => void }) {
  const { moyennes, analyse } = useDonnees();
  if (!moyennes) {
    return (
      <div className="carte">
        <Vide icone={<Trophy size={24} />} titre="Pas encore de moyennes" texte="Elles arrivent avec la synchro Pronote." />
      </div>
    );
  }
  const conseils = new Map((analyse?.matieres ?? []).map((c) => [c.matiere, c]));

  return (
    <div className="grid gap-4">
      <MoyenneGenerale />
      <div className="grid grid-cols-1 gap-4 xl:grid-cols-[1.15fr_1fr]">
        <OuGagnerDesPoints aller={aller} />
        <Revisions />
      </div>
      <Carte
        titre="Tes moyennes par matière"
        aide="La barre bleue, c'est toi ; le trait noir, la classe ; le rond, l'objectif conseillé par Claude."
        sansMarge
        actions={<Legende />}
      >
        {[...moyennes.matieres]
          .sort((a, b) => (conseils.get(a.matiere)?.priorite ?? 9) - (conseils.get(b.matiere)?.priorite ?? 9) || (b.actuelle ?? 0) - (a.actuelle ?? 0))
          .map((l) => (
            <LigneMatiere key={l.matiere} l={l} conseil={conseils.get(l.matiere)} aller={aller} />
          ))}
      </Carte>
    </div>
  );
}

// ---------------------------------------------------------------- moyenne générale (chiffre phare)

function MoyenneGenerale() {
  const { moyennes } = useDonnees();
  const m_ = moyennes!;
  const g = m_.generale;
  const pasEncore = m_.periode_courante && g.periode_actuelle !== m_.periode_courante;
  return (
    <section className="carte flex flex-wrap items-center gap-x-10 gap-y-4 p-5">
      <div>
        <p className="text-[12px] font-semibold tracking-wide text-encre-3 uppercase">
          Moyenne générale · {g.periode_actuelle ?? "—"}
          {m_.generale_calculee && <span className="font-normal normal-case"> (calculée)</span>}
        </p>
        <div className="mt-1 flex items-baseline gap-3">
          <span className="text-[44px] leading-none font-bold tracking-tight tabular-nums">{fr(g.actuelle)}</span>
          <Evolution valeur={g.evolution} depuis={avant(g)} />
        </div>
        <p className="mt-1.5 text-[13px] text-encre-2">Classe : {fr(g.classe)}</p>
      </div>

      <div className="flex items-center gap-3 rounded-lg bg-vert/8 px-4 py-3">
        <Trophy size={22} className="shrink-0 text-vert" />
        <p className="text-[13.5px]">
          Au-dessus de la classe dans <strong>{m_.au_dessus_classe}</strong> matière{m_.au_dessus_classe > 1 ? "s" : ""} sur {m_.matieres.length}
        </p>
      </div>

      {/* Les trimestres de l'année : on voit où on en est et ce qui reste à jouer. */}
      <div className="ml-auto flex gap-2">
        {g.periodes.map((p) => (
          <div key={p.periode} className={`min-w-20 rounded-md border px-3 py-2 text-center ${p.periode === m_.periode_courante ? "border-accent/50 bg-accent/5" : "border-bordure"}`}>
            <p className="text-[11.5px] font-semibold text-encre-3">{court(p.periode)}</p>
            <p className="text-[17px] font-semibold tabular-nums">{fr(p.eleve)}</p>
          </div>
        ))}
      </div>
      {pasEncore && (
        <p className="w-full text-[12.5px] text-encre-3">
          Pas encore de note ce {m_.periode_courante?.toLowerCase()} : tout reste à jouer, et les révisions d'aujourd'hui comptent déjà.
        </p>
      )}
    </section>
  );
}

const avant = (l: LigneMoyenne) => {
  const notees = l.periodes.filter((p) => p.eleve !== null);
  return notees.length >= 2 ? court(notees[notees.length - 2].periode) : null;
};

/** Hausse en vert avec flèche ; baisse en orange (pas en rouge : pas de quoi s'alarmer). */
function Evolution({ valeur, depuis }: { valeur: number | null; depuis: string | null }) {
  if (valeur === null) return null;
  const Icone = valeur > 0.05 ? ArrowUpRight : valeur < -0.05 ? ArrowDownRight : Minus;
  const ton = valeur > 0.05 ? "text-vert bg-vert/10" : valeur < -0.05 ? "text-orange bg-orange/10" : "text-encre-2 bg-fond-2";
  return (
    <span className={`inline-flex items-center gap-0.5 rounded px-1.5 py-0.5 text-[12.5px] font-semibold tabular-nums ${ton}`}>
      <Icone size={13} strokeWidth={2.5} />
      {valeur > 0 ? "+" : ""}
      {fr(valeur)}
      {depuis && <span className="ml-1 font-normal opacity-80">depuis {depuis}</span>}
    </span>
  );
}

// ---------------------------------------------------------------- conseils de Claude

function OuGagnerDesPoints({ aller }: { aller: (id: IdPage) => void }) {
  const { analyse, analyseEnCours, erreurAnalyse, lancerAnalyse, claude } = useDonnees();
  const prioritaires = (analyse?.matieres ?? []).filter((c) => c.priorite <= 2).slice(0, 4);
  return (
    <Carte
      titre="Où gagner des points"
      aide="Claude regarde tes moyennes, tes notes récentes et tes révisions, et estime où un effort rapporte le plus."
      sansMarge
      className="h-full"
      actions={
        <m.button
          whileHover={analyseEnCours ? undefined : "survol"}
          className="btn h-7"
          onClick={() => void lancerAnalyse()}
          disabled={analyseEnCours || !claude?.connecte}
          title={claude?.connecte ? "Demander une nouvelle analyse à Claude" : "Connecte Claude dans Paramètres"}
        >
          <m.span variants={{ survol: { rotate: 90 } }} transition={RESSORT_ICONE} className="inline-flex">
            <RefreshCw size={13} className={analyseEnCours ? "animate-[spin_0.6s_linear_infinite]" : ""} />
          </m.span>
          {analyseEnCours ? "Analyse…" : analyse ? "Mettre à jour" : "Analyser"}
        </m.button>
      }
    >
      {erreurAnalyse && <p className="border-b border-bordure/70 px-3.5 py-2.5 text-[12.5px] text-rouge">{erreurAnalyse}</p>}
      {!analyse ? (
        <Vide
          icone={<Sparkles size={22} />}
          titre={analyseEnCours ? "Claude analyse tes moyennes…" : "Pas encore d'analyse"}
          texte={analyseEnCours ? "Environ une minute." : "Claude te dit dans quelles matières un peu de révision peut faire monter ta moyenne."}
        />
      ) : (
        <>
          <p className="flex gap-2.5 border-b border-bordure/70 px-3.5 py-3 text-[13.5px] leading-relaxed">
            <Sparkles size={16} className="mt-0.5 shrink-0 text-accent" />
            {analyse.message}
          </p>
          {prioritaires.map((c) => (
            <Conseil key={c.matiere} c={c} aller={aller} />
          ))}
          <p className="px-3.5 py-2 text-[11.5px] text-encre-3">Analyse {ilYa(analyse.cree_le)}</p>
        </>
      )}
    </Carte>
  );
}

function Conseil({ c, aller }: { c: ConseilMatiere; aller: (id: IdPage) => void }) {
  return (
    <div className="group ligne-survol flex items-start gap-3 border-b border-bordure/70 px-3.5 py-3">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          <Matiere nom={c.matiere} />
          <Pastille ton={c.priorite === 1 ? "orange" : "neutre"}>+{fr(c.potentiel, 1)} pt possible</Pastille>
          {c.objectif !== null && (
            <span className="inline-flex items-center gap-1 text-[12px] text-encre-3">
              <Target size={12} /> objectif {fr(c.objectif, 1)}
            </span>
          )}
        </div>
        <p className="mt-1 text-[12.5px] text-encre-2">{c.pourquoi}</p>
        <p className="mt-0.5 text-[12.5px]">
          <span className="font-medium">À réviser :</span> {c.a_reviser}
        </p>
      </div>
      <button className="btn h-7 shrink-0" onClick={() => reviserMatiere(c.matiere, aller)}>
        <Play size={12} /> Réviser
      </button>
    </div>
  );
}

// ---------------------------------------------------------------- révisions de la semaine

function Revisions() {
  const { stats } = useDonnees();
  const [survol, setSurvol] = useState<number | null>(null);
  if (!stats) return null;
  const max = Math.max(1, ...stats.semaines.map((s) => s.questions));
  const taux = stats.total_questions ? Math.round((stats.total_reussies / stats.total_questions) * 100) : null;
  return (
    <Carte titre="Tes révisions" aide="Chaque question répondue (dans l'app ou dans le chat) compte." className="h-full">
      <div className="grid grid-cols-3 gap-3">
        <Chiffre valeur={String(stats.total_questions)} libelle="questions répondues" />
        <Chiffre valeur={taux === null ? "—" : `${taux} %`} libelle="de bonnes réponses" />
        <Chiffre valeur={`${stats.notions_maitrisees}/${stats.notions_total}`} libelle="notions maîtrisées" />
      </div>

      {/* Questions par semaine : une barre par semaine, détail au survol. */}
      <div className="mt-5">
        <p className="mb-2 text-[12px] text-encre-3">Questions par semaine</p>
        <div className="relative flex h-28 items-end gap-2 border-b border-bordure" onMouseLeave={() => setSurvol(null)}>
          {stats.semaines.map((s, i) => {
            const courante = i === stats.semaines.length - 1;
            return (
              <div key={s.debut} className="relative flex h-full flex-1 items-end" onMouseEnter={() => setSurvol(i)}>
                <div
                  className={`w-full rounded-t-[4px] transition-opacity duration-150 ${courante ? "bg-bleu" : "bg-bleu/55"} ${survol !== null && survol !== i ? "opacity-40" : ""}`}
                  style={{ height: s.questions ? `${Math.max(4, (s.questions / max) * 100)}%` : "2px" }}
                />
                {survol === i && (
                  <div className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-1.5 -translate-x-1/2 rounded bg-encre px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap text-white">
                    {s.questions} question{s.questions > 1 ? "s" : ""}
                    {s.questions ? ` · ${s.reussies} justes (${Math.round((s.reussies / s.questions) * 100)} %)` : ""}
                  </div>
                )}
              </div>
            );
          })}
        </div>
        <div className="mt-1 flex gap-2">
          {stats.semaines.map((s, i) => (
            <span key={s.debut} className={`flex-1 text-center text-[11px] tabular-nums ${i === stats.semaines.length - 1 ? "font-semibold text-encre" : "text-encre-3"}`}>
              {i === stats.semaines.length - 1 ? "Cette sem." : new Date(s.debut + "T00:00:00").toLocaleDateString("fr-FR", { day: "2-digit", month: "2-digit" })}
            </span>
          ))}
        </div>
      </div>
    </Carte>
  );
}

function Chiffre({ valeur, libelle }: { valeur: string; libelle: string }) {
  return (
    <div className="rounded-md bg-fond/70 px-3 py-2.5">
      <p className="text-[22px] leading-none font-bold tabular-nums">{valeur}</p>
      <p className="mt-1 text-[11.5px] text-encre-3">{libelle}</p>
    </div>
  );
}

// ---------------------------------------------------------------- moyennes par matière

function Legende() {
  return (
    <div className="flex items-center gap-3 text-[11.5px] text-encre-2">
      <span className="inline-flex items-center gap-1.5">
        <span className="h-2 w-4 rounded-sm bg-bleu" /> Toi
      </span>
      <span className="inline-flex items-center gap-1.5">
        <span className="h-3 w-0.5 bg-encre" /> Classe
      </span>
      <span className="inline-flex items-center gap-1.5">
        <span className="size-2.5 rounded-full border-2 border-accent bg-surface" /> Objectif
      </span>
    </div>
  );
}

function LigneMatiere({ l, conseil, aller }: { l: LigneMoyenne; conseil?: ConseilMatiere; aller: (id: IdPage) => void }) {
  const pct = (x: number) => `${Math.min(100, Math.max(0, (x / 20) * 100))}%`;
  const bulle: ReactNode = (
    <>
      Toi {fr(l.actuelle)} · classe {fr(l.classe)}
      {conseil?.objectif != null && ` · objectif ${fr(conseil.objectif, 1)}`}
    </>
  );
  return (
    <div className="group ligne-survol flex items-center gap-4 border-b border-bordure/70 px-3.5 py-3 last:border-b-0">
      <div className="w-36 shrink-0">
        <Matiere nom={l.matiere} />
        {conseil && conseil.priorite === 1 && <p className="mt-0.5 text-[11px] font-medium text-orange">à faire remonter</p>}
      </div>

      {/* Barre sur 20 : toi (bleu), la classe (trait), l'objectif (rond). */}
      <div className="relative h-3 flex-1">
        <div className="absolute inset-0 rounded-full bg-fond-2" />
        {l.actuelle !== null && (
          <div className="survol-glisse absolute inset-y-0 left-0 origin-left rounded-full bg-bleu transition-transform duration-150 ease-out group-hover:scale-y-125" style={{ width: pct(l.actuelle) }} />
        )}
        {l.classe !== null && <div className="absolute -top-1 h-5 w-0.5 rounded bg-encre" style={{ left: pct(l.classe) }} />}
        {conseil?.objectif != null && (
          <div className="absolute top-1/2 size-3.5 -translate-x-1/2 -translate-y-1/2 rounded-full border-2 border-accent bg-surface" style={{ left: pct(conseil.objectif) }} />
        )}
        <div className="pointer-events-none absolute bottom-full left-1/2 z-10 mb-2 -translate-x-1/2 rounded bg-encre px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap text-white opacity-0 transition-opacity duration-150 group-hover:opacity-100">
          {bulle}
        </div>
      </div>

      <MiniCourbe l={l} />
      <div className="w-14 text-right">
        <p className="text-[16px] font-semibold tabular-nums">{fr(l.actuelle)}</p>
      </div>
      <div className="w-32">
        <Evolution valeur={l.evolution} depuis={avant(l)} />
        {l.evolution === null && <span className="text-[11.5px] text-encre-3">{l.periode_actuelle ? court(l.periode_actuelle) : ""}</span>}
      </div>
      <button className="btn btn-fantome h-7 text-encre-2 opacity-60 group-hover:opacity-100" onClick={() => reviserMatiere(l.matiere, aller)} title={`Réviser ${nomMatiere(l.matiere)}`}>
        Réviser <ArrowRight size={13} />
      </button>
    </div>
  );
}

/** Mini-courbe de la moyenne : trimestres puis photos prises pendant le trimestre en cours. */
function MiniCourbe({ l }: { l: LigneMoyenne }) {
  const points = [...l.periodes.filter((p) => p.eleve !== null).map((p) => p.eleve!), ...l.historique.slice(0, -1).map((h) => h.eleve)];
  if (points.length < 2) return <div className="w-16" />;
  const min = Math.min(...points) - 1;
  const max = Math.max(...points) + 1;
  const x = (i: number) => 2 + (i / (points.length - 1)) * 60;
  const y = (v: number) => 18 - ((v - min) / (max - min)) * 16;
  const d = points.map((v, i) => `${i ? "L" : "M"}${x(i).toFixed(1)},${y(v).toFixed(1)}`).join(" ");
  const monte = points[points.length - 1] >= points[0];
  return (
    <svg width="64" height="20" className="shrink-0" aria-label={`Évolution : ${points.map((p) => fr(p)).join(" → ")}`}>
      <path d={d} fill="none" stroke="var(--color-bleu)" strokeWidth="2" strokeLinejoin="round" strokeLinecap="round" />
      <circle cx={x(points.length - 1)} cy={y(points[points.length - 1])} r="3" fill={monte ? "var(--color-vert)" : "var(--color-orange)"} stroke="var(--color-surface)" strokeWidth="1.5" />
    </svg>
  );
}
