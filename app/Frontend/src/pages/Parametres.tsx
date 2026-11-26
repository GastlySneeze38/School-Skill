import { useCallback, useEffect, useState, type ReactNode } from "react";
import { CheckCircle2, Loader2, XCircle } from "lucide-react";
import { Carte } from "../components/ui";
import { configurerPronote, dansTauri, etatConfig, testerIa, type EtatConfig } from "../api";
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
      <CarteIa />
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
 * L'IA locale : llama.cpp et les modèles sont sur l'ordinateur (dossier ~/.revisions/ia). Rien à connecter :
 * le modèle se charge tout seul quand il sert, et se décharge après 10 minutes sans usage.
 */
function CarteIa() {
  const { ia, verifierIa } = useDonnees();
  const [test, setTest] = useState<{ ok: boolean; texte: string } | "en_cours" | null>(null);

  return (
    <Carte titre="IA locale" aide="Tout tourne sur ton ordinateur : tes cours ne partent nulle part. Le modèle se charge quand il sert et libère la carte graphique après 10 minutes sans usage." sansMarge>
      <Point ok={!!ia?.installe} titre="llama.cpp et modèles">
        {ia ? (ia.installe ? <>Tout est là<code className="mt-0.5 block truncate text-[11.5px] text-encre-3">{ia.dossier}</code></> : `Manquant : ${ia.manquants.join(", ")}`) : "Vérification…"}
      </Point>
      <Point ok={!!ia?.installe} titre={`Modèle d'écriture : ${ia?.modele ?? "…"}`}>
        {ia?.ecriture === "pret" ? "Chargé sur la carte graphique, prêt à écrire." : "En veille : il se charge en quelques secondes dès qu'il sert."}
      </Point>
      <Point ok={!!ia?.installe} titre="Recherche dans tes cours">
        {ia?.recherche === "pret" ? "Prête (sur le processeur, sans carte graphique)." : "Démarre à la première recherche."}
      </Point>
      <div className="flex flex-wrap items-center gap-2 px-3.5 py-3">
        <button
          className="btn"
          disabled={!ia?.installe || test === "en_cours" || !dansTauri()}
          onClick={async () => {
            setTest("en_cours");
            setTest(await testerIa().then((t) => ({ ok: true, texte: t })).catch((e) => ({ ok: false, texte: String(e) })));
            void verifierIa();
          }}
        >
          {test === "en_cours" ? <><Loader2 size={14} className="animate-spin" /> Chargement du modèle…</> : "Tester"}
        </button>
        <button className="btn btn-fantome" onClick={() => void verifierIa()}>
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
