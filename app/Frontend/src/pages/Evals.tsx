import { CalendarCheck, Play } from "lucide-react";
import { Carte, Pastille, Vide } from "../components/ui";
import { BarrePreparation, Echeance, LegendeEtats, Matiere, compterEtats, toutesLesEvals } from "../components/partages";
import { libelleJour, useDonnees } from "../lib/donnees";
import type { IdPage } from "../lib/navigation";

export function Evals({ aller }: { aller: (id: IdPage) => void }) {
  const { pronote, suivi } = useDonnees();
  const evals = toutesLesEvals(suivi?.evals ?? [], pronote?.controles_a_venir ?? []);

  return (
    <Carte titre={`${evals.length} évaluation${evals.length > 1 ? "s" : ""} à venir`} aide="Préparation = notions maîtrisées dans les chapitres de l'éval." sansMarge actions={<LegendeEtats />}>
      {evals.length === 0 ? (
        <Vide icone={<CalendarCheck size={24} />} titre="Aucune éval à venir" texte="Dès qu'un contrôle apparaît sur Pronote ou dans un mail d'un prof, il s'affiche ici." />
      ) : (
        <table className="w-full table-fixed text-[13px]">
          <thead>
            <tr className="border-b border-bordure text-left text-[12px] text-encre-3">
              <th className="w-[200px] py-2 pl-3.5 font-medium">Quand</th>
              <th className="w-[140px] py-2 font-medium">Matière</th>
              <th className="py-2 font-medium">Éval</th>
              <th className="w-[26%] py-2 font-medium">Préparation</th>
              <th className="w-[104px] py-2 pr-3.5" />
            </tr>
          </thead>
          <tbody>
            {evals.map((e) => (
              <tr key={e.matiere + e.date} className="ligne-survol border-b border-bordure/70 last:border-b-0">
                <td className="py-2.5 pl-3.5">
                  <div className="flex items-center gap-2">
                    <Echeance date={e.date} />
                    <span className="text-encre-3 first-letter:uppercase">{libelleJour(e.date)}</span>
                  </div>
                </td>
                <td className="py-2.5">
                  <Matiere nom={e.matiere} />
                </td>
                <td className="py-2.5 pr-4">
                  <div className="flex min-w-0 items-center gap-2">
                    <span className="min-w-0 truncate font-medium" title={e.titre}>
                      {e.aConfirmer ? e.titre : `${e.type}${e.titre ? ` · ${e.titre}` : ""}`}
                    </span>
                    {e.aConfirmer && <Pastille ton="orange">à confirmer</Pastille>}
                    {!e.aConfirmer && e.poids >= 3 && <Pastille ton="rouge">gros coef</Pastille>}
                  </div>
                </td>
                <td className="py-2.5 pr-4">
                  {e.aConfirmer ? (
                    <span className="block truncate text-[12px] text-encre-3">Chapitres pas encore connus</span>
                  ) : (
                    <BarrePreparation etats={compterEtats(suivi?.matieres.find((m) => m.matiere === e.matiere), e.chapitres)} />
                  )}
                </td>
                <td className="py-2.5 pr-3.5 text-right">
                  <button className="btn h-7" onClick={() => aller("seance")}>
                    <Play size={12} /> Réviser
                  </button>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
      )}
    </Carte>
  );
}
