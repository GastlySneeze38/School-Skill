import { Vide } from "../components/ui";
import type { Page } from "../lib/navigation";

const PROCHAINEMENT: Partial<Record<Page["id"], string>> = {
  seance: "Le test de compétences en QCM, en grand, avec le brouillon à côté pour rédiger ou calculer.",
  flashcards: "Les cartes générées après chaque test, à revoir au bon moment (répétition espacée).",
  brouillons: "Un éditeur guidé pour les développements et une zone de dessin pour les calculs et schémas.",
  progres: "L'évolution de ta maîtrise par matière, à côté de tes notes.",
};

export function Bientot({ page }: { page: Page }) {
  const Icone = page.icone;
  return (
    <div className="carte">
      <Vide
        icone={<Icone size={26} style={{ color: page.couleur }} />}
        titre="Bientôt ici"
        texte={PROCHAINEMENT[page.id] ?? "Cette page arrive dans une prochaine version."}
      />
    </div>
  );
}
