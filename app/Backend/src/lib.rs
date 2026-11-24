mod claude;
mod cli;
mod config;
mod db;
mod moyennes;
mod outils;
mod pronote;
mod revision_jour;
mod seance;
mod suivi;

use std::path::PathBuf;
use std::sync::Mutex;

use rusqlite::Connection;
use serde::Serialize;

/// La connexion SQLite, partagée par les commandes rapides. Les tâches longues (séance Claude)
/// ouvrent leur propre connexion pour ne pas bloquer l'interface pendant qu'elles attendent.
struct Base {
    conn: Mutex<Connection>,
    chemin: PathBuf,
}

type R<T> = Result<T, String>;

fn erreur(e: impl std::fmt::Display) -> String {
    e.to_string()
}

fn en_tache<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> impl std::future::Future<Output = R<T>> {
    async move { tauri::async_runtime::spawn_blocking(f).await.map_err(erreur) }
}

// ---------------------------------------------------------------- configuration

#[derive(Serialize)]
struct EtatBase {
    chemin: String,
    version_schema: usize,
    tables: Vec<String>,
}

#[derive(Serialize)]
struct EtatPronote {
    config_trouvee: bool,
    url: Option<String>,
    identifiant: Option<String>,
    mode_connexion: Option<String>,
    mot_de_passe_au_coffre: bool,
    derniere_synchro: Option<String>,
    nb_cours: Option<usize>,
    nb_evals_a_venir: Option<usize>,
}

#[derive(Serialize)]
struct EtatConfig {
    base: EtatBase,
    pronote: EtatPronote,
    claude: claude::EtatClaude,
}

#[tauri::command]
async fn etat_config(base: tauri::State<'_, Base>) -> R<EtatConfig> {
    let etat_base = {
        let conn = base.conn.lock().map_err(erreur)?;
        EtatBase {
            chemin: base.chemin.display().to_string(),
            version_schema: db::version_schema(&conn).map_err(erreur)?,
            tables: db::tables(&conn).map_err(erreur)?,
        }
    };
    let cfg = config::lire_config();
    let sync = lire_pronote();
    let compter = |cle: &str| sync.as_ref().and_then(|s| s[cle].as_array()).map(|a| a.len());
    let pronote = EtatPronote {
        config_trouvee: cfg.is_some(),
        url: cfg.as_ref().and_then(|c| c.pronote_url.clone()),
        identifiant: cfg.as_ref().and_then(|c| c.identifiant.clone()),
        mode_connexion: cfg.as_ref().map(|c| c.ent.clone().unwrap_or_else(|| "atrium_educonnect".into())),
        mot_de_passe_au_coffre: cfg.as_ref().is_some_and(config::mot_de_passe_present),
        derniere_synchro: sync.as_ref().and_then(|s| s["synchronise_le"].as_str().map(String::from)),
        nb_cours: compter("cours"),
        nb_evals_a_venir: compter("controles_a_venir"),
    };
    // `claude auth status` lance un programme : hors du fil principal.
    let claude = en_tache(claude::etat).await?;
    Ok(EtatConfig { base: etat_base, pronote, claude })
}

/// Enregistre la connexion Pronote. Le mot de passe va directement dans le coffre Windows ;
/// vide = garder celui déjà enregistré.
#[tauri::command]
fn configurer_pronote(url: String, identifiant: String, ent: String, mot_de_passe: Option<String>) -> R<()> {
    let url = url.trim().split('?').next().unwrap_or_default().to_string();
    if !url.contains("index-education.net") {
        return Err("Ça ne ressemble pas à une adresse Pronote (…index-education.net/pronote/eleve.html).".into());
    }
    if identifiant.trim().is_empty() {
        return Err("Identifiant manquant.".into());
    }
    let mut c = config::lire_config().unwrap_or_default();
    c.pronote_url = Some(url);
    c.identifiant = Some(identifiant.trim().to_string());
    c.ent = Some(ent);
    config::ecrire_config(&c).map_err(erreur)?;
    if let Some(m) = mot_de_passe.filter(|m| !m.is_empty()) {
        config::enregistrer_mot_de_passe(identifiant.trim(), &m).map_err(erreur)?;
    } else if !config::mot_de_passe_present(&c) {
        return Err("Mot de passe manquant.".into());
    }
    Ok(())
}

// ---------------------------------------------------------------- Pronote

#[tauri::command]
async fn synchroniser_pronote(base: tauri::State<'_, Base>) -> R<pronote::Resume> {
    let r = pronote::synchroniser(false, true).await;
    if r.ok && !r.ignoree {
        let conn = base.conn.lock().map_err(erreur)?;
        photographier_moyennes(&conn);
        drop(conn);
        // Nouvelles leçons récupérées : la séance du jour peut maintenant se préparer.
        revision_jour::lancer_preparation(base.chemin.clone());
    }
    Ok(r)
}

/// Garde la photo des moyennes de la dernière synchro (progression au fil du trimestre).
fn photographier_moyennes(conn: &Connection) {
    if let Some(sync) = lire_pronote() {
        if let Err(e) = moyennes::enregistrer_instantane(conn, &sync) {
            eprintln!("Historique des moyennes non enregistré : {e}");
        }
    }
}

fn lire_json(chemin: PathBuf) -> Option<serde_json::Value> {
    serde_json::from_str(&std::fs::read_to_string(chemin).ok()?).ok()
}

/// Dernière synchro Pronote (cours, devoirs, contrôles, notes, infos), ou `null`.
#[tauri::command]
fn lire_pronote() -> Option<serde_json::Value> {
    lire_json(config::dossier_donnees().join("pronote").join("sync.json"))
}

// ---------------------------------------------------------------- suivi

#[tauri::command]
fn lire_suivi(base: tauri::State<'_, Base>) -> R<serde_json::Value> {
    let conn = base.conn.lock().map_err(erreur)?;
    suivi::exporter(&conn).map_err(erreur)
}

#[tauri::command]
fn contenu_matieres(base: tauri::State<'_, Base>) -> R<Vec<suivi::ContenuMatiere>> {
    let conn = base.conn.lock().map_err(erreur)?;
    suivi::contenu(&conn, &config::dossier_donnees()).map_err(erreur)
}

#[tauri::command]
fn carte(base: tauri::State<'_, Base>, matiere: Option<String>) -> R<Vec<suivi::LigneCarte>> {
    let conn = base.conn.lock().map_err(erreur)?;
    suivi::carte(&conn, matiere.as_deref(), None).map_err(erreur)
}

// ---------------------------------------------------------------- séance du jour (écran principal)

#[tauri::command]
fn etat_jour(base: tauri::State<'_, Base>) -> R<revision_jour::EtatJour> {
    let conn = base.conn.lock().map_err(erreur)?;
    revision_jour::etat(&conn).map_err(erreur)
}

/// Prépare la séance du jour en arrière-plan si besoin (sans effet si c'est déjà fait ou en cours).
#[tauri::command]
fn preparer_jour(base: tauri::State<'_, Base>) -> bool {
    revision_jour::lancer_preparation(base.chemin.clone())
}

/// L'élève a choisi une matière pas encore prête : elle se prépare tout de suite, en priorité.
#[tauri::command]
fn preparer_matiere(base: tauri::State<'_, Base>, matiere: String) -> bool {
    revision_jour::preparer_matiere(base.chemin.clone(), matiere)
}

/// Prépare la série suivante de questions de révision (en arrière-plan).
#[tauri::command]
fn suite_jour(base: tauri::State<'_, Base>) -> bool {
    revision_jour::lancer_suite(base.chemin.clone())
}

#[tauri::command]
fn repondre_jour(base: tauri::State<'_, Base>, idx: usize, reponse: serde_json::Value) -> R<revision_jour::ReponseJour> {
    let conn = base.conn.lock().map_err(erreur)?;
    revision_jour::repondre(&conn, base.chemin.clone(), idx, reponse).map_err(erreur)
}

// ---------------------------------------------------------------- moyennes et progression

#[tauri::command]
fn lire_moyennes(base: tauri::State<'_, Base>) -> R<Option<moyennes::Moyennes>> {
    let conn = base.conn.lock().map_err(erreur)?;
    moyennes::lire(&conn).map_err(erreur)
}

#[tauri::command]
fn derniere_analyse(base: tauri::State<'_, Base>) -> R<Option<moyennes::Analyse>> {
    let conn = base.conn.lock().map_err(erreur)?;
    moyennes::derniere_analyse(&conn).map_err(erreur)
}

/// Claude estime où gagner des points (peut prendre une minute : connexion à part).
#[tauri::command]
async fn analyser_moyennes(base: tauri::State<'_, Base>) -> R<moyennes::Analyse> {
    let chemin = base.chemin.clone();
    en_tache(move || moyennes::analyser(&db::ouvrir(&chemin)?)).await?.map_err(erreur)
}

#[tauri::command]
fn stats_revision(base: tauri::State<'_, Base>) -> R<suivi::StatsRevision> {
    let conn = base.conn.lock().map_err(erreur)?;
    suivi::stats_revision(&conn).map_err(erreur)
}

// ---------------------------------------------------------------- Claude et séances

#[tauri::command]
async fn etat_claude() -> R<claude::EtatClaude> {
    en_tache(claude::etat).await
}

#[tauri::command]
fn connecter_claude() -> R<()> {
    claude::ouvrir_connexion().map_err(erreur)
}

#[tauri::command]
async fn tester_claude() -> R<claude::Reponse> {
    en_tache(|| claude::demander("Réponds uniquement par : Claude est prêt pour tes révisions.", "", None)).await
}

#[tauri::command]
async fn preparer_test(base: tauri::State<'_, Base>, matiere: String, nb_questions: Option<usize>) -> R<seance::Test> {
    let chemin = base.chemin.clone();
    en_tache(move || {
        let conn = db::ouvrir(&chemin)?;
        seance::preparer_test(&conn, &matiere, nb_questions.unwrap_or(8))
    })
    .await?
    .map_err(erreur)
}

#[tauri::command]
fn enregistrer_reponse(base: tauri::State<'_, Base>, matiere: String, question: seance::Question, choix: Option<usize>) -> R<bool> {
    let conn = base.conn.lock().map_err(erreur)?;
    seance::enregistrer_reponse(&conn, &matiere, &question, choix).map_err(erreur)
}

// ---------------------------------------------------------------- démarrage

pub fn run() {
    // Avec des arguments, le même programme sert de ligne de commande (pour le skill).
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !args.is_empty() {
        attacher_console();
        std::process::exit(cli::executer(&args));
    }

    let dossier = config::dossier_donnees();
    let chemin = db::chemin_base(&dossier);
    let conn = db::ouvrir(&chemin).unwrap_or_else(|e| panic!("Impossible d'ouvrir la base {} : {e}", chemin.display()));
    if let Err(e) = suivi::importer_json_si_vide(&conn, &dossier) {
        eprintln!("Import des anciennes données JSON impossible : {e}");
    }
    photographier_moyennes(&conn);
    // La séance du jour se prépare dès l'ouverture : quand l'élève arrive, elle est prête.
    revision_jour::lancer_preparation(chemin.clone());

    tauri::Builder::default()
        .manage(Base { conn: Mutex::new(conn), chemin })
        .invoke_handler(tauri::generate_handler![
            etat_config,
            configurer_pronote,
            synchroniser_pronote,
            lire_pronote,
            lire_suivi,
            contenu_matieres,
            carte,
            etat_claude,
            connecter_claude,
            tester_claude,
            preparer_test,
            enregistrer_reponse,
            lire_moyennes,
            derniere_analyse,
            analyser_moyennes,
            stats_revision,
            etat_jour,
            preparer_jour,
            preparer_matiere,
            suite_jour,
            repondre_jour,
        ])
        .run(tauri::generate_context!())
        .expect("erreur au lancement de l'application");
}

/// En version publiée, l'app n'a pas de console : en ligne de commande, on se rattache
/// à celle qui l'a lancée pour que le texte s'affiche.
fn attacher_console() {
    #[cfg(all(windows, not(debug_assertions)))]
    unsafe {
        windows_sys::Win32::System::Console::AttachConsole(windows_sys::Win32::System::Console::ATTACH_PARENT_PROCESS);
    }
}
