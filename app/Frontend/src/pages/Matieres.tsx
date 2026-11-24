import { BookOpen, Play } from "lucide-react";
import { Carte, Pastille, Vide } from "../components/ui";
import { BarrePreparation, LegendeEtats, Matiere, compterEtats } from "../components/partages";
import { COULEUR_ETAT, LIBELLE_ETAT, aujourdhui, etatNotion, useDonnees } from "../lib/donnees";
import type { IdPage } from "../lib/navigation";

/** Matières suivies (notions et états) puis matières pas encore suivies, triées par contenu dispo. */
export function Matieres({ aller }: { aller: (id: IdPage) => void }) {
  const { pronote, suivi } = useDonnees();
  const suivies = (suivi?.matieres ?? []).filter((m) => Object.keys(m.chapitres).length);
  const nomsSuivis = new Set(suivies.map((m) => m.matiere));

  // Même idée que `suivi.py contenu` : on commence par là où il y a le plus de cours.
  const contenu = new Map<string, { cours: number; docs: number }>();
  for (const c of pronote?.cours ?? []) {
    if (!c.contenu || c.debut.slice(0, 10) > aujourdhui() || nomsSuivis.has(c.matiere)) continue;
    const s = contenu.get(c.matiere) ?? { cours: 0, docs: 0 };
    s.cours += 1;
    s.docs += c.contenu.fichiers.length;
    contenu.set(c.matiere, s);
  }
  const aDecouvrir = [...contenu].sort((a, b) => b[1].cours + 3 * b[1].docs - (a[1].cours + 3 * a[1].docs));

  return (
    <div className="flex flex-col gap-4">
      {suivies.length > 0 && <LegendeEtats />}
      {suivies.map((m) => (
        <Carte key={m.matiere} titre={<Matiere nom={m.matiere} />} sansMarge actions={<div className="w-48"><BarrePreparation etats={compterEtats(m)} /></div>}>
          {Object.entries(m.chapitres).map(([id, ch]) => (
            <div key={id} className="border-b border-bordure/70 px-3.5 py-3 last:border-b-0">
              <p className="text-[13px] font-semibold">{ch.titre}</p>
              <div className="mt-2 flex flex-wrap gap-1.5">
                {Object.entries(ch.notions).map(([nid, n]) => {
                  const etat = etatNotion(n);
                  const erreurs = n.essais.filter((e) => e.erreur).map((e) => e.erreur);
                  return (
                    <span
                      key={nid}
                      title={`${LIBELLE_ETAT[etat]}${erreurs.length ? `\nDernière erreur : ${erreurs[erreurs.length - 1]}` : ""}`}
                      className="survol-glisse inline-flex cursor-default items-center gap-1.5 rounded-md border border-bordure bg-surface px-2 py-1 text-[12.5px] transition-[transform,border-color] duration-150 ease-out hover:-translate-y-px hover:border-bordure-forte"
                    >
                      <span className="size-2 rounded-full" style={{ background: COULEUR_ETAT[etat] }} />
                      {n.intitule}
                    </span>
                  );
                })}
              </div>
            </div>
          ))}
        </Carte>
      ))}

      <Carte titre="Pas encore suivies" aide="Classées par quantité de cours récupérée sur Pronote : commence par celles du haut." sansMarge>
        {aDecouvrir.length === 0 ? (
          <Vide icone={<BookOpen size={22} />} titre="Toutes tes matières sont suivies" texte="Bravo, ou alors Pronote n'a pas encore de contenu à proposer." />
        ) : (
          aDecouvrir.map(([nom, s], i) => (
            <div key={nom} className="ligne-survol flex items-center gap-3 border-b border-bordure/70 px-3.5 py-2.5 last:border-b-0">
              <div className="w-36 shrink-0">
                <Matiere nom={nom} />
              </div>
              <span className="flex-1 text-[12.5px] text-encre-3">
                {s.cours} cours avec contenu{s.docs ? ` · ${s.docs} document${s.docs > 1 ? "s" : ""}` : ""}
              </span>
              {i === 0 && <Pastille ton="vert">conseillé</Pastille>}
              <button className="btn h-7" onClick={() => aller("seance")}>
                <Play size={12} /> Tester
              </button>
            </div>
          ))
        )}
      </Carte>
    </div>
  );
}