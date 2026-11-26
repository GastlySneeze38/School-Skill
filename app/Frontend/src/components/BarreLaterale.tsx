import type { MouseEvent } from "react";
import { animate, m, useMotionValue } from "motion/react";
import { PanelLeftClose, Search, GraduationCap } from "lucide-react";
import { PAGE_PARAMETRES, SECTIONS, type IdPage, type Page } from "../lib/navigation";
import { RESSORT_SUIVI } from "../lib/mouvement";
import { useDonnees } from "../lib/donnees";

export function BarreLaterale({
  page,
  aller,
  ouvrirPalette,
  replier,
  compteurs,
  classe,
}: {
  page: IdPage;
  aller: (id: IdPage) => void;
  ouvrirPalette: () => void;
  replier: () => void;
  compteurs: Partial<Record<IdPage, number>>;
  classe: string | null;
}) {
  const survol = useSurbrillance();

  return (
    <nav className="flex h-full w-[220px] shrink-0 flex-col bg-fond px-2 pt-2 pb-2">
      <div className="flex items-center gap-1 px-1">
        <div className="flex min-w-0 flex-1 items-center gap-2 rounded-md px-1.5 py-1">
          <span className="grid size-5 shrink-0 place-items-center rounded bg-accent text-white">
            <GraduationCap size={13} strokeWidth={2.5} />
          </span>
          <span className="truncate text-[13px] font-semibold">Révisions{classe ? ` · ${classe}` : ""}</span>
        </div>
        <button className="btn btn-fantome h-7 px-1.5" onClick={replier} title="Replier la barre (Ctrl+B)" aria-label="Replier la barre">
          <PanelLeftClose size={15} />
        </button>
      </div>

      <button onClick={ouvrirPalette} className="btn mx-1 mt-2 mb-3 justify-start text-encre-3" title="Aller à une page ou lancer une action">
        <Search size={14} />
        <span className="flex-1 text-left font-normal">Rechercher…</span>
        <span className="kbd">Ctrl K</span>
      </button>

      <div className="relative min-h-0 flex-1 overflow-y-auto" onMouseLeave={survol.quitter}>
        {survol.pastille}
        {SECTIONS.map((s, i) => (
          <div key={i} className="mb-3">
            {s.titre && <p className="px-2.5 pb-1 text-[11px] font-semibold text-encre-3">{s.titre}</p>}
            {s.pages.map((p) => (
              <Entree key={p.id} p={p} active={page === p.id} aller={aller} compteur={compteurs[p.id]} survoler={survol.survoler} />
            ))}
          </div>
        ))}
      </div>

      <div className="border-t border-bordure pt-2">
        <IndicateurIa ouvrir={() => aller("parametres")} />
        <Entree p={PAGE_PARAMETRES} active={page === "parametres"} aller={aller} />
      </div>
    </nav>
  );
}

/**
 * Une seule pastille de survol qui glisse d'un lien à l'autre au lieu d'un fond par lien.
 * Elle n'anime que `y` (transform) et `opacity` : aucun recalcul de mise en page, et elle
 * n'entraîne aucun rendu React (valeurs Motion modifiées directement).
 */
function useSurbrillance() {
  const y = useMotionValue(0);
  const opacite = useMotionValue(0);

  const survoler = (e: MouseEvent<HTMLElement>) => {
    const cible = e.currentTarget.offsetTop;
    if (opacite.get() < 0.1) {
      // Entrée dans la liste : la pastille apparaît sur place, sans glisser depuis l'ancienne position.
      y.jump(cible);
    } else {
      animate(y, cible, RESSORT_SUIVI);
    }
    animate(opacite, 1, { duration: 0.12 });
  };
  const quitter = () => void animate(opacite, 0, { duration: 0.18 });

  // Un élément (pas un composant recréé à chaque rendu) : il n'est jamais remonté.
  const pastille = (
    <m.div
      aria-hidden
      style={{ y, opacity: opacite }}
      className="pointer-events-none absolute top-0 right-0 left-0 h-8 rounded-md bg-black/[0.045]"
    />
  );
  return { survoler, quitter, pastille };
}

/** État de l'IA locale toujours visible : s'il manque un modèle, on le voit tout de suite. */
function IndicateurIa({ ouvrir }: { ouvrir: () => void }) {
  const { ia } = useDonnees();
  const pret = !!ia?.installe;
  return (
    <button
      onClick={ouvrir}
      title={pret ? `IA locale : ${ia?.modele}, sur ton ordinateur` : "Il manque des fichiers de l'IA locale : clique pour voir lesquels"}
      className="mb-1 flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-[13px] text-encre-2 hover:bg-black/[0.045]"
    >
      <span className="relative flex size-[15px] items-center justify-center">
        <span className={`size-2 rounded-full ${ia === null ? "bg-bordure-forte" : pret ? "bg-vert" : "bg-orange"}`} />
      </span>
      <span className="flex-1">IA locale</span>
      <span className={`text-[11.5px] ${pret ? "text-encre-3" : "font-medium text-orange"}`}>
        {ia === null ? "…" : !pret ? "à installer" : ia.ecriture === "pret" ? "chargée" : "en veille"}
      </span>
    </button>
  );
}

function Entree({
  p,
  active,
  aller,
  compteur,
  survoler,
}: {
  p: Page;
  active: boolean;
  aller: (id: IdPage) => void;
  compteur?: number;
  survoler?: (e: MouseEvent<HTMLElement>) => void;
}) {
  const Icone = p.icone;
  return (
    <button
      onClick={() => aller(p.id)}
      onMouseEnter={survoler}
      aria-current={active ? "page" : undefined}
      className={`group relative flex h-8 w-full items-center gap-2.5 rounded-md px-2.5 text-left text-[13.5px] transition-colors duration-150 ${
        active ? "bg-black/[0.07] font-medium text-encre" : "text-encre-2 hover:text-encre"
      } ${survoler ? "" : "hover:bg-black/[0.045]"}`}
    >
      {/* L'icône avance d'un pixel au survol : on sent que le lien répond, sans distraire. */}
      <Icone size={15} style={{ color: p.couleur }} strokeWidth={2} className="survol-glisse transition-transform duration-150 ease-out group-hover:translate-x-px" />
      <span className="flex-1 truncate">{p.titre}</span>
      {!!compteur && <span className="rounded bg-fond-2 px-1.5 text-[11px] font-semibold tabular-nums text-encre-2">{compteur}</span>}
    </button>
  );
}
