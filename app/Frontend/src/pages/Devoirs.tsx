import { useState } from "react";
import { CheckCircle2, Circle, ListTodo, Paperclip } from "lucide-react";
import { Carte, Ligne, Pastille, Vide } from "../components/ui";
import { Matiere } from "../components/partages";
import { joursAvant, libelleJour, useDonnees } from "../lib/donnees";
import { Segments } from "../components/Segments";

type Filtre = "a_faire" | "tous" | "passes";

export function Devoirs() {
  const { pronote } = useDonnees();
  const [filtre, setFiltre] = useState<Filtre>("a_faire");
  const tous = pronote?.devoirs ?? [];
  const visibles = tous.filter((d) => {
    const j = joursAvant(d.pour_le);
    if (filtre === "a_faire") return j >= 0 && !d.fait;
    if (filtre === "tous") return j >= 0;
    return j < 0;
  });

  // Regroupés par jour de rendu : c'est comme ça qu'on s'organise.
  const parJour = new Map<string, typeof visibles>();
  for (const d of filtre === "passes" ? [...visibles].reverse() : visibles) {
    parJour.set(d.pour_le, [...(parJour.get(d.pour_le) ?? []), d]);
  }

  return (
    <div className="flex flex-col gap-3">
      <Segments
        valeur={filtre}
        choisir={setFiltre}
        options={[
          { valeur: "a_faire", libelle: "À faire", compte: tous.filter((d) => joursAvant(d.pour_le) >= 0 && !d.fait).length },
          { valeur: "tous", libelle: "À venir" },
          { valeur: "passes", libelle: "Passés" },
        ]}
      />
      {parJour.size === 0 ? (
        <div className="carte">
          <Vide icone={<ListTodo size={24} />} titre={filtre === "a_faire" ? "Tout est fait 🎉" : "Aucun devoir"} texte="Les devoirs sont récupérés à chaque synchro Pronote." />
        </div>
      ) : (
        [...parJour].map(([jour, liste]) => (
          <Carte
            key={jour}
            titre={<span className="first-letter:uppercase">{libelleJour(jour)}</span>}
            sansMarge
            actions={joursAvant(jour) <= 1 && joursAvant(jour) >= 0 && <Pastille ton="rouge">urgent</Pastille>}
          >
            {liste.map((d, i) => (
              <Ligne key={i} className="items-start">
                {d.fait ? <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-vert" /> : <Circle size={16} className="mt-0.5 shrink-0 text-encre-3" />}
                <div className="w-32 shrink-0 pt-px">
                  <Matiere nom={d.matiere} />
                </div>
                <div className="min-w-0 flex-1">
                  <p className={`text-[13px] whitespace-pre-line ${d.fait ? "text-encre-3 line-through" : ""}`}>{d.description}</p>
                  {d.fichiers.length > 0 && (
                    <div className="mt-1.5 flex flex-wrap gap-1.5">
                      {d.fichiers.map((f) => (
                        <span key={f.nom} className="inline-flex items-center gap-1 rounded border border-bordure px-1.5 py-0.5 text-[12px] text-encre-2">
                          <Paperclip size={11} /> {f.nom}
                        </span>
                      ))}
                    </div>
                  )}
                </div>
              </Ligne>
            ))}
          </Carte>
        ))
      )}
    </div>
  );
}
