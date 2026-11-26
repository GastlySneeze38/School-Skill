//! La séance du jour, l'écran principal de l'app : comprendre la leçon du jour, puis réviser.
//!
//! - Préparée en arrière-plan dès l'ouverture de l'app (et après une synchro) : l'élève n'attend pas.
//! - L'IA locale écrit les questions une par une (voir `generation`) : la première arrive en quelques
//!   secondes, les suivantes pendant que l'élève répond.
//! - Chaque question a le format qui rend la réponse la plus naturelle (QCM, vrai/faux, réponse
//!   courte, calcul, développement) ; QCM, vrai/faux et calcul sont corrigés sur place, les réponses
//!   rédigées par l'IA locale en arrière-plan pendant que l'élève continue.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{config, connaissances, db, generation, ia, suivi};

fn jour() -> String {
    Local::now().date_naive().to_string()
}

fn maintenant() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

/// Poids d'une question dans le % de compréhension : appliquer compte plus que réciter.
fn poids(niveau: &str) -> f64 {
    match niveau {
        "appliquer" => 2.0,
        "comprendre" => 1.5,
        _ => 1.0,
    }
}

// ---------------------------------------------------------------- types

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct QuestionJour {
    /// « comprendre » (leçon du jour), « reviser », « echauffement ».
    #[serde(default)]
    pub phase: String,
    #[serde(default)]
    pub matiere: String,
    /// Titre de la leçon visée (phase « comprendre »).
    #[serde(default)]
    pub lecon: Option<String>,
    pub chapitre: String,
    pub notion: String,
    pub niveau: String,
    #[serde(default = "qcm")]
    pub format: String,
    pub enonce: String,
    #[serde(default)]
    pub support: Option<String>,
    #[serde(default)]
    pub choix: Vec<String>,
    #[serde(default)]
    pub bonne: Option<usize>,
    #[serde(default)]
    pub reponses_acceptees: Vec<String>,
    #[serde(default)]
    pub valeur: Option<f64>,
    #[serde(default)]
    pub tolerance: Option<f64>,
    #[serde(default)]
    pub unite: Option<String>,
    /// Développement : les parties du plan (une zone de texte par partie).
    #[serde(default)]
    pub plan: Vec<String>,
    /// Développement / réponse courte : les points qu'une bonne réponse doit contenir.
    #[serde(default)]
    pub attendus: Vec<String>,
    #[serde(default)]
    pub erreurs: Vec<Option<String>>,
    pub explication: String,
    /// La connaissance du cours indispensable pour répondre (montrée avec la correction).
    #[serde(default)]
    pub connaissance: Option<String>,
}

fn qcm() -> String {
    "qcm".into()
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Lecon {
    pub matiere: String,
    pub titre: String,
    pub description: String,
    pub debut: String,
    pub documents: Vec<String>,
}

#[derive(Serialize, Clone)]
pub struct ReponseJour {
    pub idx: usize,
    /// juste, partiel, faux, en_correction (l'IA corrige en arrière-plan).
    pub statut: String,
    pub score: f64,
    pub reponse: Value,
    pub retour: Option<String>,
}

/// Une matière du jour : ses leçons (avec ou sans contenu écrit par le prof).
#[derive(Serialize, Deserialize, Clone)]
pub struct MatiereJour {
    pub matiere: String,
    /// Heure du premier cours (« 08h00 »).
    pub heure: String,
    pub lecons: Vec<Lecon>,
    /// Le prof a écrit un contenu pour au moins une leçon.
    pub a_contenu: bool,
}

/// État d'une matière dans la séance du jour (pour l'écran de choix).
#[derive(Serialize)]
pub struct EtatMatiere {
    #[serde(flatten)]
    pub matiere: MatiereJour,
    pub nb_questions: usize,
    pub nb_repondues: usize,
    pub comprehension: Option<f64>,
    pub en_preparation: bool,
}

#[derive(Serialize)]
pub struct EtatJour {
    pub jour: String,
    /// Date des cours utilisés (aujourd'hui, ou le dernier jour de cours s'il n'y en a pas eu aujourd'hui).
    pub date_lecons: Option<String>,
    /// Les matières du jour, dans l'ordre de l'emploi du temps.
    pub matieres: Vec<EtatMatiere>,
    pub questions: Vec<QuestionJour>,
    pub reponses: Vec<ReponseJour>,
    pub suite_en_cours: bool,
    pub erreur: Option<String>,
}

// ---------------------------------------------------------------- tâches de fond

/// Tâches en cours (« matiere-<slug> », « suite », « correction-N ») : jamais deux fois la même.
static EN_COURS: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(|| Mutex::new(HashSet::new()));
/// Plusieurs préparations écrivent la liste des questions : une seule à la fois.
static ECRITURE: Mutex<()> = Mutex::new(());

fn reserver(cle: &str) -> bool {
    EN_COURS.lock().unwrap().insert(cle.to_string())
}

fn liberer(cle: &str) {
    EN_COURS.lock().unwrap().remove(cle);
}

fn en_cours(cle: &str) -> bool {
    EN_COURS.lock().unwrap().contains(cle)
}

fn noter_erreur(c: &Connection, e: &str) {
    let _ = c.execute("UPDATE seances_jour SET erreur = ?2, maj = ?3 WHERE jour = ?1", params![jour(), e, maintenant()]);
}

/// Lance `f` dans un fil à part, avec sa propre connexion, si la tâche `cle` n'est pas déjà en cours.
fn tache(cle: String, chemin: PathBuf, f: impl FnOnce(&Connection) -> Result<()> + Send + 'static) -> bool {
    if !reserver(&cle) {
        return false;
    }
    std::thread::spawn(move || {
        match db::ouvrir(&chemin) {
            Ok(c) => {
                if let Err(e) = f(&c) {
                    eprintln!("séance du jour ({cle}) : {e:#}");
                    noter_erreur(&c, &e.to_string());
                }
            }
            Err(e) => eprintln!("séance du jour ({cle}) : {e}"),
        }
        liberer(&cle);
    });
    true
}

fn cle_matiere(m: &str) -> String {
    format!("matiere-{}", suivi::slug(m))
}

// ---------------------------------------------------------------- lecture / écriture de la séance

fn assurer_ligne(c: &Connection) -> Result<()> {
    c.execute("INSERT INTO seances_jour (jour, maj) VALUES (?1, ?2) ON CONFLICT(jour) DO NOTHING", params![jour(), maintenant()])?;
    Ok(())
}

fn questions(c: &Connection) -> Result<Vec<QuestionJour>> {
    let t: Option<String> = c.query_row("SELECT questions FROM seances_jour WHERE jour = ?1", [jour()], |r| r.get(0)).optional()?;
    Ok(t.map(|t| serde_json::from_str(&t).unwrap_or_default()).unwrap_or_default())
}

/// Ajoute des questions à la fin (les positions des questions déjà là ne changent jamais).
fn ajouter_questions(c: &Connection, nouvelles: Vec<QuestionJour>) -> Result<()> {
    let _verrou = ECRITURE.lock().unwrap();
    let mut q = questions(c)?;
    q.extend(nouvelles);
    c.execute(
        "UPDATE seances_jour SET questions = ?2, erreur = NULL, maj = ?3 WHERE jour = ?1",
        params![jour(), serde_json::to_string(&q)?, maintenant()],
    )?;
    Ok(())
}

fn reponses(c: &Connection) -> Result<Vec<ReponseJour>> {
    let mut st = c.prepare("SELECT idx, statut, score, reponse, retour FROM reponses_jour WHERE jour = ?1 ORDER BY idx")?;
    let v = st
        .query_map([jour()], |r| {
            Ok(ReponseJour {
                idx: r.get::<_, i64>(0)? as usize,
                statut: r.get(1)?,
                score: r.get(2)?,
                reponse: serde_json::from_str(&r.get::<_, String>(3)?).unwrap_or(Value::Null),
                retour: r.get(4)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

fn matieres_enregistrees(c: &Connection) -> Result<Vec<MatiereJour>> {
    let t: String = c.query_row("SELECT lecons FROM seances_jour WHERE jour = ?1", [jour()], |r| r.get(0))?;
    Ok(serde_json::from_str(&t).unwrap_or_default())
}

fn a_des_questions(qs: &[QuestionJour], matiere: &str) -> bool {
    qs.iter().any(|q| q.phase == "comprendre" && suivi::slug(&q.matiere) == suivi::slug(matiere))
}

pub fn etat(c: &Connection) -> Result<EtatJour> {
    assurer_ligne(c)?;
    let erreur: Option<String> = c.query_row("SELECT erreur FROM seances_jour WHERE jour = ?1", [jour()], |r| r.get(0))?;
    let matieres = matieres_enregistrees(c)?;
    let questions = questions(c)?;
    let reponses = reponses(c)?;

    let etats = matieres
        .into_iter()
        .map(|m| {
            let (mut total, mut acquis, mut nb, mut faites) = (0.0, 0.0, 0, 0);
            for (i, q) in questions.iter().enumerate() {
                if q.phase != "comprendre" || suivi::slug(&q.matiere) != suivi::slug(&m.matiere) {
                    continue;
                }
                nb += 1;
                total += poids(&q.niveau);
                if let Some(r) = reponses.iter().find(|r| r.idx == i) {
                    faites += 1;
                    if r.statut != "en_correction" {
                        acquis += poids(&q.niveau) * r.score;
                    }
                }
            }
            EtatMatiere {
                en_preparation: en_cours(&cle_matiere(&m.matiere)),
                comprehension: (total > 0.0).then(|| (acquis / total * 100.0).round()),
                nb_questions: nb,
                nb_repondues: faites,
                matiere: m,
            }
        })
        .collect::<Vec<_>>();
    Ok(EtatJour {
        jour: jour(),
        date_lecons: etats.first().and_then(|m| m.matiere.lecons.first()).map(|l| l.debut.get(..10).unwrap_or_default().to_string()),
        matieres: etats,
        questions,
        reponses,
        suite_en_cours: en_cours("suite"),
        erreur,
    })
}

// ---------------------------------------------------------------- matières du jour

/// Les matières qu'on a eues aujourd'hui (ou, s'il n'y a pas eu cours, le dernier jour de cours),
/// dans l'ordre de l'emploi du temps, avec leurs leçons.
pub fn matieres_du_jour(dossier: &Path) -> Vec<MatiereJour> {
    let Ok(t) = std::fs::read_to_string(dossier.join("pronote").join("sync.json")) else { return vec![] };
    let Ok(sync) = serde_json::from_str::<Value>(&t) else { return vec![] };
    let auj = jour();
    let cours: Vec<&Value> = sync["cours"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["annule"] != true && c["debut"].as_str().unwrap_or("").get(..10).is_some_and(|d| d <= auj.as_str()))
        .collect();
    let Some(date) = cours.iter().filter_map(|c| c["debut"].as_str().and_then(|d| d.get(..10))).max().map(String::from) else { return vec![] };
    let mut du_jour: Vec<&Value> = cours.into_iter().filter(|c| c["debut"].as_str().unwrap_or("").starts_with(&date)).collect();
    du_jour.sort_by(|a, b| a["debut"].as_str().cmp(&b["debut"].as_str()));

    let mut sortie: Vec<MatiereJour> = Vec::new();
    for c in du_jour {
        let matiere = c["matiere"].as_str().unwrap_or("Autre").to_string();
        if !revisable(&matiere) {
            continue;
        }
        let titre = c["contenu"]["titre"].as_str().filter(|t| !t.is_empty()).unwrap_or("").to_string();
        let description: String = c["contenu"]["description"].as_str().unwrap_or("").chars().take(1500).collect();
        let utile = format!("{titre} {description}").trim().chars().count() >= 12;
        let lecon = Lecon {
            matiere: matiere.clone(),
            titre: if titre.is_empty() { "Cours".into() } else { titre },
            description,
            debut: c["debut"].as_str().unwrap_or("").into(),
            documents: c["contenu"]["fichiers"].as_array().into_iter().flatten().filter_map(|f| f["chemin"].as_str().map(String::from)).collect(),
        };
        match sortie.iter_mut().find(|m| m.matiere == matiere) {
            Some(m) => {
                m.a_contenu |= utile;
                m.lecons.push(lecon);
            }
            None => sortie.push(MatiereJour {
                heure: c["debut"].as_str().and_then(|d| d.get(11..16)).unwrap_or("").replace(':', "h"),
                a_contenu: utile,
                lecons: vec![lecon],
                matiere,
            }),
        }
    }
    sortie
}

/// Pas de questions de cours en sport, en vie de classe ou en orientation.
pub fn revisable(matiere: &str) -> bool {
    let s = suivi::slug(matiere);
    !["sport", "eps", "vie-de-classe", "heure-de-vie", "accomp", "orient", "pause", "permanence", "etude"].iter().any(|m| s.contains(m))
}

// ---------------------------------------------------------------- préparation (IA locale)

/// Nombre de questions de compréhension par matière.
const PAR_MATIERE: usize = 6;

/// (début du cours, titre de la leçon) des leçons d'une matière du jour.
fn debuts(m: &MatiereJour) -> Vec<(String, String)> {
    m.lecons.iter().map(|l| (l.debut.clone(), l.titre.clone())).collect()
}

/// Met la base de connaissances à jour (nouveaux cours, documents, OCR) : rapide si rien n'a changé.
fn connaissances_a_jour(c: &Connection) {
    match connaissances::mettre_a_jour(c) {
        Ok(r) if r.nouvelles > 0 || !r.illisibles.is_empty() => {
            eprintln!("connaissances : {} nouvelles sources, {} passages, {} vecteurs, illisibles : {:?}", r.nouvelles, r.passages, r.vecteurs, r.illisibles)
        }
        Ok(_) => {}
        Err(e) => eprintln!("connaissances : {e:#}"),
    }
}

/// Prépare une matière : ses questions arrivent dans la séance une par une, dès qu'elles sont prêtes.
fn preparer_une(c: &Connection, m: &MatiereJour) -> Result<()> {
    generation::serie_comprendre(c, &m.matiere, &debuts(m), PAR_MATIERE, |q| ajouter_questions(c, vec![q]))?;
    Ok(())
}

/// Prépare en arrière-plan toutes les matières du jour qui ne le sont pas encore, une par une
/// (celles qui ont le plus de contenu d'abord). Sans effet sur une matière déjà prête ou en cours.
pub fn lancer_preparation(chemin: PathBuf) -> bool {
    let Ok(c) = db::ouvrir(&chemin) else { return false };
    let matieres = matieres_du_jour(&config::dossier_donnees());
    if assurer_ligne(&c).is_err()
        || c.execute("UPDATE seances_jour SET lecons = ?2 WHERE jour = ?1", params![jour(), serde_json::to_string(&matieres).unwrap_or_default()]).is_err()
    {
        return false;
    }
    tache("jour".into(), chemin.clone(), move |c| {
        connaissances_a_jour(c);
        let mut ordre = matieres;
        ordre.sort_by_key(|m| std::cmp::Reverse((m.a_contenu, m.lecons.iter().map(|l| l.description.len()).sum::<usize>())));
        for m in ordre {
            if a_des_questions(&questions(c)?, &m.matiere) || !reserver(&cle_matiere(&m.matiere)) {
                continue; // déjà prête, ou l'élève l'a choisie et elle se prépare déjà à part
            }
            let r = preparer_une(c, &m);
            liberer(&cle_matiere(&m.matiere));
            if let Err(e) = r {
                noter_erreur(c, &format!("{} : {e}", m.matiere));
            }
        }
        Ok(())
    })
}

/// L'élève a choisi une matière pas encore prête : elle passe devant, tout de suite.
pub fn preparer_matiere(chemin: PathBuf, matiere: String) -> bool {
    let Ok(c) = db::ouvrir(&chemin) else { return false };
    let Some(m) = matieres_enregistrees(&c).unwrap_or_default().into_iter().find(|m| m.matiere == matiere) else { return false };
    if a_des_questions(&questions(&c).unwrap_or_default(), &m.matiere) {
        return false;
    }
    tache(cle_matiere(&m.matiere), chemin, move |c| {
        connaissances_a_jour(c);
        preparer_une(c, &m)
    })
}

/// Préparation complète sans fil à part (ligne de commande).
pub fn preparer(c: &Connection, seulement: Option<&str>) -> Result<()> {
    assurer_ligne(c)?;
    let matieres = matieres_du_jour(&config::dossier_donnees());
    c.execute("UPDATE seances_jour SET lecons = ?2 WHERE jour = ?1", params![jour(), serde_json::to_string(&matieres)?])?;
    connaissances_a_jour(c);
    for m in matieres.iter().filter(|m| seulement.is_none_or(|s| suivi::slug(s) == suivi::slug(&m.matiere))) {
        if !a_des_questions(&questions(c)?, &m.matiere) {
            preparer_une(c, m)?;
        }
    }
    Ok(())
}

/// Prépare une série de questions de révision (notions faibles, à revoir, évals proches), en arrière-plan.
pub fn lancer_suite(chemin: PathBuf) -> bool {
    tache("suite".into(), chemin, |c| {
        let deja: Vec<String> = questions(c)?.into_iter().map(|q| q.enonce).collect();
        generation::serie_reviser(c, PAR_MATIERE, &deja, |q| ajouter_questions(c, vec![q]))?;
        Ok(())
    })
}


pub fn valide(q: &QuestionJour) -> bool {
    if !suivi::NIVEAUX.contains(&q.niveau.as_str()) || q.enonce.trim().is_empty() || q.matiere.is_empty() {
        return false;
    }
    match q.format.as_str() {
        "qcm" => q.choix.len() == 4 && q.bonne.is_some_and(|b| b < 4),
        "vrai_faux" => q.bonne.is_some_and(|b| b < 2),
        "reponse_courte" => !q.reponses_acceptees.is_empty(),
        "calcul" => q.valeur.is_some(),
        // On développe toujours à partir d'un document, comme en éval.
        // Un vrai développement se fait sur document ; une explication courte (une seule zone) n'en a pas besoin.
        "developpement" => !q.attendus.is_empty() && (q.support.as_deref().is_some_and(|s| s.trim().chars().count() >= 20) || q.plan.len() == 1),
        _ => false,
    }
}

// ---------------------------------------------------------------- réponses et correction

/// Minuscules, sans accents ni ponctuation, espaces simples : pour comparer des réponses tapées.
pub fn normaliser(s: &str) -> String {
    let slug = suivi::slug(s);
    let s = slug.replace('-', " ");
    let mots: Vec<&str> = s.split_whitespace().filter(|m| !["le", "la", "les", "l", "un", "une", "des", "the", "a", "an", "il", "lo", "gli"].contains(m)).collect();
    mots.join(" ")
}

fn nombre(t: &str) -> Option<f64> {
    let propre: String = t.replace(',', ".").chars().filter(|c| c.is_ascii_digit() || *c == '.' || *c == '-').collect();
    propre.parse().ok()
}

/// Enregistre la réponse à la question `idx`. Corrige tout de suite quand c'est possible ;
/// sinon (réponse rédigée), l'IA corrige en arrière-plan et le statut passe à « en_correction ».
pub fn repondre(c: &Connection, chemin: PathBuf, idx: usize, reponse: Value) -> Result<ReponseJour> {
    let qs = questions(c)?;
    let q = qs.get(idx).context("question inconnue")?.clone();
    let deja: Option<String> = c.query_row("SELECT statut FROM reponses_jour WHERE jour = ?1 AND idx = ?2", params![jour(), idx as i64], |r| r.get(0)).optional()?;
    if deja.is_some() {
        bail!("question déjà répondue");
    }

    let passe = reponse["passe"] == true;
    let (statut, score, erreur): (&str, f64, Option<String>) = if passe {
        ("faux", 0.0, Some(format!("oubli : ne savait pas ({})", q.enonce.chars().take(70).collect::<String>())))
    } else {
        match q.format.as_str() {
            "qcm" | "vrai_faux" => {
                let choix = reponse["choix"].as_u64().map(|x| x as usize);
                if choix == q.bonne {
                    ("juste", 1.0, None)
                } else {
                    let e = choix.and_then(|i| q.erreurs.get(i).cloned().flatten()).unwrap_or_else(|| format!("a répondu « {} »", choix.and_then(|i| q.choix.get(i).cloned()).unwrap_or_default()));
                    ("faux", 0.0, Some(e))
                }
            }
            "calcul" => {
                let x = reponse["texte"].as_str().and_then(nombre);
                let v = q.valeur.unwrap_or_default();
                let tol = q.tolerance.unwrap_or((v.abs() * 0.01).max(0.01));
                match x {
                    Some(x) if (x - v).abs() <= tol + 1e-9 => ("juste", 1.0, None),
                    Some(x) => ("faux", 0.0, Some(format!("calcul : a trouvé {x} au lieu de {v}"))),
                    None => ("faux", 0.0, Some("calcul : pas de résultat chiffré".into())),
                }
            }
            "reponse_courte" => {
                let r = normaliser(reponse["texte"].as_str().unwrap_or(""));
                let ok = !r.is_empty() && q.reponses_acceptees.iter().any(|a| {
                    let a = normaliser(a);
                    !a.is_empty() && (a == r || (a.len() >= 4 && r.contains(&a)))
                });
                if ok { ("juste", 1.0, None) } else { ("en_correction", 0.0, None) }
            }
            _ => ("en_correction", 0.0, None),
        }
    };

    let retour = (statut != "en_correction").then(|| q.explication.clone());
    c.execute(
        "INSERT INTO reponses_jour (jour, idx, reponse, statut, score, retour, cree_le) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![jour(), idx as i64, reponse.to_string(), statut, score, retour, maintenant()],
    )?;

    if statut == "en_correction" {
        let qc = q.clone();
        let rc = reponse.clone();
        tache(format!("correction-{idx}"), chemin, move |c| corriger(c, idx, &qc, &rc));
    } else {
        noter_essai(c, &q, score >= 0.7, erreur.as_deref(), &texte_reponse(&q, &reponse))?;
    }
    Ok(ReponseJour { idx, statut: statut.into(), score, reponse, retour })
}

fn texte_reponse(q: &QuestionJour, r: &Value) -> String {
    if r["passe"] == true {
        return "je sais pas".into();
    }
    if let Some(i) = r["choix"].as_u64() {
        return q.choix.get(i as usize).cloned().unwrap_or_default();
    }
    if let Some(s) = r["sections"].as_array() {
        return q.plan.iter().zip(s).map(|(p, t)| format!("[{p}] {}", t.as_str().unwrap_or(""))).collect::<Vec<_>>().join("\n");
    }
    r["texte"].as_str().unwrap_or("").to_string()
}

fn noter_essai(c: &Connection, q: &QuestionJour, ok: bool, erreur: Option<&str>, reponse: &str) -> Result<()> {
    suivi::resultat(
        c,
        &suivi::NouveauResultat {
            matiere: &q.matiere,
            chapitre: &q.chapitre,
            notion: &q.notion,
            niveau: &q.niveau,
            ok,
            erreur,
            question: Some(&serde_json::to_string(q)?),
            reponse: Some(reponse),
        },
    )?;
    Ok(())
}

/// Réponse courte proche par le sens d'une réponse acceptée : juste sans passer par le modèle.
fn proche_par_le_sens(q: &QuestionJour, texte: &str) -> bool {
    if q.reponses_acceptees.is_empty() || texte.trim().is_empty() {
        return false;
    }
    let mut tous = vec![texte.to_string()];
    tous.extend(q.reponses_acceptees.iter().cloned());
    match ia::embeddings(&tous, false) {
        Ok(v) if v.len() == tous.len() => v[1..].iter().any(|a| ia::cosinus(&v[0], a) >= 0.92),
        _ => false,
    }
}

const SYSTEME_CORRECTION: &str = "Tu corriges la réponse d'un lycéen de seconde comme un bon professeur : précis, juste et bienveillant. Tu t'appuies sur la connaissance du cours et les points attendus. Tu tutoies l'élève. Réponds uniquement en JSON.";

/// Correction d'une réponse rédigée (réponse courte non reconnue, développement) par l'IA locale.
fn corriger(c: &Connection, idx: usize, q: &QuestionJour, reponse: &Value) -> Result<()> {
    let texte = texte_reponse(q, reponse);
    let (score, retour, erreur) = if q.format == "reponse_courte" && proche_par_le_sens(q, &texte) {
        (1.0, q.explication.clone(), None)
    } else {
        let attendus = if q.attendus.is_empty() {
            q.reponses_acceptees.join(" / ")
        } else {
            q.attendus.iter().map(|a| format!("- {a}")).collect::<Vec<_>>().join("\n")
        };
        let mut prompt = format!("Question ({}) : {}\n", q.matiere, q.enonce);
        if let Some(s) = &q.support {
            prompt.push_str(&format!("Document :\n{s}\n"));
        }
        if let Some(k) = &q.connaissance {
            prompt.push_str(&format!("Connaissance du cours en jeu : {k}\n"));
        }
        prompt.push_str(&format!("Réponses acceptées / points attendus :\n{attendus}\nCorrection de référence : {}\n\n", q.explication));
        prompt.push_str(&format!("Réponse de l'élève :\n\"\"\"\n{texte}\n\"\"\"\n\n"));
        prompt.push_str(concat!(
            "Une réponse courte juste mais formulée autrement (synonyme, faute d'orthographe légère) est juste. ",
            "Pour une réponse rédigée, note la part des points attendus vraiment présents et bien expliqués. ",
            "S'il y a un document, l'élève doit l'EXPLOITER (citer, relever un chiffre, un détail) : s'il récite son cours ",
            "sans s'en servir, dis-le et montre quel passage il aurait pu utiliser.\n",
            "\"retour\" : 2 ou 3 phrases — ce qui est juste, puis ce qui manque ou est faux, et la bonne réponse.\n",
            "\"erreur\" : type parmi oubli, confusion, vocabulaire, incompréhension, méthode, calcul, consigne, suivi de « : détail » ; null si c'est juste."
        ));
        let schema = serde_json::json!({
            "type": "object",
            "properties": { "score": { "type": "number", "minimum": 0, "maximum": 1 }, "retour": { "type": "string" }, "erreur": { "type": ["string", "null"] } },
            "required": ["score", "retour", "erreur"]
        });
        match ia::demander_json(SYSTEME_CORRECTION, &prompt, &ia::Options { schema: Some(schema), temperature: Some(0.3), ..Default::default() }) {
            Ok(v) => (
                v["score"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
                v["retour"].as_str().unwrap_or(&q.explication).to_string(),
                v["erreur"].as_str().filter(|e| !e.is_empty() && *e != "null").map(String::from),
            ),
            // IA indisponible : on donne la correction de référence sans pénaliser.
            Err(e) => (0.5, format!("Correction automatique indisponible ({e}). La réponse attendue : {}", q.explication), None),
        }
    };
    let statut = if score >= 0.7 { "juste" } else if score >= 0.3 { "partiel" } else { "faux" };
    c.execute(
        "UPDATE reponses_jour SET statut = ?3, score = ?4, retour = ?5 WHERE jour = ?1 AND idx = ?2",
        params![jour(), idx as i64, statut, score, retour],
    )?;
    noter_essai(c, q, score >= 0.7, erreur.as_deref(), &texte)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalisation() {
        assert_eq!(normaliser("  La Photosynthèse ! "), "photosynthese");
        assert_eq!(normaliser("1789"), "1789");
    }

    #[test]
    fn matieres_revisables() {
        assert!(!revisable("ED.PHYSIQUE & SPORT."));
        assert!(!revisable("ACCOMP.CHOIX ORIENT."));
        assert!(revisable("PHYSIQUE-CHIMIE"));
        assert!(revisable("ITALIEN LV2"));
    }

    #[test]
    fn nombres() {
        assert_eq!(nombre("12,5 €"), Some(12.5));
        assert_eq!(nombre("-3"), Some(-3.0));
        assert_eq!(nombre("abc"), None);
    }

    #[test]
    fn validation_formats() {
        let base = || QuestionJour {
            phase: "reviser".into(), matiere: "SVT".into(), lecon: None, chapitre: "c".into(), notion: "n".into(), niveau: "savoir".into(),
            format: "qcm".into(), enonce: "?".into(), support: None, choix: vec![], bonne: None, reponses_acceptees: vec![], valeur: None,
            tolerance: None, unite: None, plan: vec![], attendus: vec![], erreurs: vec![], explication: "".into(), connaissance: None,
        };
        let mut q = base();
        assert!(!valide(&q));
        q.choix = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        q.bonne = Some(2);
        assert!(valide(&q));
        let mut c = base();
        c.format = "calcul".into();
        c.valeur = Some(25.0);
        assert!(valide(&c));
        // Un développement sans document est refusé.
        let mut d = base();
        d.format = "developpement".into();
        d.attendus = vec!["cite le texte".into()];
        assert!(!valide(&d));
        d.support = Some("« Épouser une sotte est pour n'être point sot. » (L'École des femmes, I, 1)".into());
        assert!(valide(&d));
    }
}
