//! Moyennes : progression par trimestre et au fil du temps, et conseils de Claude
//! sur les matières où l'élève peut gagner le plus de points.

use std::collections::BTreeMap;

use anyhow::{Result, bail};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{claude, config, suivi};

const GENERALE: &str = "__generale__";

pub fn nombre(v: &Value) -> Option<f64> {
    let s = v.as_str()?.replace(',', ".");
    s.trim().parse().ok()
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

// ---------------------------------------------------------------- historique

/// Garde une photo des moyennes de la synchro (une par jour, la dernière du jour gagne).
pub fn enregistrer_instantane(conn: &Connection, sync: &Value) -> Result<usize> {
    let jour = sync["synchronise_le"].as_str().and_then(|s| s.get(..10)).map(String::from).unwrap_or_else(|| Local::now().date_naive().to_string());
    let mut n = 0;
    let mut ecrire = |periode: &str, matiere: &str, eleve: Option<f64>, classe: Option<f64>| -> Result<()> {
        if eleve.is_none() {
            return Ok(());
        }
        conn.execute(
            "INSERT INTO historique_moyennes (jour, periode, matiere, eleve, classe) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(jour, periode, matiere) DO UPDATE SET eleve = excluded.eleve, classe = excluded.classe",
            params![jour, periode, matiere, eleve, classe],
        )?;
        n += 1;
        Ok(())
    };
    for m in sync["moyennes"].as_array().into_iter().flatten() {
        ecrire(m["periode"].as_str().unwrap_or_default(), m["matiere"].as_str().unwrap_or_default(), nombre(&m["eleve"]), nombre(&m["classe"]))?;
    }
    for g in sync["moyennes_generales"].as_array().into_iter().flatten() {
        ecrire(g["periode"].as_str().unwrap_or_default(), GENERALE, nombre(&g["eleve"]), nombre(&g["classe"]))?;
    }
    Ok(n)
}

// ---------------------------------------------------------------- lecture

#[derive(Serialize, Clone)]
pub struct ValeurPeriode {
    pub periode: String,
    pub eleve: Option<f64>,
    pub classe: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct PointHistorique {
    pub jour: String,
    pub eleve: f64,
}

#[derive(Serialize, Clone)]
pub struct Ligne {
    pub matiere: String,
    /// Une valeur par trimestre (dans l'ordre de l'année), `null` si pas encore de note.
    pub periodes: Vec<ValeurPeriode>,
    /// Moyenne affichée : celle du trimestre en cours, ou du dernier trimestre noté.
    pub actuelle: Option<f64>,
    pub periode_actuelle: Option<String>,
    pub classe: Option<f64>,
    /// Différence avec le trimestre précédent (positive = progrès).
    pub evolution: Option<f64>,
    /// Photos successives pendant le trimestre affiché (progression au fil des semaines).
    pub historique: Vec<PointHistorique>,
}

#[derive(Serialize)]
pub struct Moyennes {
    pub periodes: Vec<String>,
    pub periode_courante: Option<String>,
    pub generale: Ligne,
    /// Vrai si Pronote ne donne pas la moyenne générale et qu'on l'a calculée (moyenne simple).
    pub generale_calculee: bool,
    pub matieres: Vec<Ligne>,
    pub au_dessus_classe: usize,
}

pub fn lire(conn: &Connection) -> Result<Option<Moyennes>> {
    let dossier = config::dossier_donnees();
    let Ok(t) = std::fs::read_to_string(dossier.join("pronote").join("sync.json")) else { return Ok(None) };
    let sync: Value = serde_json::from_str(&t)?;

    // Ordre des trimestres : celui de Pronote ; seulement ceux qui ont des moyennes, plus le trimestre en cours.
    let courante = sync["periode"]["nom"].as_str().map(String::from);
    let mut ordre: Vec<String> = sync["periodes"].as_array().into_iter().flatten().filter_map(|p| p["nom"].as_str().map(String::from)).collect();
    for m in sync["moyennes"].as_array().into_iter().flatten() {
        if let Some(p) = m["periode"].as_str() {
            if !ordre.iter().any(|o| o == p) {
                ordre.push(p.into());
            }
        }
    }
    let avec_notes: Vec<String> = ordre
        .iter()
        .filter(|p| sync["moyennes"].as_array().into_iter().flatten().any(|m| m["periode"].as_str() == Some(p.as_str()) && nombre(&m["eleve"]).is_some()) || courante.as_deref() == Some(p.as_str()))
        .filter(|p| p.to_lowercase().starts_with("trimestre") || p.to_lowercase().starts_with("semestre"))
        .cloned()
        .collect();

    let mut par_matiere: BTreeMap<String, BTreeMap<String, (Option<f64>, Option<f64>)>> = BTreeMap::new();
    for m in sync["moyennes"].as_array().into_iter().flatten() {
        let (Some(mat), Some(per)) = (m["matiere"].as_str(), m["periode"].as_str()) else { continue };
        par_matiere.entry(mat.into()).or_default().insert(per.into(), (nombre(&m["eleve"]), nombre(&m["classe"])));
    }

    let construire = |matiere: &str, valeurs: &BTreeMap<String, (Option<f64>, Option<f64>)>| -> Result<Ligne> {
        let periodes: Vec<ValeurPeriode> = avec_notes
            .iter()
            .map(|p| {
                let (e, c) = valeurs.get(p).cloned().unwrap_or((None, None));
                ValeurPeriode { periode: p.clone(), eleve: e, classe: c }
            })
            .collect();
        let notees: Vec<&ValeurPeriode> = periodes.iter().filter(|v| v.eleve.is_some()).collect();
        let derniere = notees.last();
        let avant = if notees.len() >= 2 { notees.get(notees.len() - 2) } else { None };
        let periode_actuelle = derniere.map(|v| v.periode.clone());
        let mut historique = Vec::new();
        if let Some(p) = &periode_actuelle {
            let mut st = conn.prepare("SELECT jour, eleve FROM historique_moyennes WHERE periode = ?1 AND matiere = ?2 AND eleve IS NOT NULL ORDER BY jour")?;
            historique = st.query_map(params![p, matiere], |r| Ok(PointHistorique { jour: r.get(0)?, eleve: r.get(1)? }))?.collect::<rusqlite::Result<_>>()?;
        }
        Ok(Ligne {
            matiere: matiere.into(),
            actuelle: derniere.and_then(|v| v.eleve),
            classe: derniere.and_then(|v| v.classe),
            evolution: match (derniere.and_then(|v| v.eleve), avant.and_then(|v| v.eleve)) {
                (Some(a), Some(b)) => Some(arrondi(a - b)),
                _ => None,
            },
            periode_actuelle,
            periodes,
            historique,
        })
    };

    let matieres: Vec<Ligne> = par_matiere.iter().map(|(m, v)| construire(m, v)).collect::<Result<_>>()?;

    // Moyenne générale : celle de Pronote si elle existe, sinon la moyenne simple des matières.
    let mut gen: BTreeMap<String, (Option<f64>, Option<f64>)> = BTreeMap::new();
    for g in sync["moyennes_generales"].as_array().into_iter().flatten() {
        if let Some(p) = g["periode"].as_str() {
            gen.insert(p.into(), (nombre(&g["eleve"]), nombre(&g["classe"])));
        }
    }
    let calculee = gen.is_empty();
    if calculee {
        for p in &avec_notes {
            let (mut se, mut ne, mut sc, mut nc) = (0.0, 0, 0.0, 0);
            for v in par_matiere.values() {
                if let Some((e, c)) = v.get(p) {
                    if let Some(e) = e { se += e; ne += 1; }
                    if let Some(c) = c { sc += c; nc += 1; }
                }
            }
            if ne > 0 {
                gen.insert(p.clone(), (Some(arrondi(se / ne as f64)), (nc > 0).then(|| arrondi(sc / nc as f64))));
            }
        }
    }
    let generale = construire(GENERALE, &gen)?;
    let au_dessus = matieres.iter().filter(|l| matches!((l.actuelle, l.classe), (Some(a), Some(c)) if a >= c)).count();
    Ok(Some(Moyennes { periodes: avec_notes, periode_courante: courante, generale, generale_calculee: calculee, matieres, au_dessus_classe: au_dessus }))
}

// ---------------------------------------------------------------- conseils de Claude

#[derive(Serialize, Deserialize, Clone)]
pub struct ConseilMatiere {
    pub matiere: String,
    /// 1 = à faire remonter en priorité, 2 = à surveiller, 3 = à entretenir.
    pub priorite: u8,
    /// Points de moyenne gagnables d'ici la fin du trimestre, selon Claude.
    pub potentiel: f64,
    pub objectif: Option<f64>,
    pub pourquoi: String,
    pub a_reviser: String,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct Analyse {
    pub cree_le: String,
    /// Mot d'encouragement global, concret.
    pub message: String,
    pub matieres: Vec<ConseilMatiere>,
}

pub fn derniere_analyse(conn: &Connection) -> Result<Option<Analyse>> {
    let t: Option<String> = conn.query_row("SELECT contenu FROM analyses_moyennes ORDER BY id DESC LIMIT 1", [], |r| r.get(0)).optional()?;
    Ok(t.and_then(|t| serde_json::from_str(&t).ok()))
}

/// Demande à Claude d'estimer où l'élève peut gagner le plus de points, et quoi réviser.
pub fn analyser(conn: &Connection) -> Result<Analyse> {
    let Some(moy) = lire(conn)? else { bail!("Pas encore de moyennes : synchronise Pronote d'abord.") };
    let dossier = config::dossier_donnees();
    let sync: Value = serde_json::from_str(&std::fs::read_to_string(dossier.join("pronote").join("sync.json"))?)?;
    let notes_recentes: Vec<Value> = sync["notes"].as_array().into_iter().flatten().take(25).cloned().collect();
    let notions: Vec<Value> = suivi::carte(conn, None, None)?
        .into_iter()
        .map(|l| json!({ "matiere": l.matiere, "notion": l.intitule, "etat": l.etat, "derniere_erreur": l.derniere_erreur }))
        .collect();
    let evals = suivi::priorites(conn, 0)?.evals_a_venir;
    let noms: Vec<&str> = moy.matieres.iter().map(|l| l.matiere.as_str()).collect();

    let prompt = format!(
        r#"Tu es le coach de révision d'un lycéen. Analyse ses moyennes et dis-lui où il peut gagner le plus
de points, pour qu'il révise en priorité les bonnes matières. Il veut rester motivé : sois honnête mais
encourageant, sans en faire trop.

Moyennes (par trimestre, élève et classe ; « historique » = progression pendant le trimestre) :
{moyennes}

Notes récentes (le détail, pour voir la tendance) :
{notes}

Notions suivies dans ses révisions et leur état (vert = maîtrisé, rouge = à retravailler) :
{notions}

Évaluations à venir : {evals}

Raisonne sur le potentiel réel : une matière très en dessous de la classe ou de son niveau habituel peut
remonter vite avec du travail ciblé ; une matière déjà très haute rapporte peu de points en plus.
Tiens compte des évals proches (occasions de remonter la moyenne).

Réponds UNIQUEMENT avec un objet JSON, sans texte autour :
{{
  "message": "2 phrases max : un constat positif réel, puis la priorité du moment",
  "matieres": [{{
    "matiere": "nom EXACT parmi : {noms}",
    "priorite": 1,
    "potentiel": 2.5,
    "objectif": 9.0,
    "pourquoi": "1 phrase : ce qui fait perdre des points ou ce qui permet d'en gagner",
    "a_reviser": "1 phrase : quoi réviser concrètement en premier"
  }}]
}}
"priorite" : 1 = à faire remonter en priorité, 2 = à surveiller, 3 = à entretenir. Mets toutes les matières,
triées par priorité puis par potentiel décroissant. "potentiel" = points de moyenne réalistes à gagner d'ici la
fin du trimestre. "objectif" = moyenne visée réaliste pour la fin du trimestre."#,
        moyennes = serde_json::to_string_pretty(&json!({ "generale": moy.generale, "matieres": moy.matieres, "trimestre_en_cours": moy.periode_courante }))?,
        notes = serde_json::to_string_pretty(&notes_recentes)?,
        notions = serde_json::to_string_pretty(&notions)?,
        evals = serde_json::to_string(&evals)?,
        noms = noms.join(" | "),
    );

    let r = claude::demander(&prompt, "", None);
    if !r.ok {
        bail!("{}", r.texte);
    }
    let v = claude::extraire_json(&r.texte)?;
    let mut conseils: Vec<ConseilMatiere> = serde_json::from_value(v["matieres"].clone())?;
    conseils.retain(|c| noms.contains(&c.matiere.as_str()));
    conseils.sort_by(|a, b| a.priorite.cmp(&b.priorite).then(b.potentiel.partial_cmp(&a.potentiel).unwrap_or(std::cmp::Ordering::Equal)));
    let analyse = Analyse {
        cree_le: Local::now().format("%Y-%m-%dT%H:%M").to_string(),
        message: v["message"].as_str().unwrap_or_default().to_string(),
        matieres: conseils,
    };
    conn.execute("INSERT INTO analyses_moyennes (cree_le, contenu) VALUES (?1, ?2)", params![analyse.cree_le, serde_json::to_string(&analyse)?])?;
    Ok(analyse)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nombres_pronote() {
        assert_eq!(nombre(&json!("16,95")), Some(16.95));
        assert_eq!(nombre(&json!("Absent")), None);
    }

    #[test]
    fn instantane() {
        let c = crate::db::ouvrir_memoire().unwrap();
        let sync = json!({
            "synchronise_le": "2026-11-24T19:52",
            "moyennes": [{ "periode": "Trimestre 1", "matiere": "ANGLAIS LV1", "eleve": "6,27", "classe": "9,59" }],
            "moyennes_generales": [],
        });
        assert_eq!(enregistrer_instantane(&c, &sync).unwrap(), 1);
        assert_eq!(enregistrer_instantane(&c, &sync).unwrap(), 1, "même jour : remplacé, pas dupliqué");
        let n: i64 = c.query_row("SELECT COUNT(*) FROM historique_moyennes", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }
}
