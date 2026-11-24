import { useCallback, useEffect, useRef, useState, type ReactNode } from "react";
import { CheckCircle2, ExternalLink, Loader2, XCircle } from "lucide-react";
import { Carte } from "../components/ui";
import { configurerPronote, connecterClaude, dansTauri, etatConfig, testerClaude, type EtatConfig } from "../api";
import { useDonnees } from "../lib/donnees";

export function Parametres() {
  const [etat, setEtat] = useState<EtatConfig | null>(null);
  const [erreur, setErreur] = useState<string | null>(null);

  const verifier = useCallback(() => {
    setErreur(null);
    if (!dansTauri()) return setErreur("Aperçu dans un navigateur : les réglages ne marchent que dans la fenêtre de l'app (run-dev.bat).");
    etatConfig().then(setEtat).catch((e) => setErreur(String(e)));
  }, []);
  useEffect(verifier, [verifier]);

  return (
    <div className="grid max-w-3xl gap-4">
      {erreur && <p className="carte p-3.5 text-[13px] text-rouge">{erreur}</p>}
      <CarteClaude />
      <CartePronote etat={etat} apresEnregistrement={verifier} />
      {etat && (
        <Carte titre="Données" sansMarge>
          <Point ok titre="Base de données locale">
            Schéma v{etat.base.version_schema} · {etat.base.tables.length} tables · partagée avec le skill de révision
            <code className="mt-0.5 block truncate text-[11.5px] text-encre-3">{etat.base.chemin}</code>
          </Point>
        </Carte>
      )}
    </div>
  );
}

/**
 * Connexion à Claude : l'app utilise Claude Code connecté à ton forfait. Le bouton ouvre une
 * fenêtre de connexion ; l'app vérifie ensuite toute seule, toutes les 3 s, que c'est fait.
 */
function CarteClaude() {
  const { claude, verifierClaude } = useDonnees();
  const [attente, setAttente] = useState(false);
  const [test, setTest] = useState<{ ok: boolean; texte: string } | "en_cours" | null>(null);
  const minuteur = useRef<number | null>(null);

  useEffect(() => () => void (minuteur.current && clearInterval(minuteur.current)), []);

  const connecter = async () => {
    await connecterClaude();
    setAttente(true);
    const debut = Date.now();
    minuteur.current = window.setInterval(async () => {
      const e = await verifierClaude();
      if (e?.connecte || Date.now() - debut > 180_000) {
        clearInterval(minuteur.current!);
        setAttente(false);
      }
    }, 3000);
  };

  return (
    <Carte titre="Claude" aide="L'app passe par Claude Code, connecté à ton forfait Claude : aucun mot de passe n'est stocké ici." sansMarge>
      <Point ok={!!claude?.installe} titre="Claude Code installé">
        {claude ? (claude.installe ? claude.version : "Introuvable : installe Claude Code (npm install -g @anthropic-ai/claude-code).") : "Vérification…"}
      </Point>
      <Point ok={!!claude?.connecte} titre="Connecté à ton compte">
        {claude?.connecte ? (
          `Oui${claude.abonnement ? ` · forfait ${claude.abonnement}` : ""}${claude.methode ? ` (${claude.methode})` : ""}`
        ) : attente ? (
          <span className="inline-flex items-center gap-1.5">
            <Loader2 size={13} className="animate-spin" /> Termine la connexion dans la fenêtre qui s'est ouverte (puis dans ton navigateur)…
          </span>
        ) : (
          "Pas encore : clique sur « Connecter Claude »."
        )}
      </Point>
      <div className="flex flex-wrap items-center gap-2 px-3.5 py-3">
        {!claude?.connecte && (
          <button className="btn btn-primaire" onClick={connecter} disabled={!claude?.installe || attente || !dansTauri()}>
            <ExternalLink size={14} /> Connecter Claude
          </button>
        )}
        <button
          className="btn"
          disabled={!claude?.connecte || test === "en_cours" || !dansTauri()}
          onClick={async () => {
            setTest("en_cours");
            setTest(await testerClaude().catch((e) => ({ ok: false, texte: String(e) })));
          }}
        >
          {test === "en_cours" ? "Réponse en cours…" : "Tester"}
        </button>
        <button className="btn btn-fantome" onClick={() => void verifierClaude()}>
          Revérifier
        </button>
        {test && test !== "en_cours" && <p className={`text-[13px] ${test.ok ? "text-vert" : "text-rouge"}`}>{test.texte}</p>}
      </div>
    </Carte>
  );
}

function CartePronote({ etat, apresEnregistrement }: { etat: EtatConfig | null; apresEnregistrement: () => void }) {
  const p = etat?.pronote;
  const [url, setUrl] = useState("");
  const [identifiant, setIdentifiant] = useState("");
  const [ent, setEnt] = useState("atrium_educonnect");
  const [motDePasse, setMotDePasse] = useState("");
  const [message, setMessage] = useState<{ ok: boolean; texte: string } | null>(null);

  useEffect(() => {
    if (!p) return;
    setUrl(p.url ?? "");
    setIdentifiant(p.identifiant ?? "");
    setEnt(p.mode_connexion ?? "atrium_educonnect");
  }, [p]);

  const enregistrer = async () => {
    setMessage(null);
    try {
      await configurerPronote({ url, identifiant, ent, motDePasse: motDePasse || null });
      setMotDePasse("");
      setMessage({ ok: true, texte: "Enregistré. Le mot de passe est dans le coffre-fort Windows." });
      apresEnregistrement();
    } catch (e) {
      setMessage({ ok: false, texte: String(e) });
    }
  };

  const champ = "h-8 w-full rounded-md border border-bordure bg-surface px-2.5 text-[13px] outline-none focus:border-bordure-forte";

  return (
    <Carte titre="Pronote" aide="Connexion via l'ENT Atrium. Le mot de passe va directement dans le coffre-fort Windows." sansMarge>
      {p && (
        <Point ok={p.config_trouvee && p.mot_de_passe_au_coffre} titre="État">
          {p.config_trouvee ? (p.mot_de_passe_au_coffre ? "Configuré" : "Mot de passe manquant") : "Pas encore configuré"}
          {p.derniere_synchro && ` · dernière synchro le ${new Date(p.derniere_synchro).toLocaleString("fr-FR")} (${p.nb_cours} cours)`}
        </Point>
      )}
      <div className="grid gap-3 px-3.5 py-3">
        <Champ libelle="Adresse Pronote" aide="Ouvre Pronote depuis Atrium et copie l'adresse de la page (…index-education.net/pronote/eleve.html).">
          <input className={champ} value={url} onChange={(e) => setUrl(e.target.value)} placeholder="https://0130xxxx.index-education.net/pronote/eleve.html" />
        </Champ>
        <Champ libelle="Connexion à Atrium">
          <select className={champ} value={ent} onChange={(e) => setEnt(e.target.value)}>
            <option value="atrium_educonnect">Avec ÉduConnect (bouton « élève »)</option>
            <option value="atrium_sud">Avec un compte Atrium</option>
          </select>
        </Champ>
        <div className="grid gap-3 sm:grid-cols-2">
          <Champ libelle={`Identifiant ${ent === "atrium_sud" ? "Atrium" : "ÉduConnect"}`}>
            <input className={champ} value={identifiant} onChange={(e) => setIdentifiant(e.target.value)} autoComplete="username" />
          </Champ>
          <Champ libelle="Mot de passe" aide={p?.mot_de_passe_au_coffre ? "Laisse vide pour garder celui enregistré." : undefined}>
            <input className={champ} type="password" value={motDePasse} onChange={(e) => setMotDePasse(e.target.value)} autoComplete="current-password" />
          </Champ>
        </div>
        <div className="flex items-center gap-3">
          <button className="btn btn-primaire" onClick={enregistrer} disabled={!dansTauri()}>
            Enregistrer
          </button>
          {message && <p className={`text-[13px] ${message.ok ? "text-vert" : "text-rouge"}`}>{message.texte}</p>}
        </div>
      </div>
    </Carte>
  );
}

function Champ({ libelle, aide, children }: { libelle: string; aide?: string; children: ReactNode }) {
  return (
    <label className="grid gap-1">
      <span className="text-[12.5px] font-medium text-encre-2">{libelle}</span>
      {children}
      {aide && <span className="text-[11.5px] text-encre-3">{aide}</span>}
    </label>
  );
}

function Point({ ok, titre, children }: { ok: boolean; titre: string; children: ReactNode }) {
  return (
    <div className="flex gap-3 border-b border-bordure/70 px-3.5 py-3 last:border-b-0">
      {ok ? <CheckCircle2 size={16} className="mt-0.5 shrink-0 text-vert" /> : <XCircle size={16} className="mt-0.5 shrink-0 text-rouge" />}
      <div className="min-w-0">
        <p className="text-[13.5px] font-medium">{titre}</p>
        <div className="text-[12.5px] text-encre-2">{children}</div>
      </div>
    </div>
  );
}
