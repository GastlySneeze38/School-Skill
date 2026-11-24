/** Contrôle segmenté (filtres de page). Changement instantané : on le manipule souvent. */
export function Segments<T extends string>({
  valeur,
  choisir,
  options,
}: {
  valeur: T;
  choisir: (v: T) => void;
  options: { valeur: T; libelle: string; compte?: number }[];
}) {
  return (
    <div className="inline-flex w-fit rounded-md border border-bordure bg-fond-2 p-0.5">
      {options.map((o) => (
        <button
          key={o.valeur}
          onClick={() => choisir(o.valeur)}
          className={`flex h-7 items-center gap-1.5 rounded px-3 text-[13px] font-medium ${
            valeur === o.valeur ? "bg-surface text-encre shadow-[0_1px_0_var(--color-bordure-forte)]" : "text-encre-2 hover:text-encre"
          }`}
        >
          {o.libelle}
          {o.compte !== undefined && <span className="text-[11px] tabular-nums text-encre-3">{o.compte}</span>}
        </button>
      ))}
    </div>
  );
}
