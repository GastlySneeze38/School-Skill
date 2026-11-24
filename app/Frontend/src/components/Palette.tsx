import { useEffect, useMemo, useRef, useState } from "react";
import { CornerDownLeft, RefreshCw, Search } from "lucide-react";
import { TOUTES_LES_PAGES, type IdPage } from "../lib/navigation";

type Action = { id: string; titre: string; sousTitre: string; icone: React.ReactNode; lancer: () => void; raccourci?: string };

/**
 * Palette de commandes (Ctrl+K) : aller à n'importe quelle page ou lancer une action au clavier.
 * Aucune animation à l'ouverture : c'est une action clavier répétée, elle doit être instantanée.
 */
export function Palette({
  ouverte,
  fermer,
  aller,
  synchroniser,
}: {
  ouverte: boolean;
  fermer: () => void;
  aller: (id: IdPage) => void;
  synchroniser: () => void;
}) {
  const [texte, setTexte] = useState("");
  const [index, setIndex] = useState(0);
  const champ = useRef<HTMLInputElement>(null);

  const actions: Action[] = useMemo(
    () => [
      ...TOUTES_LES_PAGES.map((p) => {
        const Icone = p.icone;
        return {
          id: p.id,
          titre: p.titre,
          sousTitre: p.description,
          icone: <Icone size={15} style={{ color: p.couleur }} />,
          lancer: () => aller(p.id),
          raccourci: p.raccourci,
        };
      }),
      {
        id: "synchro",
        titre: "Synchroniser Pronote",
        sousTitre: "Récupérer les derniers cours, devoirs et notes",
        icone: <RefreshCw size={15} className="text-encre-2" />,
        lancer: synchroniser,
      },
    ],
    [aller, synchroniser],
  );

  const resultats = useMemo(() => {
    const t = normaliser(texte);
    return t ? actions.filter((a) => normaliser(`${a.titre} ${a.sousTitre}`).includes(t)) : actions;
  }, [texte, actions]);

  useEffect(() => {
    if (ouverte) {
      setTexte("");
      setIndex(0);
      champ.current?.focus();
    }
  }, [ouverte]);
  useEffect(() => setIndex(0), [texte]);

  if (!ouverte) return null;

  const valider = (a: Action | undefined) => {
    if (!a) return;
    fermer();
    a.lancer();
  };

  return (
    <div className="fixed inset-0 z-50 flex items-start justify-center bg-black/20 pt-[14vh]" onMouseDown={fermer}>
      <div
        className="carte w-[560px] overflow-hidden shadow-[0_12px_40px_rgb(0_0_0/0.18)]"
        onMouseDown={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Palette de commandes"
      >
        <div className="flex items-center gap-2 border-b border-bordure px-3.5">
          <Search size={16} className="text-encre-3" />
          <input
            ref={champ}
            value={texte}
            onChange={(e) => setTexte(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "ArrowDown") {
                e.preventDefault();
                setIndex((i) => Math.min(i + 1, resultats.length - 1));
              } else if (e.key === "ArrowUp") {
                e.preventDefault();
                setIndex((i) => Math.max(i - 1, 0));
              } else if (e.key === "Enter") {
                valider(resultats[index]);
              } else if (e.key === "Escape") {
                fermer();
              }
            }}
            placeholder="Aller à… ou lancer une action"
            className="h-12 flex-1 bg-transparent text-[15px] outline-none placeholder:text-encre-3"
          />
          <span className="kbd">Échap</span>
        </div>
        <ul className="max-h-[360px] overflow-y-auto p-1.5">
          {resultats.length === 0 && <li className="px-3 py-6 text-center text-[13px] text-encre-3">Rien ne correspond.</li>}
          {resultats.map((a, i) => (
            <li key={a.id}>
              <button
                onMouseMove={() => setIndex(i)}
                onClick={() => valider(a)}
                className={`flex w-full items-center gap-3 rounded-md px-2.5 py-2 text-left ${i === index ? "bg-black/[0.06]" : ""}`}
              >
                {a.icone}
                <span className="min-w-0 flex-1">
                  <span className="block text-[13.5px] font-medium">{a.titre}</span>
                  <span className="block truncate text-[12px] text-encre-3">{a.sousTitre}</span>
                </span>
                {a.raccourci && (
                  <span className="flex gap-1">
                    <span className="kbd">G</span>
                    <span className="kbd">{a.raccourci}</span>
                  </span>
                )}
                {i === index && <CornerDownLeft size={14} className="text-encre-3" />}
              </button>
            </li>
          ))}
        </ul>
      </div>
    </div>
  );
}

const normaliser = (s: string) =>
  s
    .normalize("NFD")
    .replace(/[̀-ͯ]/g, "")
    .toLowerCase()
    .trim();
