import { useState } from "react";
import { CalendarDays, Paperclip } from "lucide-react";
import { Carte, Ligne, Pastille, Vide } from "../components/ui";
import { Matiere } from "../components/partages";
import { heure, joursAvant, libelleJour, useDonnees } from "../lib/donnees";
import { Segments } from "../components/Segments";

export function EmploiDuTemps() {
  const { pronote } = useDonnees();
  const [vue, setVue] = useState<"a_venir" | "passes">("a_venir");
  const cours = (pronote?.cours ?? []).filter((c) => (vue === "a_venir" ? joursAvant(c.debut) >= 0 : joursAvant(c.debut) < 0));
  const ordre = vue === "a_venir" ? cours : [...cours].reverse();

  const parJour = new Map<string, typeof ordre>();
  for (const c of ordre) parJour.set(c.debut.slice(0, 10), [...(parJour.get(c.debut.slice(0, 10)) ?? []), c]);

  return (
    <div className="flex flex-col gap-3">
      <Segments
        valeur={vue}
        choisir={setVue}
        options={[
          { valeur: "a_venir", libelle: "À venir" },
          { valeur: "passes", libelle: "Cours passés (avec leur contenu)" },
        ]}
      />
      {parJour.size === 0 ? (
        <div className="carte">
          <Vide icone={<CalendarDays size={24} />} titre="Aucun cours" texte="L'emploi du temps est récupéré à chaque synchro Pronote." />
        </div>
      ) : (
        [...parJour].slice(0, 10).map(([jour, liste]) => (
          <Carte key={jour} titre={<span className="first-letter:uppercase">{libelleJour(jour)}</span>} sansMarge>
            {liste.map((c, i) => (
              <Ligne key={i} className={`items-start ${c.annule ? "opacity-50" : ""}`}>
                <span className="w-24 shrink-0 pt-px text-[12.5px] font-medium tabular-nums text-encre-2">
                  {heure(c.debut)} – {heure(c.fin)}
                </span>
                <div className="w-32 shrink-0 pt-px">
                  <Matiere nom={c.matiere} />
                </div>
                <div className="min-w-0 flex-1">
                  <p className={`text-[13px] ${c.annule ? "line-through" : ""}`}>{c.contenu?.titre || c.contenu?.description || <span className="text-encre-3">—</span>}</p>
                  {c.contenu?.titre && c.contenu.description && (
                    <p className="mt-0.5 line-clamp-2 text-[12.5px] text-encre-3">{c.contenu.description}</p>
                  )}
                  {!!c.contenu?.fichiers.length && (
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      {c.contenu.fichiers.map((f) => (
                        <span key={f.nom} className="inline-flex items-center gap-1 rounded border border-bordure px-1.5 py-0.5 text-[12px] text-encre-2">
                          <Paperclip size={11} /> {f.nom}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
                <div className="flex shrink-0 items-center gap-1.5">
                  {c.controle && <Pastille ton="rouge">Contrôle</Pastille>}
                  {c.annule && <Pastille>Annulé</Pastille>}
                  {c.salle && <span className="text-[12px] text-encre-3">{c.salle}</span>}
                </div>
              </Ligne>
            ))}
          </Carte>
        ))
      )}
    </div>
  );
}
