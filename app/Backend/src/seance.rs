//! Séances de révision dans l'app : Claude rédige les QCM en suivant les instructions du skill
//! `revisions` (SKILL.md + references/pedagogie.md), l'app les affiche et enregistre les réponses
//! dans le même suivi que le skill.

use std::path::Path;

use anyhow::{Context, Result, bail};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{claude, config, suivi};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Question {
    pub chapitre: String,
    pub notion: String,
    pub niveau: String,
    pub enonce: String,
    /// Document, tableau, extrait ou calcul à avoir sous les yeux (facultatif).
    #[serde(default)]
    pub support: Option<String>,
    pub choix: Vec<String>,
    /// Index de la bonne réponse (0 à 3).
    pub bonne: usize,
    /// Pour chaque choix : l'erreur qu'il trahit (« confusion : … »), `null` pour la bonne.
    #[serde(default)]
    pub erreurs: Vec<Option<String>>,
    pub explication: String,
}

#[derive(Serialize)]
pub struct Test {
    pub matiere: String,
    pub questions: Vec<Question>,
    /// Chapitres créés ou complétés pendant la préparation (messages pour l'élève).
    pub chapitres: Vec<String>,
}

#[derive(Deserialize)]
struct ChapitrePropose {
    code: String,
    titre: String,
    notions: Vec<NotionProposee>,
}

#[derive(Deserialize)]
struct NotionProposee {
    code: String,
    intitule: String,
}

fn lire(chemin: &Path) -> String {
    std::fs::read_to_string(chemin).unwrap_or_default()
}

/// Les cours de cette matière récupérés sur Pronote (les 3 dernières semaines), en texte court.
fn contexte_pronote(dossier: &Path, matiere: &str) -> (Value, Vec<String>) {
    let sync: Value = serde_json::from_str(&lire(&dossier.join("pronote").join("sync.json"))).unwrap_or(Value::Null);
    let meme = |v: &Value| suivi::slug(v["matiere"].as_str().unwrap_or_default()) == suivi::slug(matiere);
    let mut docs = Vec::new();
    let cours: Vec<Value> = sync["cours"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|c| meme(c) && !c["contenu"].is_null())
        .map(|c| {
            for f in c["contenu"]["fichiers"].as_array().into_iter().flatten() {
                if let Some(p) = f["chemin"].as_str() {
                    docs.push(p.to_string());
                }
            }
            json!({ "date": &c["debut"].as_str().unwrap_or_default()[..10], "titre": c["contenu"]["titre"], "description": c["contenu"]["description"] })
        })
        .collect();
    let devoirs: Vec<Value> = sync["devoirs"].as_array().into_iter().flatten().filter(|d| meme(d)).map(|d| json!({ "pour_le": d["pour_le"], "description": d["description"] })).collect();
    docs.sort();
    docs.dedup();
    (json!({ "classe": sync["eleve"]["classe"], "cours": cours, "devoirs": devoirs }), docs)
}

/// Prépare un test de compétences (QCM) sur une matière, avec les instructions du skill.
pub fn preparer_test(conn: &Connection, matiere: &str, nb_questions: usize) -> Result<Test> {
    let dossier = config::dossier_donnees();
    let skill = config::dossier_skill();
    let (pronote, docs) = contexte_pronote(&dossier, matiere);
    let notions_suivies: Vec<_> = suivi::carte(conn, Some(matiere), None)?
        .into_iter()
        .map(|l| json!({ "chapitre": l.chapitre, "titre_chapitre": l.titre_chapitre, "notion": l.notion, "intitule": l.intitule, "etat": l.etat, "derniere_erreur": l.derniere_erreur }))
        .collect();
    let evals = suivi::priorites(conn, 0)?.evals_a_venir;
    let eval = evals.iter().find(|e| suivi::slug(e["matiere"].as_str().unwrap_or_default()) == suivi::slug(matiere));

    let prompt = format!(
        r#"Tu prépares un test de compétences pour un lycéen, dans son app de révision.
Suis les instructions pédagogiques du skill « revisions » ci-dessous. Elles ont été écrites pour une
conversation ; ici c'est l'app qui affiche les questions (en grand, 4 boutons) et enregistre les réponses :
toi, tu rédiges seulement le contenu du test.

===== SKILL.md =====
{skill_md}

===== references/pedagogie.md =====
{pedagogie}

===== Contexte =====
Matière : {matiere}
Évaluation à venir : {eval}
Notions déjà suivies pour cette matière (réutilise exactement ces codes de chapitre et de notion) :
{notions}

Cours récupérés sur Pronote (cahier de textes) :
{pronote}

Documents des profs pour cette matière (tu peux les lire avec l'outil Read si utile) :
{docs}

===== Ta tâche =====
1. Si les notions suivies ne couvrent pas les cours récents, découpe les chapitres manquants en 4 à 8
   notions (section 2 du skill), avec des codes courts en minuscules et tirets.
2. Rédige {n} questions QCM à 4 choix qui couvrent les notions (priorité aux chapitres de l'éval s'il y en a
   une, puis aux notions jamais testées ou en difficulté), en mélangeant savoir / comprendre / appliquer
   comme le décrit la section 4 et le guide de pédagogie. Les mauvaises réponses sont des erreurs réelles.
3. Réponds UNIQUEMENT avec un objet JSON, sans texte autour, de cette forme exacte :
{{
  "chapitres": [{{"code": "photosynthese", "titre": "La photosynthèse", "notions": [{{"code": "bilan", "intitule": "Bilan de la photosynthèse"}}]}}],
  "questions": [{{
    "chapitre": "photosynthese", "notion": "bilan", "niveau": "comprendre",
    "enonce": "…", "support": null,
    "choix": ["…", "…", "…", "…"], "bonne": 2,
    "erreurs": ["confusion : respiration vs photosynthèse", "oubli : …", null, "incompréhension : …"],
    "explication": "Correction en 1 à 3 phrases."
  }}]
}}
"chapitres" ne contient que les chapitres nouveaux ou complétés ([] sinon). "bonne" est l'index (0 à 3)
de la bonne réponse, à varier d'une question à l'autre. "erreurs" a 4 éléments, null pour la bonne réponse."#,
        skill_md = lire(&skill.join("SKILL.md")),
        pedagogie = lire(&skill.join("references").join("pedagogie.md")),
        eval = eval.map_or("aucune : révision de cours".into(), |e| format!("{} le {} ({})", e["type"], e["date"], e["titre"])),
        notions = serde_json::to_string_pretty(&notions_suivies)?,
        pronote = serde_json::to_string_pretty(&pronote)?,
        docs = if docs.is_empty() { "(aucun)".into() } else { docs.join("\n") },
        n = nb_questions,
    );

    let reponse = claude::demander(&prompt, "Read", Some(&dossier));
    if !reponse.ok {
        bail!("{}", reponse.texte);
    }
    let v = claude::extraire_json(&reponse.texte)?;

    let mut messages = Vec::new();
    for ch in serde_json::from_value::<Vec<ChapitrePropose>>(v["chapitres"].clone()).unwrap_or_default() {
        let notions: Vec<String> = ch.notions.iter().map(|n| format!("{}:{}", n.code, n.intitule)).collect();
        let source = format!("séance app du {}", chrono::Local::now().format("%d/%m/%Y"));
        messages.push(suivi::chapitre(conn, matiere, &ch.code, Some(&ch.titre), &notions, Some(&source))?);
    }

    let questions: Vec<Question> = serde_json::from_value(v["questions"].clone()).context("questions illisibles")?;
    let connues: Vec<(String, String)> = suivi::carte(conn, Some(matiere), None)?.into_iter().map(|l| (l.chapitre, l.notion)).collect();
    let valides: Vec<Question> = questions
        .into_iter()
        .filter(|q| q.choix.len() == 4 && q.bonne < 4 && suivi::NIVEAUX.contains(&q.niveau.as_str()))
        .filter(|q| connues.contains(&(q.chapitre.clone(), suivi::slug(&q.notion))))
        .collect();
    if valides.is_empty() {
        bail!("Claude n'a pas produit de questions utilisables, réessaie.");
    }
    Ok(Test { matiere: matiere.to_string(), questions: valides, chapitres: messages })
}

/// Enregistre la réponse de l'élève (`choix` = None pour « je sais pas »). Renvoie vrai si c'est juste.
pub fn enregistrer_reponse(conn: &Connection, matiere: &str, q: &Question, choix: Option<usize>) -> Result<bool> {
    let ok = choix == Some(q.bonne);
    let erreur = match choix {
        _ if ok => None,
        None => Some(format!("oubli : ne savait pas ({})", q.enonce.chars().take(80).collect::<String>())),
        Some(i) => Some(
            q.erreurs
                .get(i)
                .cloned()
                .flatten()
                .unwrap_or_else(|| format!("a choisi « {} »", q.choix.get(i).cloned().unwrap_or_default())),
        ),
    };
    let reponse = match choix {
        Some(i) => q.choix.get(i).cloned().unwrap_or_default(),
        None => "je sais pas".into(),
    };
    suivi::resultat(
        conn,
        &suivi::NouveauResultat {
            matiere,
            chapitre: &q.chapitre,
            notion: &q.notion,
            niveau: &q.niveau,
            ok,
            erreur: erreur.as_deref(),
            question: Some(&serde_json::to_string(q)?),
            reponse: Some(&reponse),
        },
    )?;
    Ok(ok)
}
