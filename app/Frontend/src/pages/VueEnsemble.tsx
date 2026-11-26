import { useEffect, useState } from "react";
import { m } from "motion/react";
import { ArrowRight, CalendarCheck, CheckCircle2, Circle, Play, TrendingUp } from "lucide-react";
import { Carte, Ligne, Pastille, Vide } from "../components/ui";
import { BarrePreparation, Echeance, Matiere, compterEtats, toutesLesEvals } from "../components/partages";
import { aujourdhui, fr, heure, joursAvant, libelleJour, nomMatiere, useDonnees } from "../lib/donnees";
import { reviserMatiere } from "./Progres";
import type { IdPage } from "../lib/navigation";
import { EASE_SORTIE, RESSORT_ICONE, SURVOL } from "../lib/mouvement";

/** Apparition en cascade, une seule fois au chargement de la page (pas à chaque clic). */
const cascade = {
  cache: { opacity: 0, transform: "translateY(6px)" },
  visible: (i: number) => ({
    opacity: 1,
    transform: "translateY(0px)",
    transition: { delay: i * 0.05, duration: 0.25, ease: EASE_SORTIE },
  }),
};

/** Vrai seulement au tout premier affichage de la session. */
let dejaAffichee = false;

export function VueEnsemble({ aller }: { aller: (id: IdPage) => void }) {
  const { pronote, suivi, moyennes, analyse } = useDonnees();
  // On revient sur cette page des dizaines de fois : la cascade ne joue qu'une fois.
  const [initial] = useState<"cache" | false>(() => (dejaAffichee ? false : "cache"));
  useEffect(() => {
    dejaAffichee = true;
  }, []);
  const evals = toutesLesEvals(suivi?.evals ?? [], pronote?.controles_a_venir ?? []);
  // Une éval dans la semaine passe d'abord ; sinon, la matière où l'IA voit le plus de points à gagner.
  const prochaine = evals.find((e) => joursAvant(e.date) <= 7);
  const aRemonter = analyse?.matieres.find((c) => c.priorite === 1);
  const matiereSeance = prochaine?.matiere ?? aRemonter?.matiere;
  const devoirs = (pronote?.devoirs ?? []).filter((d) => joursAvant(d.pour_le) >= 0 && joursAvant(d.pour_le) <= 7);
  const aFaire = devoirs.filter((d) => !d.fait);
  const coursDuJour = (pronote?.cours ?? []).filter((c) => c.debut.slice(0, 10) === aujourdhui() && !c.annule);

  return (
    <div className="grid grid-cols-1 gap-4 xl:grid-cols-2">
      {/* La séance du jour : la décision est déjà prise, il ne reste qu'à cliquer */}
      <m.div custom={0} variants={cascade} initial={initial} animate="visible" className="xl:col-span-2">
        {/* Survol de la carte → l'icône s'anime et la flèche avance : les deux disent
            « c'est par ici ». Motion propage l'état « survol » aux enfants (variants). */}
        <m.section whileHover="survol" className="carte flex items-center gap-5 p-5">
          <m.div
            variants={{ survol: { scale: 1.08, rotate: -6 } }}
            transition={RESSORT_ICONE}
            className="grid size-12 shrink-0 place-items-center rounded-lg bg-accent/10 text-accent"
          >
            <Play size={22} fill="currentColor" />
          </m.div>
          <div className="min-w-0 flex-1">
            <p className="text-[12px] font-semibold tracking-wide text-encre-3 uppercase">Séance du jour · ~20 min</p>
            <p className="mt-0.5 truncate text-[17px] font-semibold">
              {prochaine
                ? `Préparer ${nomMatiere(prochaine.matiere)} — ${prochaine.type} ${libelleJour(prochaine.date).toLowerCase()}`
                : aRemonter
                  ? `Faire remonter ${nomMatiere(aRemonter.matiere)} — jusqu'à +${fr(aRemonter.potentiel, 1)} pt possible`
                  : "Test de compétences sur la matière de ton choix"}
            </p>
            <p className="mt-0.5 text-[13px] text-encre-2">
              {prochaine
                ? "Test de compétences en QCM, puis révision ciblée sur ce qui coince."
                : aRemonter
                  ? `À réviser : ${aRemonter.a_reviser}`
                  : "Aucune éval proche : on fait le point sur tes cours."}
            </p>
          </div>
          <button
            className="btn btn-primaire h-10 px-4 text-[14px]"
            onClick={() => (matiereSeance ? reviserMatiere(matiereSeance, aller) : aller("seance"))}
          >
            Commencer
            <m.span variants={{ survol: { x: 3 } }} transition={SURVOL} className="inline-flex">
              <ArrowRight size={15} />
            </m.span>
          </button>
        </m.section>
      </m.div>

      <m.div custom={1} variants={cascade} initial={initial} animate="visible">
        <Carte
          titre="Évaluations à venir"
          aide="Ta préparation : la part de notions maîtrisées dans les chapitres de l'éval."
          sansMarge
          className="h-full"
          actions={<button className="btn btn-fantome h-7 text-encre-2" onClick={() => aller("evals")}>Tout voir</button>}
        >
          {evals.length === 0 ? (
            <Vide icone={<CalendarCheck size={22} />} titre="Aucune éval repérée" texte="Les contrôles marqués dans Pronote ou annoncés par mail apparaîtront ici." />
          ) : (
            evals.slice(0, 5).map((e) => {
              const m = suivi?.matieres.find((x) => x.matiere === e.matiere);
              return (
                <Ligne key={e.matiere + e.date} onClick={() => aller("evals")}>
                  <Echeance date={e.date} />
                  <div className="w-28 shrink-0">
                    <Matiere nom={e.matiere} />
                  </div>
                  <div className="min-w-0 flex-1">
                    {e.aConfirmer ? (
                      <span className="block truncate text-[12.5px] text-encre-3" title={e.titre}>
                        Repérée sur Pronote · {e.titre}
                      </span>
                    ) : (
                      <BarrePreparation etats={compterEtats(m, e.chapitres)} />
                    )}
                  </div>
                </Ligne>
              );
            })
          )}
        </Carte>
      </m.div>

      <m.div custom={2} variants={cascade} initial={initial} animate="visible">
        <Carte
          titre="Devoirs de la semaine"
          aide="Récupérés sur Pronote. Les devoirs pour demain passent avant les révisions."
          sansMarge
          className="h-full"
          actions={aFaire.length > 0 && <Pastille ton="orange">{aFaire.length} à faire</Pastille>}
        >
          {devoirs.length === 0 ? (
            <Vide icone={<CheckCircle2 size={22} />} titre="Rien à rendre cette semaine" texte="Profites-en pour avancer tes révisions." />
          ) : (
            devoirs.slice(0, 6).map((d, i) => (
              <Ligne key={i} onClick={() => aller("devoirs")}>
                {d.fait ? <CheckCircle2 size={16} className="shrink-0 text-vert" /> : <Circle size={16} className="shrink-0 text-encre-3" />}
                <div className="w-28 shrink-0">
                  <Matiere nom={d.matiere} />
                </div>
                <p className={`min-w-0 flex-1 truncate text-[13px] ${d.fait ? "text-encre-3 line-through" : ""}`} title={d.description}>
                  {d.description}
                </p>
                <span className="shrink-0 text-[12px] text-encre-3">{libelleJour(d.pour_le)}</span>
              </Ligne>
            ))
          )}
        </Carte>
      </m.div>

      <m.div custom={3} variants={cascade} initial={initial} animate="visible">
        <Carte titre="Cours d'aujourd'hui" sansMarge className="h-full" actions={<button className="btn btn-fantome h-7 text-encre-2" onClick={() => aller("emploi")}>Emploi du temps</button>}>
          {coursDuJour.length === 0 ? (
            <Vide icone={<CalendarCheck size={22} />} titre="Pas cours aujourd'hui" texte="Le prochain cours apparaîtra ici." />
          ) : (
            coursDuJour.map((c, i) => (
              <Ligne key={i} onClick={() => aller("emploi")}>
                <span className="w-12 shrink-0 text-[12.5px] font-medium tabular-nums text-encre-2">{heure(c.debut)}</span>
                <div className="w-28 shrink-0">
                  <Matiere nom={c.matiere} />
                </div>
                <p className="min-w-0 flex-1 truncate text-[13px] text-encre-2">{c.contenu?.titre || c.contenu?.description || "—"}</p>
                {c.controle && <Pastille ton="rouge">Contrôle</Pastille>}
              </Ligne>
            ))
          )}
        </Carte>
      </m.div>

<m.div custom={4} variants={cascade} initial={initial} animate="visible">
        <Carte
          titre="Tes moyennes"
          aide="Ta moyenne générale et les matières où l'IA voit le plus de points à gagner."
          sansMarge
          className="h-full"
          actions={<button className="btn btn-fantome h-7 text-encre-2" onClick={() => aller("progres")}>Voir mes progrès</button>}
        >
          {!moyennes ? (
            <Vide icone={<TrendingUp size={22} />} titre="Pas encore de moyennes" texte="Elles arriveront avec la prochaine synchro Pronote." />
          ) : (
            <>
              <Ligne onClick={() => aller("progres")}>
                <div className="flex-1">
                  <p className="text-[12px] text-encre-3">Moyenne générale · {moyennes.generale.periode_actuelle}</p>
                  <p className="text-[26px] leading-tight font-bold tabular-nums">{fr(moyennes.generale.actuelle)}</p>
                </div>
                <div className="text-right text-[12.5px] text-encre-2">
                  <p>classe {fr(moyennes.generale.classe)}</p>
                  <p className="text-vert">
                    au-dessus dans {moyennes.au_dessus_classe}/{moyennes.matieres.length} matières
                  </p>
                </div>
              </Ligne>
              {(analyse?.matieres ?? []).filter((c) => c.priorite === 1).slice(0, 3).map((c) => (
                <Ligne key={c.matiere} onClick={() => reviserMatiere(c.matiere, aller)}>
                  <div className="w-28 shrink-0">
                    <Matiere nom={c.matiere} />
                  </div>
                  <p className="min-w-0 flex-1 truncate text-[12.5px] text-encre-2">{c.a_reviser}</p>
                  <Pastille ton="orange">+{fr(c.potentiel, 1)} pt</Pastille>
                </Ligne>
              ))}
              {!analyse && (
                <Ligne onClick={() => aller("progres")}>
                  <p className="flex-1 text-[12.5px] text-encre-2">Demande à l'IA où tu peux gagner des points.</p>
                </Ligne>
              )}
            </>
          )}
        </Carte>
      </m.div>
    </div>
  );
}
