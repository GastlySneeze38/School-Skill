//! Où sont les choses : la config (`~/.revisions/config.json`), le dossier de données,
//! le mot de passe Pronote (coffre-fort Windows) et les fichiers du skill.

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ConfigRevisions {
    pub pronote_url: Option<String>,
    pub identifiant: Option<String>,
    /// "atrium_educonnect" (défaut) ou "atrium_sud" (compte Atrium).
    pub ent: Option<String>,
    pub dossier_donnees: Option<String>,
}

/// Nom de l'entrée dans le Gestionnaire d'identification Windows. C'est celui qu'utilisait
/// l'ancien script Python : le mot de passe déjà enregistré reste valable.
const COFFRE: &str = "revisions-pronote";

pub fn dossier_utilisateur() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn fichier_config() -> PathBuf {
    dossier_utilisateur().join(".revisions").join("config.json")
}

/// `None` si l'app n'a jamais été configurée.
pub fn lire_config() -> Option<ConfigRevisions> {
    let texte = std::fs::read_to_string(fichier_config()).ok()?;
    serde_json::from_str(&texte).ok()
}

pub fn ecrire_config(c: &ConfigRevisions) -> Result<()> {
    let f = fichier_config();
    std::fs::create_dir_all(f.parent().unwrap())?;
    std::fs::write(f, serde_json::to_string_pretty(c)?)?;
    Ok(())
}

fn entree_coffre(identifiant: &str) -> Result<keyring::Entry> {
    Ok(keyring::Entry::new_with_target(COFFRE, COFFRE, identifiant)?)
}

pub fn mot_de_passe_pronote(c: &ConfigRevisions) -> Result<String> {
    let id = c.identifiant.as_deref().context("identifiant Pronote absent de la config")?;
    entree_coffre(id)?
        .get_password()
        .context("mot de passe Pronote absent du coffre Windows : renseigne-le dans Paramètres")
}

pub fn enregistrer_mot_de_passe(identifiant: &str, mot_de_passe: &str) -> Result<()> {
    Ok(entree_coffre(identifiant)?.set_password(mot_de_passe)?)
}

pub fn mot_de_passe_present(c: &ConfigRevisions) -> bool {
    mot_de_passe_pronote(c).is_ok()
}

/// Racine du projet « skill école » (le dossier qui contient `app/`).
pub fn racine_projet() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..")
}

/// Même dossier de données que le skill, pour que le chat et l'app partagent tout.
pub fn dossier_donnees() -> PathBuf {
    lire_config()
        .and_then(|c| c.dossier_donnees)
        .map(PathBuf::from)
        .unwrap_or_else(|| racine_projet().join("donnees"))
}

/// Dossier du skill `revisions` (ses instructions servent aussi à l'app pour les séances).
pub fn dossier_skill() -> PathBuf {
    racine_projet().join(".claude").join("skills").join("revisions")
}
