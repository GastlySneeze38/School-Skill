import { useCallback, useEffect, useRef, useState } from "react";
import { AnimatePresence, m } from "motion/react";
import { BarreLaterale } from "./components/BarreLaterale";
import { EnTete } from "./components/EnTete";
import { Palette } from "./components/Palette";
import { dansTauri, synchroniserPronote } from "./api";
import { FournisseurDonnees, ilYa, joursAvant, useDonnees } from "./lib/donnees";
import { TOUTES_LES_PAGES, pageParId, usePage, type IdPage } from "./lib/navigation";
import { toutesLesEvals } from "./components/partages";
import { VueEnsemble } from "./pages/VueEnsemble";
import { Revision } from "./pages/Revision";
import { Evals } from "./pages/Evals";
import { Devoirs } from "./pages/Devoirs";
import { EmploiDuTemps } from "./pages/EmploiDuTemps";
import { Matieres } from "./pages/Matieres";
import { Parametres } from "./pages/Parametres";
import { Bientot } from "./pages/Bientot";
import { Seance } from "./pages/Seance";
import { Progres } from "./pages/Progres";

export default function App() {
  return (
    <FournisseurDonnees>
      <Coque />
    </FournisseurDonnees>
  );
}

function Coque() {
  const [page, aller] = usePage();
  const { pronote, suivi, recharger } = useDonnees();
  const [paletteOuverte, setPaletteOuverte] = useState(false);
  const [barreRepliee, setBarreRepliee] = useState(() => localStorage.getItem("barre-repliee") === "1");
  const [synchro, setSynchro] = useState<{ enCours: boolean; message: string | null; ok: boolean }>({ enCours: false, message: null, ok: true });

  const basculerBarre = useCallback(() => {
    setBarreRepliee((r) => {
      localStorage.setItem("barre-repliee", r ? "0" : "1");
      return !r;
    });
  }, []);

  const synchroniser = useCallback(async () => {
    if (!dansTauri()) return setSynchro({ enCours: false, ok: false, message: "Synchro indisponible dans l'aperçu navigateur." });
    setSynchro({ enCours: true, message: null, ok: true });
    try {
      const r = await synchroniserPronote();
      await recharger();
      // Le script écrit plusieurs lignes : la première résume tout.
      setSynchro({ enCours: false, ok: r.ok, message: r.message.split("\n")[0] });
    } catch (e) {
      setSynchro({ enCours: false, ok: false, message: String(e) });
    }
  }, [recharger]);

  // Raccourcis clavier : Ctrl+K palette, Ctrl+B barre, « G » puis une lettre pour changer de page.
  const attenteG = useRef(0);
  useEffect(() => {
    const surTouche = (e: KeyboardEvent) => {
      const cible = e.target as HTMLElement;
      const saisie = cible.closest("input, textarea, [contenteditable=true]");
      if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "k") {
        e.preventDefault();
        setPaletteOuverte((o) => !o);
      } else if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === "b") {
        e.preventDefault();
        basculerBarre();
      } else if (!saisie && !e.ctrlKey && !e.metaKey && !e.altKey) {
        if (e.key.toLowerCase() === "g") {
          attenteG.current = Date.now();
        } else if (Date.now() - attenteG.current < 1000) {
          const cibleePage = TOUTES_LES_PAGES.find((p) => p.raccourci?.toLowerCase() === e.key.toLowerCase());
          if (cibleePage) aller(cibleePage.id);
          attenteG.current = 0;
        }
      }
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [aller, basculerBarre]);

  const compteurs: Partial<Record<IdPage, number>> = {
    devoirs: (pronote?.devoirs ?? []).filter((d) => !d.fait && joursAvant(d.pour_le) >= 0).length,
    evals: toutesLesEvals(suivi?.evals ?? [], pronote?.controles_a_venir ?? []).filter((e) => joursAvant(e.date) <= 7).length,
  };

  const infos = pageParId(page);

  return (
    <div className="flex h-full">
      {!barreRepliee && (
        <BarreLaterale
          page={page}
          aller={aller}
          ouvrirPalette={() => setPaletteOuverte(true)}
          replier={basculerBarre}
          compteurs={compteurs}
          classe={pronote?.eleve.classe ?? null}
        />
      )}

      <main className="flex min-w-0 flex-1 flex-col border-l border-bordure bg-[#f3f4ef]">
        <EnTete
          page={infos}
          barreRepliee={barreRepliee}
          deplier={basculerBarre}
          derniereSynchro={pronote ? ilYa(pronote.synchronise_le) : null}
          synchroEnCours={synchro.enCours}
          synchroniser={synchroniser}
        />
        {/* Pas d'animation entre les pages : on navigue trop souvent pour attendre. */}
        <div className="min-h-0 flex-1 overflow-y-auto px-6 pt-4 pb-8">
          {page === "aujourdhui" && <Revision aller={aller} />}
          {page === "apercu" && <VueEnsemble aller={aller} />}
          {page === "evals" && <Evals aller={aller} />}
          {page === "devoirs" && <Devoirs />}
          {page === "emploi" && <EmploiDuTemps />}
          {page === "matieres" && <Matieres aller={aller} />}
          {page === "parametres" && <Parametres />}
          {page === "seance" && <Seance aller={aller} />}
          {page === "progres" && <Progres aller={aller} />}
          {(page === "flashcards" || page === "brouillons") && <Bientot page={infos} />}
        </div>
      </main>

      <Palette ouverte={paletteOuverte} fermer={() => setPaletteOuverte(false)} aller={aller} synchroniser={synchroniser} />

      {/* Résultat de synchro : entre et sort par le bas, sortie plus rapide que l'entrée */}
      <AnimatePresence>
        {synchro.message && (
          <m.div
            key={synchro.message}
            initial={{ opacity: 0, transform: "translateY(12px)" }}
            animate={{ opacity: 1, transform: "translateY(0px)", transition: { duration: 0.22, ease: [0.23, 1, 0.32, 1] } }}
            exit={{ opacity: 0, transform: "translateY(8px)", transition: { duration: 0.15 } }}
            className="carte fixed right-5 bottom-5 z-40 flex max-w-sm items-start gap-3 p-3 shadow-[0_8px_24px_rgb(0_0_0/0.12)]"
          >
            <span className={`mt-1 size-2 shrink-0 rounded-full ${synchro.ok ? "bg-vert" : "bg-rouge"}`} />
            <p className="flex-1 text-[13px]">{synchro.message}</p>
            <button className="btn btn-fantome h-6 px-1.5 text-encre-3" onClick={() => setSynchro((s) => ({ ...s, message: null }))} aria-label="Fermer">
              ✕
            </button>
          </m.div>
        )}
      </AnimatePresence>
    </div>
  );
}
