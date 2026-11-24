import { useState } from "react";
import { COULEUR_ETAT, LIBELLE_ETAT, couleurMatiere, etatNotion, joursAvant, nomMatiere, type EtatNotion } from "../lib/donnees";
import type { EvalSuivie, MatiereSuivie } from "../api";
import { Pastille } from "./ui";

/** Nom court de la matière avec sa couleur : le repère visuel le plus fréquent de l'app. */
export function Matiere({ nom }: { nom: string }) {
  return (
    <span className="inline-flex min-w-0 items-center gap-1.5 text-[13px] font-medium">
      <span className="size-2 shrink-0 rounded-full" style={{ background: couleurMatiere(nom) }} />
      <span className="truncate">{nomMatiere(nom)}</span>
    </span>
  );
}

/** « J-3 », coloré selon l'urgence. */
export function Echeance({ date }: { date: string }) {
  const j = joursAvant(date);
  const texte = j === 0 ? "Aujourd'hui" : j === 1 ? "Demain" : j < 0 ? `il y a ${-j} j` : `J-${j}`;
  return <Pastille ton={j <= 1 ? "rouge" : j <= 4 ? "orange" : "neutre"}>{texte}</Pastille>;
}

export const ORDRE_ETATS: EtatNotion[] = ["vert", "a_revoir", "orange", "rouge", "non_teste"];

/** Compte les notions par état pour une liste de chapitres d'une matière. */
export function compterEtats(m: MatiereSuivie | undefined, chapitres?: string[]) {
  const c: Record<EtatNotion, number> = { vert: 0, a_revoir: 0, orange: 0, rouge: 0, non_teste: 0 };
  if (!m) return c;
  for (const [id, ch] of Object.entries(m.chapitres)) {
    if (chapitres && !chapitres.includes(id)) continue;
    for (const n of Object.values(ch.notions)) c[etatNotion(n)]++;
  }
  return c;
}

/**
 * Barre empilée de préparation : une part par état, comme un graphique de répartition.
 * Au survol d'une part : elle s'épaissit (scaleY), les autres s'estompent, une bulle dit
 * ce qu'elle représente. Seulement transform et opacity, en CSS (transitions interruptibles
 * quand on balaie la barre d'un bout à l'autre).
 */
export function BarrePreparation({ etats, large = false }: { etats: Record<EtatNotion, number>; large?: boolean }) {
  const [survole, setSurvole] = useState<EtatNotion | null>(null);
  const [bulle, setBulle] = useState<{ etat: EtatNotion; gauche: number } | null>(null);
  const total = ORDRE_ETATS.reduce((s, e) => s + etats[e], 0);
  if (total === 0) return <span className="text-[12px] text-encre-3">Notions pas encore découpées</span>;
  const pret = Math.round((etats.vert / total) * 100);
  const parts = ORDRE_ETATS.filter((e) => etats[e]);

  let cumul = 0;
  return (
    <div className="flex min-w-0 items-center gap-2.5">
      <div className={`relative flex flex-1 gap-0.5 ${large ? "h-3" : "h-2"}`} onMouseLeave={() => setSurvole(null)}>
        {parts.map((e) => {
          const debut = cumul;
          cumul += etats[e];
          return (
            <div
              key={e}
              onMouseEnter={() => {
                setSurvole(e);
                setBulle({ etat: e, gauche: ((debut + etats[e] / 2) / total) * 100 });
              }}
              style={{
                flex: etats[e],
                background: COULEUR_ETAT[e],
                transform: survole === e ? "scaleY(1.6)" : "scaleY(1)",
                opacity: survole && survole !== e ? 0.35 : 1,
              }}
              className="survol-glisse h-full transition-[transform,opacity] duration-150 ease-out first:rounded-l-sm last:rounded-r-sm"
            />
          );
        })}
        {/* La bulle garde son dernier contenu pendant qu'elle s'efface, pour ne pas clignoter. */}
        {bulle && (
          <div
            style={{ left: `${bulle.gauche}%`, opacity: survole ? 1 : 0, transform: `translate(-50%, ${survole ? "0px" : "3px"})` }}
            className="pointer-events-none absolute bottom-full z-10 mb-2 rounded bg-encre px-1.5 py-0.5 text-[11px] font-medium whitespace-nowrap text-white transition-[opacity,transform] duration-150 ease-out"
          >
            {etats[bulle.etat]} {LIBELLE_ETAT[bulle.etat].toLowerCase()}
          </div>
        )}
      </div>
      <span className="w-9 shrink-0 text-right text-[12px] font-semibold tabular-nums text-encre-2">{pret}%</span>
    </div>
  );
}

/** Légende des états, pour les pages qui affichent des barres. */
export function LegendeEtats() {
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-1">
      {ORDRE_ETATS.map((e) => (
        <span key={e} className="inline-flex items-center gap-1.5 text-[12px] text-encre-2">
          <span className="size-2 rounded-sm" style={{ background: COULEUR_ETAT[e] }} />
          {LIBELLE_ETAT[e]}
        </span>
      ))}
    </div>
  );
}

/** Évals du suivi + contrôles repérés par Pronote pas encore dans le suivi, triés par date. */
export function toutesLesEvals(evalsSuivi: EvalSuivie[], controlesPronote: { matiere: string; date: string; indice: string }[]) {
  const cle = (m: string, d: string) => `${m}|${d}`;
  const connues = new Set(evalsSuivi.map((e) => cle(e.matiere, e.date)));
  const reperees = controlesPronote
    .filter((c) => !connues.has(cle(c.matiere, c.date)))
    .map((c) => ({ matiere: c.matiere, date: c.date, type: "à confirmer", titre: c.indice, chapitres: [] as string[], poids: 1, aConfirmer: true }));
  return [...evalsSuivi.map((e) => ({ ...e, aConfirmer: false })), ...reperees]
    .filter((e) => joursAvant(e.date) >= 0)
    .sort((a, b) => a.date.localeCompare(b.date));
}
