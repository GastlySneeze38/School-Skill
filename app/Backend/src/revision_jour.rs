//! La séance du jour, l'écran principal de l'app : comprendre la leçon du jour, puis réviser.
//!
//! - Préparée en arrière-plan dès l'ouverture de l'app (et après une synchro) : l'élève n'attend pas.
//! - Tant que Claude prépare, un « échauffement » reprend des questions déjà posées sur des
//!   notions à revoir, pour démarrer tout de suite.
//! - Chaque question a le format qui rend la réponse la plus naturelle (QCM, vrai/faux, réponse
//!   courte, calcul, développement) ; QCM, vrai/faux et calcul sont corrigés sur place, les réponses
//!   rédigées par Claude en arrière-plan pendant que l'élève continue.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use anyhow::{Context, Result, bail};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{claude, config, db, moyennes, suivi};

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
    /// juste, partiel, faux, en_correction (Claude corrige en arrière-plan).
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

/// Derniers cours avec contenu d'une matière (quand la leçon du jour n'en a pas).
fn derniers_cours(dossier: &Path, matiere: &str) -> Vec<Value> {
    let Ok(t) = std::fs::read_to_string(dossier.join("pronote").join("sync.json")) else { return vec![] };
    let Ok(sync) = serde_json::from_str::<Value>(&t) else { return vec![] };
    let auj = jour();
    let mut v: Vec<Value> = sync["cours"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| c["matiere"].as_str() == Some(matiere) && !c["contenu"].is_null())
        .filter(|c| c["debut"].as_str().unwrap_or("").get(..10).is_some_and(|d| d <= auj.as_str()))
        .map(|c| {
            let documents: Vec<&str> = c["contenu"]["fichiers"].as_array().into_iter().flatten().filter_map(|f| f["chemin"].as_str()).collect();
            json!({ "date": &c["debut"].as_str().unwrap_or("")[..10], "titre": c["contenu"]["titre"], "contenu": c["contenu"]["description"], "documents": documents })
        })
        .collect();
    v.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));
    v.truncate(4);
    v
}

// ---------------------------------------------------------------- préparation

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
        let mut ordre = matieres;
        ordre.sort_by_key(|m| std::cmp::Reverse((m.a_contenu, m.lecons.iter().map(|l| l.description.len()).sum::<usize>())));
        for m in ordre {
            if a_des_questions(&questions(c)?, &m.matiere) || !reserver(&cle_matiere(&m.matiere)) {
                continue; // déjà prête, ou l'élève l'a choisie et elle se prépare déjà à part
            }
            let r = generer(c, Phase::Comprendre(&m)).and_then(|q| ajouter_questions(c, q));
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
    tache(cle_matiere(&m.matiere), chemin, move |c| ajouter_questions(c, generer(c, Phase::Comprendre(&m))?))
}

/// Préparation complète sans fil à part (ligne de commande).
pub fn preparer(c: &Connection, seulement: Option<&str>) -> Result<()> {
    assurer_ligne(c)?;
    let matieres = matieres_du_jour(&config::dossier_donnees());
    c.execute("UPDATE seances_jour SET lecons = ?2 WHERE jour = ?1", params![jour(), serde_json::to_string(&matieres)?])?;
    for m in matieres.iter().filter(|m| seulement.is_none_or(|s| suivi::slug(s) == suivi::slug(&m.matiere))) {
        if !a_des_questions(&questions(c)?, &m.matiere) {
            ajouter_questions(c, generer(c, Phase::Comprendre(m))?)?;
        }
    }
    Ok(())
}

/// Prépare une série de questions de révision (points faibles, évals proches, moyennes à remonter).
pub fn lancer_suite(chemin: PathBuf) -> bool {
    tache("suite".into(), chemin, |c| {
        let nouvelles = generer(c, Phase::Reviser)?;
        ajouter_questions(c, nouvelles)
    })
}
#[derive(Clone, Copy)]
enum Phase<'a> {
    Comprendre(&'a MatiereJour),
    Reviser,
}

#[derive(Deserialize)]
struct ChapitrePropose {
    matiere: String,
    code: String,
    titre: String,
    notions: Vec<NotionProposee>,
}

#[derive(Deserialize)]
struct NotionProposee {
    code: String,
    intitule: String,
}

const REGLES_FORMATS: &str = r#"Pour CHAQUE question, choisis le format qui rend la réponse la plus naturelle et la plus rapide :
- "qcm" : 4 choix, pour distinguer des notions proches ; les 3 mauvais choix sont des erreurs réelles.
  Champs : "choix" (4), "bonne" (0-3), "erreurs" (4 éléments, null pour la bonne réponse).
- "vrai_faux" : vérification rapide d'une affirmation (pas plus d'1 question sur 5).
  Champs : "bonne" (0 = Vrai, 1 = Faux), "erreurs" (2 éléments).
- "reponse_courte" : un mot, une date, une définition en quelques mots, une traduction.
  Champs : "reponses_acceptees" (toutes les variantes justes : synonymes, avec/sans article, chiffres…).
- "calcul" : un résultat chiffré. Champs : "valeur" (nombre), "tolerance" (écart accepté), "unite" (ou null).
- "developpement" : expliquer, justifier, argumenter À PARTIR D'UN DOCUMENT (histoire-géo, SES, français,
  sciences). Au plus 1 ou 2 par série. "support" est OBLIGATOIRE : le document que l'élève a sous les yeux
  (voir « Documents » plus bas). Champs : "plan" (2 à 4 parties courtes, ex. ["Ce que montre le document",
  "Explication avec le cours", "Conclusion"]), "attendus" (3 à 5 points qu'une bonne réponse doit contenir,
  dont au moins un qui exige de citer ou d'exploiter précisément le document).
Langues : privilégie reponse_courte (vocabulaire, conjugaison, traduction) et qcm, mais en contexte : une phrase
ou un court texte dans la langue (le support), plutôt qu'un mot isolé."#;

/// Ce qui fait une bonne question : de la réflexion sur ce qui s'est vraiment passé en cours.
const REGLES_QUESTIONS: &str = r#"Des questions qui font RÉFLÉCHIR, pas réciter. Une question de cours (« Donne la définition de… »,
« Qui a écrit… ? », « En quelle année… ? ») ne demande aucune réflexion : au plus 1 ou 2 par série, en
échauffement. Toutes les autres demandent de SE SERVIR du cours pour raisonner :
- expliquer un pourquoi ou un comment (« Pourquoi Arnolphe veut-il épouser une jeune fille ignorante ? ») ;
- interpréter un document : que montre-t-il, qu'est-ce qui le prouve, quelle limite a-t-il ;
- appliquer à un cas nouveau, prévoir ce qui se passerait si… ;
- comparer deux notions, deux personnages, deux situations, et dire ce qui change ;
- repérer l'erreur dans un raisonnement, ou choisir entre deux interprétations et justifier.

Colle à CE QUI S'EST VRAIMENT PASSÉ EN COURS : les textes, scènes, documents, exemples, exercices et
activités cités dans le cahier de textes et dans les documents des profs. Une question doit ressembler à ce que
CE prof pourrait demander en éval sur CE cours, pas à une question générique de manuel. Le programme officiel ne
sert qu'à combler un cours vide.

DES DOCUMENTS : au moins la moitié des questions partent d'un "support". Ouvre les documents des profs avec
l'outil Read (PDF, images) et prends-y un extrait, un tableau de chiffres, une phrase d'un texte étudié, la
légende d'un schéma, l'énoncé d'une activité faite en classe. À défaut, un court extrait authentique d'une œuvre
étudiée (texte exact, pas inventé). Le support est du TEXTE recopié fidèlement (pas un nom de fichier), 1 à 15
lignes, et finit par sa source entre parenthèses, ex. « (Molière, L'École des femmes, I, 1) » ou
« (Document du prof : Activité 2, la photosynthèse) ». Pour un graphique ou une image, décris précisément ce
qu'on y voit et recopie les valeurs utiles.

La correction ("explication") s'appuie aussi sur le cours et le document : elle fait comprendre, elle ne se
contente pas de donner la réponse."#;

fn generer(c: &Connection, phase: Phase) -> Result<Vec<QuestionJour>> {
    let skill = config::dossier_skill();
    let pedagogie = std::fs::read_to_string(skill.join("references").join("pedagogie.md")).unwrap_or_default();
    let carte: Vec<Value> = suivi::carte(c, None, None)?
        .into_iter()
        .map(|l| json!({ "matiere": l.matiere, "chapitre": l.chapitre, "titre_chapitre": l.titre_chapitre, "notion": l.notion, "intitule": l.intitule, "etat": l.etat, "derniere_erreur": l.derniere_erreur }))
        .collect();

    let (tache_txt, contexte) = match phase {
        Phase::Comprendre(m) => (
            format!(
                "Vérifie que l'élève a COMPRIS ce qu'il a fait aujourd'hui en {matiere}. Écris 6 à 8 questions : 1 ou 2 de\n\
                 « savoir » pour démarrer, puis surtout « comprendre » et « appliquer », toutes avec \"matiere\": \"{matiere}\", \"phase\": \"comprendre\" et \"lecon\": le titre de la\n\
                 leçon visée. Ignore ce qui n'est pas du contenu (« correction de l'évaluation », « rendu des copies »…). Si le prof\n\
                 n'a rien écrit pour la leçon du jour, appuie-toi sur les derniers cours de la matière (chapitre en cours) et sur le\n\
                 programme officiel du niveau de l'élève.",
                matiere = m.matiere
            ),
            format!(
                "Leçon(s) du {} :\n{}\n\nDerniers cours de cette matière (avec leurs documents) :\n{}\n\nDocuments du jour (lisibles avec l'outil Read) :\n{}",
                m.lecons.first().map(|l| &l.debut[..10]).unwrap_or(""),
                serde_json::to_string_pretty(&m.lecons.iter().map(|l| json!({ "titre": l.titre, "contenu": l.description })).collect::<Vec<_>>())?,
                serde_json::to_string_pretty(&derniers_cours(&config::dossier_donnees(), &m.matiere))?,
                m.lecons.iter().flat_map(|l| l.documents.clone()).collect::<Vec<_>>().join("\n")
            ),
        ),
        Phase::Reviser => {
            let p = suivi::priorites(c, 10)?;
            let analyse = moyennes::derniere_analyse(c)?;
            let deja: Vec<String> = questions(c)?.into_iter().map(|q| q.enonce).collect();
            (
                String::from("Écris 8 questions de RÉVISION, \"phase\": \"reviser\". Priorité : les notions en rouge/orange ou à revoir\n\
                 aujourd'hui (répétition espacée), les chapitres des évals proches, puis les matières où l'analyse des moyennes voit\n\
                 le plus de points à gagner (tu peux y créer de nouveaux chapitres d'après les cours récents). Varie les matières.\n\
                 Ne repose pas une question déjà posée aujourd'hui (liste plus bas) : reformule ou change d'angle."),
                format!(
                    "Priorités :\n{}\n\nÉvaluations à venir : {}\n\nAnalyse des moyennes : {}\n\nCours récents :\n{}\n\n\
                     Documents des profs : dossier pronote/fichiers/<matière>/ (outil Read).\n\nDéjà posé aujourd'hui :\n{}",
                    serde_json::to_string_pretty(&p.priorites)?,
                    serde_json::to_string(&p.evals_a_venir)?,
                    analyse.map(|a| serde_json::to_string(&a.matieres).unwrap_or_default()).unwrap_or_else(|| "aucune".into()),
                    cours_recents(&config::dossier_donnees()),
                    deja.join("\n")
                ),
            )
        }
    };

    let prompt = format!(
        r#"Tu prépares la séance de révision du jour d'un lycéen, dans son app. L'app affiche les questions une à une,
corrige, et met à jour son suivi : toi, tu rédiges les questions.

===== Pédagogie (skill « revisions ») =====
{pedagogie}

===== Notions déjà suivies (réutilise exactement ces codes de matière/chapitre/notion) =====
{carte}

===== Ta tâche =====
{tache_txt}

{contexte}

===== Ce qu'on attend des questions =====
{REGLES_QUESTIONS}

===== Formats =====
{REGLES_FORMATS}

Réponds UNIQUEMENT avec un objet JSON, sans texte autour :
{{
  "chapitres": [{{"matiere": "nom Pronote exact", "code": "photosynthese", "titre": "La photosynthèse",
                  "notions": [{{"code": "bilan", "intitule": "Bilan de la photosynthèse"}}]}}],
  "questions": [{{
    "phase": "...", "matiere": "nom Pronote exact", "lecon": "titre ou null",
    "chapitre": "photosynthese", "notion": "bilan", "niveau": "savoir|comprendre|appliquer",
    "format": "qcm|vrai_faux|reponse_courte|calcul|developpement",
    "enonce": "…", "support": "extrait recopié (source)" ou null,
    "choix": [], "bonne": null, "erreurs": [],
    "reponses_acceptees": [], "valeur": null, "tolerance": null, "unite": null,
    "plan": [], "attendus": [],
    "explication": "La correction en 1 à 3 phrases, qui fait comprendre."
  }}]
}}
"chapitres" : seulement les chapitres nouveaux ou complétés (sinon []). Chaque question vise une notion
existante ou déclarée dans "chapitres"."#,
        carte = serde_json::to_string(&carte)?,
    );

    let r = claude::demander(&prompt, "Read", Some(&config::dossier_donnees()));
    if !r.ok {
        bail!("{}", r.texte);
    }
    let v = claude::extraire_json(&r.texte)?;
    let source = format!("séance du jour {}", Local::now().format("%d/%m/%Y"));
    for ch in serde_json::from_value::<Vec<ChapitrePropose>>(v["chapitres"].clone()).unwrap_or_default() {
        let notions: Vec<String> = ch.notions.iter().map(|n| format!("{}:{}", n.code, n.intitule)).collect();
        suivi::chapitre(c, &ch.matiere, &ch.code, Some(&ch.titre), &notions, Some(&source))?;
    }
    let brutes: Vec<QuestionJour> = serde_json::from_value(v["questions"].clone()).context("questions illisibles")?;
    let connues: HashSet<(String, String, String)> =
        suivi::carte(c, None, None)?.into_iter().map(|l| (suivi::slug(&l.matiere), l.chapitre, l.notion)).collect();
    let phase_txt = if matches!(phase, Phase::Comprendre(_)) { "comprendre" } else { "reviser" };
    let valides: Vec<QuestionJour> = brutes
        .into_iter()
        .map(|mut q| {
            q.phase = phase_txt.into();
            q.notion = suivi::slug(&q.notion);
            if q.format == "vrai_faux" {
                q.choix = vec!["Vrai".into(), "Faux".into()];
            }
            if q.format == "developpement" && q.plan.is_empty() {
                q.plan = vec!["Ta réponse".into()];
            }
            q
        })
        .filter(|q| valide(q) && connues.contains(&(suivi::slug(&q.matiere), q.chapitre.clone(), q.notion.clone())))
        .collect();
    if valides.is_empty() {
        bail!("Claude n'a pas produit de questions utilisables, réessaie.");
    }
    Ok(valides)
}

fn cours_recents(dossier: &Path) -> String {
    let Ok(t) = std::fs::read_to_string(dossier.join("pronote").join("sync.json")) else { return String::new() };
    let Ok(sync) = serde_json::from_str::<Value>(&t) else { return String::new() };
    let auj = jour();
    let mut v: Vec<String> = sync["cours"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| !c["contenu"].is_null() && c["debut"].as_str().unwrap_or("").get(..10).is_some_and(|d| d <= auj.as_str()))
        .map(|c| format!("- {} {} : {}", &c["debut"].as_str().unwrap_or("")[..10], c["matiere"].as_str().unwrap_or(""), c["contenu"]["titre"].as_str().or(c["contenu"]["description"].as_str()).unwrap_or("").chars().take(120).collect::<String>()))
        .collect();
    v.sort();
    v.into_iter().rev().take(25).collect::<Vec<_>>().join("\n")
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
        "developpement" => !q.attendus.is_empty() && q.support.as_deref().is_some_and(|s| s.trim().chars().count() >= 20),
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
/// sinon (réponse rédigée), Claude corrige en arrière-plan et le statut passe à « en_correction ».
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

/// Correction d'une réponse rédigée par Claude (réponse courte non reconnue, développement).
fn corriger(c: &Connection, idx: usize, q: &QuestionJour, reponse: &Value) -> Result<()> {
    let texte = texte_reponse(q, reponse);
    let prompt = format!(
        r#"Corrige la réponse d'un lycéen, avec bienveillance et précision (correction d'un prof).

Question ({matiere}, niveau {niveau}) : {enonce}
{support}
Réponses acceptées / points attendus :
{attendus}
Correction de référence : {explication}

Réponse de l'élève :
"""
{texte}
"""

Une réponse courte juste mais formulée autrement (synonyme, faute d'orthographe légère) est juste.
Pour un développement, note la part des points attendus réellement présents et bien expliqués. L'élève doit
EXPLOITER le document (le citer, relever un chiffre, un mot, un détail précis) : s'il récite son cours sans s'en
servir, dis-le dans le retour et montre quel passage il aurait pu utiliser.
Réponds UNIQUEMENT avec un objet JSON :
{{"score": 0.0 à 1.0, "retour": "2 ou 3 phrases : ce qui est juste, puis ce qui manque ou est faux, et la bonne réponse", "erreur": "type : détail (oubli, confusion, vocabulaire, incompréhension, méthode, calcul, consigne) ou null si juste"}}"#,
        matiere = q.matiere,
        niveau = q.niveau,
        enonce = q.enonce,
        support = q.support.as_deref().map(|s| format!("Support : {s}")).unwrap_or_default(),
        attendus = if q.attendus.is_empty() { q.reponses_acceptees.join(" / ") } else { q.attendus.iter().map(|a| format!("- {a}")).collect::<Vec<_>>().join("\n") },
        explication = q.explication,
    );
    let r = claude::demander(&prompt, "", None);
    let (score, retour, erreur) = if r.ok {
        let v = claude::extraire_json(&r.texte)?;
        (
            v["score"].as_f64().unwrap_or(0.0).clamp(0.0, 1.0),
            v["retour"].as_str().unwrap_or(&q.explication).to_string(),
            v["erreur"].as_str().map(String::from),
        )
    } else {
        // Correction impossible (Claude indisponible) : on donne la correction de référence sans pénaliser.
        (0.5, format!("Correction automatique indisponible ({}). La réponse attendue : {}", r.texte, q.explication), None)
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
            tolerance: None, unite: None, plan: vec![], attendus: vec![], erreurs: vec![], explication: "".into(),
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
