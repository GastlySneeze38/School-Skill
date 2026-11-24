//! Claude passe par Claude Code, installé sur le PC et connecté au forfait de l'élève :
//! l'app ne stocke aucun identifiant Claude.

use std::io::Write;
use std::process::Stdio;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::outils;

#[derive(Serialize)]
pub struct EtatClaude {
    pub installe: bool,
    pub connecte: bool,
    pub version: Option<String>,
    /// « claude.ai » (forfait), « console » (clé API)…
    pub methode: Option<String>,
    pub abonnement: Option<String>,
    pub detail: Option<String>,
}

pub fn etat() -> EtatClaude {
    let v = outils::lancer(outils::commande_claude(&["--version"]), None);
    if !v.ok {
        return EtatClaude {
            installe: false,
            connecte: false,
            version: None,
            methode: None,
            abonnement: None,
            detail: Some("Claude Code n'est pas installé (ou pas trouvé dans le PATH).".into()),
        };
    }
    let s = outils::lancer(outils::commande_claude(&["auth", "status"]), None);
    let j: Value = serde_json::from_str(&s.sortie).unwrap_or(Value::Null);
    EtatClaude {
        installe: true,
        connecte: j["loggedIn"].as_bool().unwrap_or(false),
        version: v.sortie.lines().next().map(String::from),
        methode: j["authMethod"].as_str().map(String::from),
        abonnement: j["subscriptionType"].as_str().map(String::from),
        detail: (!s.ok && j.is_null()).then(|| s.erreurs.chars().take(300).collect()),
    }
}

/// Ouvre une fenêtre de terminal qui lance la connexion au compte Claude (navigateur).
pub fn ouvrir_connexion() -> Result<()> {
    outils::terminal_claude("Connexion à Claude", "auth login")?;
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Reponse {
    pub ok: bool,
    pub texte: String,
}

/// Envoie un prompt à Claude Code en mode non interactif et renvoie le texte de la réponse.
/// `outils` : outils intégrés autorisés ("" = aucun). Le prompt passe par l'entrée standard.
pub fn demander(prompt: &str, outils_autorises: &str, dossier: Option<&std::path::Path>) -> Reponse {
    let mut c = outils::commande_claude(&[
        "-p",
        "--output-format",
        "json",
        "--model",
        "sonnet",
        "--tools",
        outils_autorises,
        "--no-session-persistence",
        "--disable-slash-commands",
    ]);
    if let Some(d) = dossier {
        c.current_dir(d);
    }
    c.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut enfant = match c.spawn() {
        Ok(e) => e,
        Err(e) => return Reponse { ok: false, texte: format!("Claude Code introuvable : {e}") },
    };
    if let Some(mut entree) = enfant.stdin.take() {
        let _ = entree.write_all(prompt.as_bytes());
    } // l'entrée est fermée ici : Claude sait que le prompt est complet
    let sortie = match enfant.wait_with_output() {
        Ok(s) => s,
        Err(e) => return Reponse { ok: false, texte: e.to_string() },
    };
    let brut = String::from_utf8_lossy(&sortie.stdout);
    // --output-format json : { "is_error": bool, "result": "<texte>", ... }
    match serde_json::from_str::<Value>(&brut) {
        Ok(v) => {
            let texte = v["result"].as_str().unwrap_or_default().to_string();
            let erreur = !sortie.status.success() || v["is_error"].as_bool().unwrap_or(false);
            Reponse { ok: !erreur, texte: if erreur { traduire_erreur(&texte) } else { texte } }
        }
        Err(_) => Reponse {
            ok: false,
            texte: traduire_erreur(&format!("{} {}", brut.trim(), String::from_utf8_lossy(&sortie.stderr).trim())),
        },
    }
}

/// Messages d'erreur de Claude Code → explication actionnable.
fn traduire_erreur(t: &str) -> String {
    let bas = t.to_lowercase();
    if bas.contains("authenticate") || bas.contains("api key") || bas.contains("login") || bas.contains("oauth") {
        "Claude n'est pas connecté à ton compte : clique sur « Connecter Claude » dans Paramètres.".into()
    } else if bas.contains("rate") || bas.contains("limit") {
        format!("Limite d'utilisation de ton forfait atteinte pour le moment. ({t})")
    } else {
        t.chars().take(500).collect()
    }
}

/// Extrait le premier objet JSON d'une réponse (Claude l'entoure parfois de ```json … ```).
pub fn extraire_json(texte: &str) -> Result<Value> {
    let debut = texte.find('{').context("la réponse de Claude ne contient pas de JSON")?;
    let fin = texte.rfind('}').context("JSON incomplet dans la réponse de Claude")?;
    if fin <= debut {
        bail!("JSON incomplet dans la réponse de Claude");
    }
    Ok(serde_json::from_str(&texte[debut..=fin]).context("JSON illisible dans la réponse de Claude")?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_entoure() {
        let v = extraire_json("Voici :\n```json\n{\"a\": {\"b\": 1}}\n```\nBonne chance").unwrap();
        assert_eq!(v["a"]["b"], 1);
    }

    #[test]
    fn erreurs_traduites() {
        assert!(traduire_erreur("Failed to authenticate: OAuth session expired").contains("Connecter Claude"));
    }
}
