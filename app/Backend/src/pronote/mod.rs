//! Client Pronote en Rust (connexion Atrium + ÉduConnect, sans Python).

mod atrium;
mod crypto;
mod donnees;
mod session;

use std::path::Path;

use anyhow::{Result, bail};
use chrono::{Local, NaiveDateTime};
use serde::Serialize;

use crate::config::{self, ConfigRevisions};


/// Pronote suspend l'adresse IP si on se connecte trop souvent.
const MINUTES_ENTRE_CONNEXIONS: i64 = 30;

#[derive(Serialize)]
pub struct Resume {
    pub ok: bool,
    /// Pas de connexion : la dernière synchro est trop récente.
    pub ignoree: bool,
    pub message: String,
    pub nb_cours: usize,
    pub nb_devoirs_a_faire: usize,
    pub nb_evals: usize,
    pub nb_notes: usize,
    pub alertes: Vec<String>,
}

impl Resume {
    fn erreur(message: String) -> Self {
        Resume { ok: false, ignoree: false, message, nb_cours: 0, nb_devoirs_a_faire: 0, nb_evals: 0, nb_notes: 0, alertes: vec![] }
    }
}

/// Synchronise tout dans `<dossier_donnees>/pronote/sync.json`.
/// Ne se reconnecte pas si la dernière connexion date de moins de 30 min (sauf `forcer`).
pub async fn synchroniser(forcer: bool, telecharger: bool) -> Resume {
    match synchroniser_interne(forcer, telecharger).await {
        Ok(r) => r,
        Err(e) => Resume::erreur(e.to_string()),
    }
}

async fn synchroniser_interne(forcer: bool, telecharger: bool) -> Result<Resume> {
    let Some(cfg) = config::lire_config() else {
        bail!("Pronote n'est pas encore configuré : renseigne la connexion dans Paramètres.")
    };
    let mot_de_passe = config::mot_de_passe_pronote(&cfg)?;
    let dossier = config::dossier_donnees();
    let racine = dossier.join("pronote");
    std::fs::create_dir_all(&racine)?;

    let fichier_derniere = racine.join("derniere_connexion.txt");
    if !forcer {
        if let Some(d) = std::fs::read_to_string(&fichier_derniere)
            .ok()
            .and_then(|t| NaiveDateTime::parse_from_str(t.trim(), "%Y-%m-%dT%H:%M:%S").ok())
        {
            let ecart = Local::now().naive_local() - d;
            if ecart.num_minutes() < MINUTES_ENTRE_CONNEXIONS {
                return Ok(Resume {
                    ok: true,
                    ignoree: true,
                    message: format!(
                        "Dernière synchro il y a {} min : on garde les données actuelles (Pronote bloque les connexions trop fréquentes).",
                        ecart.num_minutes()
                    ),
                    nb_cours: 0,
                    nb_devoirs_a_faire: 0,
                    nb_evals: 0,
                    nb_notes: 0,
                    alertes: vec![],
                });
            }
        }
    }
    // Noté AVANT la tentative : même une connexion ratée compte pour Pronote.
    std::fs::write(&fichier_derniere, Local::now().format("%Y-%m-%dT%H:%M:%S").to_string())?;

    let donnees = recuperer(&cfg, &mot_de_passe, &dossier, telecharger).await?;
    std::fs::write(racine.join("sync.json"), serde_json::to_string_pretty(&donnees)?)?;

    let auj = Local::now().date_naive().to_string();
    let compte = |cle: &str| donnees[cle].as_array().map_or(0, |a| a.len());
    let a_faire = donnees["devoirs"]
        .as_array()
        .map_or(0, |a| a.iter().filter(|d| d["fait"] == false && d["pour_le"].as_str() >= Some(auj.as_str())).count());
    let alertes: Vec<String> =
        donnees["alertes"].as_array().into_iter().flatten().filter_map(|a| a.as_str().map(String::from)).collect();
    Ok(Resume {
        ok: true,
        ignoree: false,
        message: format!(
            "Synchro Pronote OK : {} cours, {a_faire} devoirs à faire, {} évaluation(s) repérée(s), {} notes",
            compte("cours"),
            compte("controles_a_venir"),
            compte("notes")
        ),
        nb_cours: compte("cours"),
        nb_devoirs_a_faire: a_faire,
        nb_evals: compte("controles_a_venir"),
        nb_notes: compte("notes"),
        alertes,
    })
}

async fn recuperer(cfg: &ConfigRevisions, mot_de_passe: &str, dossier: &Path, telecharger: bool) -> Result<serde_json::Value> {
    let url = cfg.pronote_url.clone().unwrap_or_default();
    let identifiant = cfg.identifiant.clone().unwrap_or_default();
    let http = atrium::client_http()?;
    match cfg.ent.as_deref() {
        Some("atrium_sud") => atrium::connexion_compte_atrium(&http, &identifiant, mot_de_passe).await?,
        _ => atrium::connexion_educonnect(&http, &identifiant, mot_de_passe).await?,
    }
    let mut s = session::Session::ouvrir(http, &url).await?;
    donnees::recuperer(&mut s, &donnees::Options { dossier, telecharger }).await
}
