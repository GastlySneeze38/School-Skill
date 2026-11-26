import { useCallback, useEffect, useMemo, useState } from "react";
import { m } from "motion/react";
import { ArrowRight, Check, CircleHelp, Loader2, Play, RotateCcw, Sparkles, X } from "lucide-react";
import { Carte, Pastille, Vide } from "../components/ui";
import { Echeance, Matiere, compterEtats, BarrePreparation } from "../components/partages";
import { contenuMatieres, dansTauri, enregistrerReponse, preparerTest, type ContenuMatiere, type Question, type Test } from "../api";
import { COULEUR_ETAT, LIBELLE_ETAT, etatNotion, joursAvant, nomMatiere, useDonnees } from "../lib/donnees";
import { EASE_SORTIE } from "../lib/mouvement";
import type { IdPage } from "../lib/navigation";

type Reponse = { choix: number | null; juste: boolean };
type Etape =
  | { nom: "choix" }
  | { nom: "preparation"; matiere: string }
  | { nom: "test"; test: Test; index: number; reponses: Reponse[] }
  | { nom: "bilan"; test: Test; reponses: Reponse[] }
  | { nom: "erreur"; matiere: string; message: string };

export function Seance({ aller }: { aller: (id: IdPage) => void }) {
  const [etape, setEtape] = useState<Etape>({ nom: "choix" });
  const { recharger } = useDonnees();

  const lancer = useCallback(async (matiere: string) => {
    setEtape({ nom: "preparation", matiere });
    try {
      const test = await preparerTest(matiere, 8);
      setEtape({ nom: "test", test, index: 0, reponses: [] });
    } catch (e) {
      setEtape({ nom: "erreur", matiere, message: String(e) });
    }
  }, []);

  const repondre = useCallback(
    (choix: number | null) => {
      if (etape.nom !== "test") return;
      const q = etape.test.questions[etape.index];
      const juste = choix === q.bonne;
      // Enregistré tout de suite (rien n'est perdu si l'app se ferme), sans attendre pour passer à la suite.
      void enregistrerReponse(etape.test.matiere, q, choix).catch(() => {});
      const reponses = [...etape.reponses, { choix, juste }];
      if (etape.index + 1 < etape.test.questions.length) {
        setEtape({ ...etape, index: etape.index + 1, reponses });
      } else {
        setEtape({ nom: "bilan", test: etape.test, reponses });
        void recharger();
      }
    },
    [etape, recharger],
  );

  // Venu d'un bouton « Réviser » (Progrès, Aujourd'hui) : on lance directement sur cette matière.
  useEffect(() => {
    const demandee = sessionStorage.getItem("seance-matiere");
    if (demandee) {
      sessionStorage.removeItem("seance-matiere");
      void lancer(demandee);
    }
  }, [lancer]);

  if (etape.nom === "preparation") return <Preparation matiere={etape.matiere} annuler={() => setEtape({ nom: "choix" })} />;
  if (etape.nom === "test") return <Questionnaire test={etape.test} index={etape.index} repondre={repondre} />;
  if (etape.nom === "bilan") return <Bilan test={etape.test} reponses={etape.reponses} recommencer={() => setEtape({ nom: "choix" })} aller={aller} />;
  return (
    <ChoixMatiere
      lancer={lancer}
      aller={aller}
      erreur={etape.nom === "erreur" ? etape : null}
    />
  );
}

// ---------------------------------------------------------------- 1. choix

function ChoixMatiere({ lancer, aller, erreur }: { lancer: (m: string) => void; aller: (id: IdPage) => void; erreur: { matiere: string; message: string } | null }) {
  const { suivi, ia, analyse } = useDonnees();
  const [contenu, setContenu] = useState<ContenuMatiere[]>([]);
  useEffect(() => {
    if (dansTauri()) contenuMatieres().then(setContenu).catch(() => setContenu([]));
  }, []);

  const evals = (suivi?.evals ?? []).filter((e) => joursAvant(e.date) >= 0).sort((a, b) => a.date.localeCompare(b.date));
  const options = useMemo(() => {
    const vues = new Set<string>();
    const liste: { matiere: string; eval?: (typeof evals)[number]; detail: string }[] = [];
    for (const e of evals) {
      if (vues.has(e.matiere)) continue;
      vues.add(e.matiere);
      liste.push({ matiere: e.matiere, eval: e, detail: `${e.type}${e.titre ? ` · ${e.titre}` : ""}` });
    }
    // Puis les matières où l'IA voit le plus de points à gagner.
    for (const c of (analyse?.matieres ?? []).filter((x) => x.priorite <= 2)) {
      if (vues.has(c.matiere)) continue;
      vues.add(c.matiere);
      liste.push({ matiere: c.matiere, detail: `+${String(c.potentiel).replace(".", ",")} pt possible · ${c.a_reviser}` });
    }
    for (const c of contenu) {
      if (vues.has(c.matiere)) continue;
      vues.add(c.matiere);
      liste.push({ matiere: c.matiere, detail: `${c.cours} cours avec contenu${c.docs ? ` · ${c.docs} doc${c.docs > 1 ? "s" : ""}` : ""}` });
    }
    for (const m of suivi?.matieres ?? []) {
      if (vues.has(m.matiere)) continue;
      vues.add(m.matiere);
      liste.push({ matiere: m.matiere, detail: "notions déjà suivies" });
    }
    return liste;
  }, [evals, contenu, suivi, analyse]);

  return (
    <div className="grid max-w-3xl gap-4">
      {ia && !ia.installe && (
        <div className="carte flex items-center gap-3 border-orange/40 bg-orange/5 p-3.5">
          <CircleHelp size={18} className="shrink-0 text-orange" />
          <p className="flex-1 text-[13px]">Il manque des fichiers de l'IA locale : elle en a besoin pour écrire les questions.</p>
          <button className="btn" onClick={() => aller("parametres")}>
            Voir ce qui manque
          </button>
        </div>
      )}
      {erreur && (
        <div className="carte p-3.5 text-[13px]">
          <p className="font-semibold text-rouge">La préparation du test sur {nomMatiere(erreur.matiere)} a échoué</p>
          <p className="mt-1 text-encre-2">{erreur.message}</p>
        </div>
      )}
      <Carte
        titre="Test de compétences"
        aide="L'IA locale rédige les questions à partir de tes cours Pronote, en suivant les mêmes règles que le skill de révision."
        sansMarge
      >
        <p className="border-b border-bordure/70 px-3.5 py-3 text-[13px] text-encre-2">
          8 questions en QCM, une à la fois. Pas de correction pendant le test : tu as tout à la fin, avec ta carte forces / faiblesses.
          « Je sais pas » est une vraie réponse, mieux vaut ça que deviner.
        </p>
        {options.length === 0 ? (
          <Vide icone={<Play size={22} />} titre="Aucune matière pour l'instant" texte="Synchronise Pronote pour récupérer tes cours." />
        ) : (
          options.map((o, i) => {
            const m = suivi?.matieres.find((x) => x.matiere === o.matiere);
            return (
              <div key={o.matiere} className="ligne-survol flex items-center gap-3 border-b border-bordure/70 px-3.5 py-2.5 last:border-b-0">
                <div className="w-36 shrink-0">
                  <Matiere nom={o.matiere} />
                </div>
                <div className="min-w-0 flex-1">
                  <p className="truncate text-[12.5px] text-encre-2">{o.detail}</p>
                  {m && <div className="mt-1 w-56"><BarrePreparation etats={compterEtats(m, o.eval?.chapitres.length ? o.eval.chapitres : undefined)} /></div>}
                </div>
                {o.eval && <Echeance date={o.eval.date} />}
                {i === 0 && <Pastille ton="vert">conseillé</Pastille>}
                <button className={`btn ${i === 0 ? "btn-primaire" : ""}`} disabled={!ia?.installe} onClick={() => lancer(o.matiere)}>
                  <Play size={12} /> Tester
                </button>
              </div>
            );
          })
        )}
      </Carte>
    </div>
  );
}

// ---------------------------------------------------------------- 2. préparation

function Preparation({ matiere, annuler }: { matiere: string; annuler: () => void }) {
  const [secondes, setSecondes] = useState(0);
  useEffect(() => {
    const t = setInterval(() => setSecondes((s) => s + 1), 1000);
    return () => clearInterval(t);
  }, []);
  const etapes = ["Lecture de tes cours Pronote", "Découpage en notions", "Rédaction des questions", "Vérification des pièges"];
  const courante = Math.min(Math.floor(secondes / 15), etapes.length - 1);
  return (
    <div className="carte mx-auto mt-6 max-w-md p-8 text-center">
      <Sparkles size={26} className="mx-auto text-accent" />
      <p className="mt-3 text-[17px] font-semibold">Préparation de ton test de {nomMatiere(matiere)}</p>
      <p className="mt-1 text-[13px] text-encre-3">En général moins d'une minute.</p>
      <ul className="mx-auto mt-5 grid w-fit gap-2 text-left text-[13px]">
        {etapes.map((e, i) => (
          <li key={e} className={`flex items-center gap-2 ${i > courante ? "text-encre-3" : ""}`}>
            {i < courante ? <Check size={14} className="text-vert" /> : i === courante ? <Loader2 size={14} className="animate-spin text-accent" /> : <span className="size-3.5" />}
            {e}
          </li>
        ))}
      </ul>
      <button className="btn btn-fantome mt-6 text-encre-3" onClick={annuler}>
        Annuler
      </button>
    </div>
  );
}

// ---------------------------------------------------------------- 3. questions

const LETTRES = ["A", "B", "C", "D"];

function Questionnaire({ test, index, repondre }: { test: Test; index: number; repondre: (choix: number | null) => void }) {
  const q = test.questions[index];

  // Clavier : 1-4 pour répondre, 0 ou ? pour « je sais pas ». Pas d'animation déclenchée par le clavier.
  useEffect(() => {
    const surTouche = (e: KeyboardEvent) => {
      if (e.ctrlKey || e.metaKey || e.altKey) return;
      const n = Number(e.key);
      if (n >= 1 && n <= 4) repondre(n - 1);
      else if (e.key === "0" || e.key === "?") repondre(null);
    };
    window.addEventListener("keydown", surTouche);
    return () => window.removeEventListener("keydown", surTouche);
  }, [repondre]);

  return (
    <div className="mx-auto max-w-3xl">
      {/* Progression : un segment par question */}
      <div className="mb-4 flex gap-1">
        {test.questions.map((_, i) => (
          <div key={i} className={`h-1.5 flex-1 rounded-full ${i < index ? "bg-encre-2" : i === index ? "bg-accent" : "bg-fond-2"}`} />
        ))}
      </div>

      {/* Nouvelle question : courte apparition (160 ms), pas de sortie animée pour ne pas faire attendre. */}
      <m.div
        key={index}
        initial={{ opacity: 0, transform: "translateY(6px)" }}
        animate={{ opacity: 1, transform: "translateY(0px)" }}
        transition={{ duration: 0.16, ease: EASE_SORTIE }}
        className="carte p-6"
      >
        <div className="flex items-center gap-2 text-[12.5px] text-encre-3">
          <span className="font-semibold text-encre-2">
            Question {index + 1}/{test.questions.length}
          </span>
          <span>·</span>
          <Matiere nom={test.matiere} />
          <span>·</span>
          <span>{q.niveau}</span>
        </div>
        <p className="mt-3 text-[20px] leading-snug font-semibold">{q.enonce}</p>
        {q.support && (
          <pre className="mt-4 rounded-md border border-bordure bg-fond/60 p-3.5 font-sans text-[13.5px] whitespace-pre-wrap text-encre-2">{q.support}</pre>
        )}

        <div className="mt-5 grid gap-2.5 sm:grid-cols-2">
          {q.choix.map((c, i) => (
            <button key={i} onClick={() => repondre(i)} className="btn h-auto min-h-14 justify-start gap-3 px-4 py-3 text-left text-[15px] font-normal">
              <span className="grid size-7 shrink-0 place-items-center rounded-md bg-fond-2 text-[13px] font-semibold text-encre-2">{LETTRES[i]}</span>
              <span className="flex-1">{c}</span>
              <span className="kbd">{i + 1}</span>
            </button>
          ))}
        </div>
        <div className="mt-4 flex justify-center">
          <button className="btn btn-fantome text-encre-3" onClick={() => repondre(null)}>
            Je sais pas <span className="kbd">0</span>
          </button>
        </div>
      </m.div>
    </div>
  );
}

// ---------------------------------------------------------------- 4. bilan

function Bilan({ test, reponses, recommencer, aller }: { test: Test; reponses: Reponse[]; recommencer: () => void; aller: (id: IdPage) => void }) {
  const { suivi } = useDonnees();
  const justes = reponses.filter((r) => r.juste).length;
  const matiere = suivi?.matieres.find((x) => x.matiere === test.matiere);
  const chapitres = [...new Set(test.questions.map((q) => q.chapitre))];

  return (
    <div className="mx-auto grid max-w-3xl gap-4">
      <section className="carte flex items-center gap-5 p-5">
        <div className="text-[40px] leading-none font-bold tabular-nums">
          {justes}
          <span className="text-[20px] font-semibold text-encre-3">/{test.questions.length}</span>
        </div>
        <div className="flex-1">
          <p className="text-[17px] font-semibold">Test de {nomMatiere(test.matiere)} terminé</p>
          <p className="text-[13px] text-encre-2">Toutes tes réponses sont enregistrées : les notions à retravailler reviendront dans tes prochaines séances.</p>
        </div>
      </section>

      {matiere && (
        <Carte titre="Ta carte après ce test" sansMarge>
          {chapitres.map((cid) => {
            const ch = matiere.chapitres[cid];
            if (!ch) return null;
            return (
              <div key={cid} className="border-b border-bordure/70 px-3.5 py-3 last:border-b-0">
                <p className="text-[13px] font-semibold">{ch.titre}</p>
                <div className="mt-2 flex flex-wrap gap-1.5">
                  {Object.entries(ch.notions).map(([nid, n]) => {
                    const etat = etatNotion(n);
                    return (
                      <span key={nid} title={LIBELLE_ETAT[etat]} className="inline-flex items-center gap-1.5 rounded-md border border-bordure bg-surface px-2 py-1 text-[12.5px]">
                        <span className="size-2 rounded-full" style={{ background: COULEUR_ETAT[etat] }} />
                        {n.intitule}
                      </span>
                    );
                  })}
                </div>
              </div>
            );
          })}
        </Carte>
      )}

      <Carte titre="Corrections" sansMarge>
        {test.questions.map((q: Question, i) => {
          const r = reponses[i];
          return (
            <div key={i} className="border-b border-bordure/70 px-3.5 py-3 last:border-b-0">
              <div className="flex items-start gap-2.5">
                {r?.juste ? <Check size={16} className="mt-0.5 shrink-0 text-vert" /> : <X size={16} className="mt-0.5 shrink-0 text-rouge" />}
                <div className="min-w-0 flex-1">
                  <p className="text-[13.5px] font-medium">{q.enonce}</p>
                  {!r?.juste && (
                    <p className="mt-1 text-[12.5px] text-encre-2">
                      Ta réponse : {r?.choix === null || r?.choix === undefined ? "je sais pas" : q.choix[r.choix]} · Bonne réponse :{" "}
                      <span className="font-semibold text-encre">{q.choix[q.bonne]}</span>
                    </p>
                  )}
                  <p className="mt-1 text-[12.5px] text-encre-3">{q.explication}</p>
                </div>
              </div>
            </div>
          );
        })}
      </Carte>

      <div className="flex gap-2">
        <button className="btn btn-primaire" onClick={recommencer}>
          <RotateCcw size={14} /> Nouveau test
        </button>
        <button className="btn" onClick={() => aller("aujourdhui")}>
          Retour à Aujourd'hui <ArrowRight size={14} />
        </button>
      </div>
    </div>
  );
}
