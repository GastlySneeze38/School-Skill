import { PanelLeftOpen, RefreshCw } from "lucide-react";
import type { ReactNode } from "react";
import { m } from "motion/react";
import { RESSORT_ICONE } from "../lib/mouvement";
import type { Page } from "../lib/navigation";

/** En-tête de page : titre + description, puis une barre d'outils propre à la page. */
export function EnTete({
  page,
  barreRepliee,
  deplier,
  derniereSynchro,
  synchroEnCours,
  synchroniser,
  outils,
}: {
  page: Page;
  barreRepliee: boolean;
  deplier: () => void;
  derniereSynchro: string | null;
  synchroEnCours: boolean;
  synchroniser: () => void;
  outils?: ReactNode;
}) {
  const Icone = page.icone;
  return (
    <header className="shrink-0 px-6 pt-4">
      <div className="flex items-center gap-2">
        {barreRepliee && (
          <button className="btn btn-fantome h-7 px-1.5" onClick={deplier} title="Déplier la barre (Ctrl+B)" aria-label="Déplier la barre">
            <PanelLeftOpen size={15} />
          </button>
        )}
        <Icone size={18} style={{ color: page.couleur }} />
        <h1 className="text-[19px] font-bold tracking-tight">{page.titre}</h1>
      </div>
      <p className="mt-1.5 text-[13.5px] text-encre-2">{page.description}</p>

      <div className="mt-3 flex flex-wrap items-center gap-2">
        {outils}
        <div className="ml-auto flex items-center gap-2">
          {derniereSynchro && <span className="text-[12.5px] text-encre-3">Pronote synchronisé {derniereSynchro}</span>}
          {/* Au survol, l'icône fait un quart de tour (« ça va rafraîchir ») ; pendant la synchro
              elle tourne vite, pour que l'attente paraisse plus courte. */}
          <m.button whileHover={synchroEnCours ? undefined : "survol"} className="btn" onClick={synchroniser} disabled={synchroEnCours}>
            <m.span variants={{ survol: { rotate: 90 } }} transition={RESSORT_ICONE} className="inline-flex">
              <RefreshCw size={14} className={synchroEnCours ? "animate-[spin_0.6s_linear_infinite]" : ""} />
            </m.span>
            {synchroEnCours ? "Synchronisation…" : "Synchroniser"}
          </m.button>
        </div>
      </div>
    </header>
  );
}
