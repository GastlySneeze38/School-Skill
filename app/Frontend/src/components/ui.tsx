import type { ReactNode } from "react";
import { ChevronRight, Info, MoreHorizontal } from "lucide-react";

/** Carte façon « insight » : titre + info-bulle + menu, contenu en dessous. */
export function Carte({
  titre,
  aide,
  actions,
  children,
  className = "",
  sansMarge = false,
}: {
  titre: ReactNode;
  aide?: string;
  actions?: ReactNode;
  children: ReactNode;
  className?: string;
  sansMarge?: boolean;
}) {
  return (
    <section className={`carte flex min-w-0 flex-col ${className}`}>
      <header className="flex h-11 shrink-0 items-center gap-1.5 border-b border-bordure px-3.5">
        <h2 className="truncate text-[14px] font-semibold">{titre}</h2>
        {aide && (
          <span title={aide} className="text-encre-3">
            <Info size={14} />
          </span>
        )}
        <div className="ml-auto flex items-center gap-1">
          {actions}
          <button className="btn btn-fantome h-7 px-1.5" title="Options" aria-label="Options">
            <MoreHorizontal size={16} />
          </button>
        </div>
      </header>
      <div className={sansMarge ? "min-h-0 flex-1" : "min-h-0 flex-1 p-3.5"}>{children}</div>
    </section>
  );
}

/** Pastille de comptage ou d'état. */
export function Pastille({
  children,
  ton = "neutre",
}: {
  children: ReactNode;
  ton?: "neutre" | "rouge" | "orange" | "vert" | "bleu" | "violet";
}) {
  const tons = {
    neutre: "bg-fond-2 text-encre-2",
    rouge: "bg-rouge/10 text-rouge",
    orange: "bg-orange/12 text-orange",
    vert: "bg-vert/12 text-vert",
    bleu: "bg-bleu/10 text-bleu",
    violet: "bg-violet/10 text-violet",
  } as const;
  return (
    <span className={`inline-flex h-5 items-center rounded px-1.5 text-[11px] font-semibold tabular-nums ${tons[ton]}`}>
      {children}
    </span>
  );
}

/** État vide : dit ce qui manque et comment l'obtenir, sans culpabiliser. */
export function Vide({ icone, titre, texte, action }: { icone: ReactNode; titre: string; texte: string; action?: ReactNode }) {
  return (
    <div className="flex flex-col items-center justify-center gap-2 px-6 py-10 text-center">
      <div className="text-encre-3">{icone}</div>
      <p className="font-semibold">{titre}</p>
      <p className="max-w-sm text-[13px] text-encre-3">{texte}</p>
      {action && <div className="mt-2">{action}</div>}
    </div>
  );
}

/**
 * Ligne de liste dense. Avec `onClick`, elle devient un lien : une flèche glisse en place
 * au survol. Effet en CSS pur (pas un composant Motion par ligne) : une page peut en
 * afficher des dizaines, et le CSS tourne hors du fil principal.
 */
export function Ligne({ children, className = "", onClick }: { children: ReactNode; className?: string; onClick?: () => void }) {
  const base = `ligne-survol flex w-full items-center gap-3 border-b border-bordure/70 px-3.5 py-2.5 text-left last:border-b-0 ${className}`;
  if (!onClick) return <div className={base}>{children}</div>;
  return (
    <button onClick={onClick} className={`group ${base}`}>
      {children}
      <ChevronRight
        size={14}
        className="survol-glisse -ml-1 shrink-0 text-encre-3 opacity-0 transition-[opacity,transform] duration-150 ease-out group-hover:translate-x-0.5 group-hover:opacity-100"
      />
    </button>
  );
}
