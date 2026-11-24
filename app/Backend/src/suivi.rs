//! Suivi des notions (forces / faiblesses, répétition espacée, priorités avant les évals),
//! sur la base SQLite. Port de l'ancien `suivi.py` du skill : mêmes règles, mêmes résultats.

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result, bail};
use chrono::{Duration, Local, NaiveDate};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Serialize;
use serde_json::{Value, json};

pub const NIVEAUX: [&str; 3] = ["savoir", "comprendre", "appliquer"];
/// Jours avant la prochaine révision après une bonne réponse (palier 0, 1, 2…).
const INTERVALLES: [i64; 6] = [1, 3, 7, 14, 30, 60];

fn aujourdhui() -> NaiveDate {
    Local::now().date_naive()
}

pub fn slug(texte: &str) -> String {
    let sans_accents: String = texte
        .chars()
        .map(|c| match c {
            'à' | 'â' | 'ä' | 'À' | 'Â' => 'a',
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' => 'e',
            'î' | 'ï' | 'Î' => 'i',
            'ô' | 'ö' | 'Ô' => 'o',
            'ù' | 'û' | 'ü' | 'Ù' | 'Û' => 'u',
            'ç' | 'Ç' => 'c',
            c => c,
        })
        .collect::<String>()
        .to_lowercase();
    let mots: Vec<String> = sans_accents
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|m| !m.is_empty())
        .map(String::from)
        .collect();
    if mots.is_empty() { "x".into() } else { mots.join("-") }
}

// ---------------------------------------------------------------- états

#[derive(Clone, Copy, PartialEq, Eq, Serialize, Debug)]
#[serde(rename_all = "snake_case")]
pub enum Etat {
    Vert,
    ARevoir,
    Orange,
    Rouge,
    NonTeste,
}

impl Etat {
    pub fn emoji(self) -> &'static str {
        match self {
            Etat::Vert => "🟢",
            Etat::ARevoir => "🟡",
            Etat::Orange => "🟠",
            Etat::Rouge => "🔴",
            Etat::NonTeste => "⚪",
        }
    }
    pub fn libelle(self) -> &'static str {
        match self {
            Etat::Vert => "maîtrisé",
            Etat::ARevoir => "à revoir",
            Etat::Orange => "en cours",
            Etat::Rouge => "à retravailler",
            Etat::NonTeste => "pas encore testé",
        }
    }
    fn faiblesse(self) -> f64 {
        match self {
            Etat::Rouge => 1.0,
            Etat::NonTeste => 0.8,
            Etat::Orange => 0.6,
            Etat::ARevoir => 0.45,
            Etat::Vert => 0.1,
        }
    }
}

#[derive(Clone, Serialize, serde::Deserialize)]
pub struct Essai {
    pub date: String,
    pub niveau: String,
    pub ok: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub erreur: Option<String>,
}

/// Même règle que suivi.py : les 4 derniers essais, les plus récents comptent plus ;
/// le vert demande au moins 2 niveaux réussis (une seule bonne réponse ne prouve rien).
pub fn etat_notion(essais: &[Essai], prochaine: Option<&str>, auj: NaiveDate) -> Etat {
    if essais.is_empty() {
        return Etat::NonTeste;
    }
    let recents = &essais[essais.len().saturating_sub(4)..];
    let (mut s, mut t) = (0.0, 0.0);
    for (i, e) in recents.iter().enumerate() {
        let p = (i + 1) as f64;
        t += p;
        if e.ok {
            s += p;
        }
    }
    let score = s / t;
    if score >= 0.8 && essais.last().unwrap().ok {
        let niveaux_ok: std::collections::HashSet<&str> =
            essais.iter().filter(|e| e.ok).map(|e| e.niveau.as_str()).collect();
        if niveaux_ok.len() < 2 {
            return Etat::Orange;
        }
        if prochaine.and_then(|p| NaiveDate::parse_from_str(p, "%Y-%m-%d").ok()).is_some_and(|p| p <= auj) {
            return Etat::ARevoir;
        }
        return Etat::Vert;
    }
    if score >= 0.5 { Etat::Orange } else { Etat::Rouge }
}

fn niveaux_faibles(essais: &[Essai]) -> Vec<String> {
    let mut dernier: BTreeMap<&str, bool> = BTreeMap::new();
    for e in essais {
        dernier.insert(&e.niveau, e.ok);
    }
    NIVEAUX.iter().filter(|n| dernier.get(*n) == Some(&false)).map(|n| n.to_string()).collect()
}

// ---------------------------------------------------------------- écriture

fn assurer_matiere(conn: &Connection, nom: &str) -> Result<String> {
    let id = slug(nom);
    conn.execute(
        "INSERT INTO matieres (id, nom) VALUES (?1, ?2) ON CONFLICT(id) DO UPDATE SET nom = excluded.nom",
        params![id, nom],
    )?;
    Ok(id)
}

/// Crée ou complète un chapitre et ses notions (`notions` : liste de « code:intitulé »).
pub fn chapitre(conn: &Connection, matiere: &str, code: &str, titre: Option<&str>, notions: &[String], source: Option<&str>) -> Result<String> {
    let mid = assurer_matiere(conn, matiere)?;
    conn.execute(
        "INSERT INTO chapitres (matiere_id, code, titre) VALUES (?1, ?2, ?3) ON CONFLICT(matiere_id, code) DO NOTHING",
        params![mid, code, titre.unwrap_or(code)],
    )?;
    if let Some(t) = titre {
        conn.execute("UPDATE chapitres SET titre = ?3 WHERE matiere_id = ?1 AND code = ?2", params![mid, code, t])?;
    }
    let (cid, sources): (i64, String) = conn.query_row(
        "SELECT id, sources FROM chapitres WHERE matiere_id = ?1 AND code = ?2",
        params![mid, code],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    if let Some(src) = source {
        let mut liste: Vec<String> = serde_json::from_str(&sources).unwrap_or_default();
        if !liste.iter().any(|s| s == src) {
            liste.push(src.to_string());
            conn.execute("UPDATE chapitres SET sources = ?2 WHERE id = ?1", params![cid, serde_json::to_string(&liste)?])?;
        }
    }
    for spec in notions {
        let (code_n, intitule) = spec.split_once(':').unwrap_or((spec, ""));
        let code_n = slug(code_n);
        let intitule = if intitule.trim().is_empty() { code_n.clone() } else { intitule.trim().to_string() };
        conn.execute(
            "INSERT INTO notions (chapitre_id, code, intitule) VALUES (?1, ?2, ?3)
             ON CONFLICT(chapitre_id, code) DO UPDATE SET intitule = excluded.intitule",
            params![cid, code_n, intitule],
        )?;
    }
    let nb: i64 = conn.query_row("SELECT COUNT(*) FROM notions WHERE chapitre_id = ?1", [cid], |r| r.get(0))?;
    let titre: String = conn.query_row("SELECT titre FROM chapitres WHERE id = ?1", [cid], |r| r.get(0))?;
    Ok(format!("✅ {matiere} / {titre} : {nb} notions"))
}

pub struct NouveauResultat<'a> {
    pub matiere: &'a str,
    pub chapitre: &'a str,
    pub notion: &'a str,
    pub niveau: &'a str,
    pub ok: bool,
    pub erreur: Option<&'a str>,
    pub question: Option<&'a str>,
    pub reponse: Option<&'a str>,
}

/// Enregistre une réponse et recalcule la prochaine révision (répétition espacée).
pub fn resultat(conn: &Connection, r: &NouveauResultat) -> Result<String> {
    if !NIVEAUX.contains(&r.niveau) {
        bail!("niveau inconnu : {} (attendu : savoir, comprendre ou appliquer)", r.niveau);
    }
    let code = slug(r.notion);
    let ligne: Option<(i64, i64, String)> = conn
        .query_row(
            "SELECT n.id, n.palier, n.intitule FROM notions n JOIN chapitres c ON c.id = n.chapitre_id
             WHERE c.matiere_id = ?1 AND c.code = ?2 AND n.code = ?3",
            params![slug(r.matiere), r.chapitre, code],
            |x| Ok((x.get(0)?, x.get(1)?, x.get(2)?)),
        )
        .optional()?;
    let Some((nid, palier, intitule)) = ligne else {
        bail!("notion inconnue : {}/{}/{} — crée d'abord le chapitre", r.matiere, r.chapitre, code)
    };
    let auj = aujourdhui();
    // Bonne réponse : on espace davantage ; erreur : on repart du début (revoir demain).
    let palier = if r.ok { (palier + 1).min(INTERVALLES.len() as i64 - 1) } else { -1 };
    let prochaine = auj + Duration::days(INTERVALLES[palier.max(0) as usize]);
    conn.execute(
        "INSERT INTO essais (notion_id, date, niveau, ok, erreur, question, reponse) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![nid, auj.to_string(), r.niveau, r.ok as i64, r.erreur, r.question, r.reponse],
    )?;
    conn.execute(
        "UPDATE notions SET palier = ?2, prochaine_revision = ?3 WHERE id = ?1",
        params![nid, palier, prochaine.to_string()],
    )?;
    let essais = essais_de(conn, nid)?;
    let etat = etat_notion(&essais, Some(&prochaine.to_string()), auj);
    Ok(format!("{} {} ({} {})", etat.emoji(), intitule, r.niveau, if r.ok { "✔" } else { "✘" }))
}

pub struct NouvelleEval<'a> {
    pub matiere: &'a str,
    pub date: &'a str,
    pub type_: Option<&'a str>,
    pub titre: Option<&'a str>,
    pub chapitres: Option<Vec<String>>,
    pub poids: Option<i64>,
    pub supprimer: bool,
}

pub fn eval(conn: &Connection, e: &NouvelleEval) -> Result<String> {
    NaiveDate::parse_from_str(e.date, "%Y-%m-%d").context("date attendue au format AAAA-MM-JJ")?;
    let mid = assurer_matiere(conn, e.matiere)?;
    if e.supprimer {
        conn.execute("DELETE FROM evals WHERE matiere_id = ?1 AND date = ?2", params![mid, e.date])?;
        return Ok("🗑 éval supprimée".into());
    }
    conn.execute("INSERT INTO evals (matiere_id, date) VALUES (?1, ?2) ON CONFLICT DO NOTHING", params![mid, e.date])?;
    if let Some(t) = e.type_ {
        conn.execute("UPDATE evals SET type = ?3 WHERE matiere_id = ?1 AND date = ?2", params![mid, e.date, t])?;
    }
    if let Some(t) = e.titre {
        conn.execute("UPDATE evals SET titre = ?3 WHERE matiere_id = ?1 AND date = ?2", params![mid, e.date, t])?;
    }
    if let Some(p) = e.poids {
        conn.execute("UPDATE evals SET poids = ?3 WHERE matiere_id = ?1 AND date = ?2", params![mid, e.date, p])?;
    }
    if let Some(c) = &e.chapitres {
        conn.execute(
            "UPDATE evals SET chapitres = ?3 WHERE matiere_id = ?1 AND date = ?2",
            params![mid, e.date, serde_json::to_string(c)?],
        )?;
    }
    let (t, titre, ch): (String, String, String) = conn.query_row(
        "SELECT type, titre, chapitres FROM evals WHERE matiere_id = ?1 AND date = ?2",
        params![mid, e.date],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    let ch: Vec<String> = serde_json::from_str(&ch).unwrap_or_default();
    Ok(format!(
        "✅ éval {} le {} : {t} {titre} — chapitres : {}",
        e.matiere,
        e.date,
        if ch.is_empty() { "à préciser".into() } else { ch.join(", ") }
    ))
}

// ---------------------------------------------------------------- lecture

fn essais_de(conn: &Connection, notion_id: i64) -> Result<Vec<Essai>> {
    let mut st = conn.prepare("SELECT date, niveau, ok, erreur FROM essais WHERE notion_id = ?1 ORDER BY id")?;
    let v = st
        .query_map([notion_id], |r| Ok(Essai { date: r.get(0)?, niveau: r.get(1)?, ok: r.get::<_, i64>(2)? == 1, erreur: r.get(3)? }))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Tout le suivi, dans la forme qu'attend l'interface (et qu'avaient les anciens fichiers JSON).
pub fn exporter(conn: &Connection) -> Result<Value> {
    let mut matieres = Vec::new();
    let mut st = conn.prepare("SELECT id, nom FROM matieres ORDER BY nom")?;
    let ms: Vec<(String, String)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    for (mid, nom) in &ms {
        let mut chapitres = serde_json::Map::new();
        let mut sc = conn.prepare("SELECT id, code, titre, sources FROM chapitres WHERE matiere_id = ?1 ORDER BY id")?;
        let cs: Vec<(i64, String, String, String)> =
            sc.query_map([mid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?.collect::<rusqlite::Result<_>>()?;
        for (cid, code, titre, sources) in cs {
            let mut notions = serde_json::Map::new();
            let mut sn = conn.prepare("SELECT id, code, intitule, palier, prochaine_revision FROM notions WHERE chapitre_id = ?1 ORDER BY id")?;
            let ns: Vec<(i64, String, String, i64, Option<String>)> = sn
                .query_map([cid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)))?
                .collect::<rusqlite::Result<_>>()?;
            for (nid, ncode, intitule, palier, prochaine) in ns {
                notions.insert(
                    ncode,
                    json!({ "intitule": intitule, "essais": essais_de(conn, nid)?, "palier": palier, "prochaine_revision": prochaine }),
                );
            }
            chapitres.insert(
                code,
                json!({ "titre": titre, "sources": serde_json::from_str::<Value>(&sources).unwrap_or(json!([])), "notions": notions }),
            );
        }
        if !chapitres.is_empty() {
            matieres.push(json!({ "matiere": nom, "chapitres": chapitres }));
        }
    }
    let mut se = conn.prepare(
        "SELECT m.nom, e.date, e.type, e.titre, e.chapitres, e.poids FROM evals e JOIN matieres m ON m.id = e.matiere_id ORDER BY e.date",
    )?;
    let evals: Vec<Value> = se
        .query_map([], |r| {
            Ok(json!({
                "matiere": r.get::<_, String>(0)?, "date": r.get::<_, String>(1)?, "type": r.get::<_, String>(2)?,
                "titre": r.get::<_, String>(3)?,
                "chapitres": serde_json::from_str::<Value>(&r.get::<_, String>(4)?).unwrap_or(json!([])),
                "poids": r.get::<_, i64>(5)?,
            }))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(json!({ "evals": evals, "matieres": matieres }))
}

#[derive(Serialize)]
pub struct LigneCarte {
    pub matiere: String,
    pub chapitre: String,
    pub titre_chapitre: String,
    pub notion: String,
    pub intitule: String,
    pub etat: Etat,
    pub niveaux_faibles: Vec<String>,
    pub derniere_erreur: Option<String>,
}

pub fn carte(conn: &Connection, matiere: Option<&str>, chapitre: Option<&str>) -> Result<Vec<LigneCarte>> {
    let export = exporter(conn)?;
    let auj = aujourdhui();
    let mut lignes = Vec::new();
    for m in export["matieres"].as_array().into_iter().flatten() {
        let nom = m["matiere"].as_str().unwrap_or_default();
        if matiere.is_some_and(|x| slug(x) != slug(nom)) {
            continue;
        }
        for (code, ch) in m["chapitres"].as_object().into_iter().flatten() {
            if chapitre.is_some_and(|c| c != code) {
                continue;
            }
            for (ncode, n) in ch["notions"].as_object().into_iter().flatten() {
                let essais: Vec<Essai> = serde_json::from_value(n["essais"].clone()).unwrap_or_default();
                lignes.push(LigneCarte {
                    matiere: nom.into(),
                    chapitre: code.clone(),
                    titre_chapitre: ch["titre"].as_str().unwrap_or_default().into(),
                    notion: ncode.clone(),
                    intitule: n["intitule"].as_str().unwrap_or_default().into(),
                    etat: etat_notion(&essais, n["prochaine_revision"].as_str(), auj),
                    niveaux_faibles: niveaux_faibles(&essais),
                    derniere_erreur: essais.iter().rev().find_map(|e| e.erreur.clone()),
                });
            }
        }
    }
    Ok(lignes)
}

#[derive(Serialize)]
pub struct Priorite {
    pub score: f64,
    pub matiere: String,
    pub chapitre: String,
    pub notion: String,
    pub intitule: String,
    pub etat: Etat,
    pub niveaux_faibles: Vec<String>,
    pub raisons: Vec<String>,
}

#[derive(Serialize)]
pub struct Priorites {
    pub priorites: Vec<Priorite>,
    pub evals_a_venir: Vec<Value>,
    pub evals_sans_contenu: Vec<Value>,
}

pub fn priorites(conn: &Connection, n: usize) -> Result<Priorites> {
    let export = exporter(conn)?;
    let auj = aujourdhui();
    let auj_s = auj.to_string();
    let evals: Vec<Value> =
        export["evals"].as_array().into_iter().flatten().filter(|e| e["date"].as_str() >= Some(auj_s.as_str())).cloned().collect();
    let matieres = export["matieres"].as_array().cloned().unwrap_or_default();
    let chapitres_connus = |m: &str, c: &str| {
        matieres.iter().any(|x| slug(x["matiere"].as_str().unwrap_or_default()) == slug(m) && x["chapitres"].get(c).is_some())
    };
    let sans_contenu: Vec<Value> = evals
        .iter()
        .filter(|e| {
            let ch = e["chapitres"].as_array().cloned().unwrap_or_default();
            ch.is_empty() || !ch.iter().all(|c| chapitres_connus(e["matiere"].as_str().unwrap_or_default(), c.as_str().unwrap_or_default()))
        })
        .cloned()
        .collect();

    let mut lignes = Vec::new();
    for l in carte(conn, None, None)? {
        let liee = evals.iter().find(|e| {
            slug(e["matiere"].as_str().unwrap_or_default()) == slug(&l.matiere)
                && e["chapitres"].as_array().is_some_and(|c| c.iter().any(|x| x.as_str() == Some(&l.chapitre)))
        });
        let mut score = l.etat.faiblesse();
        let mut raisons = vec![format!("{} {}", l.etat.emoji(), l.etat.libelle())];
        if let Some(e) = liee {
            let jours = (NaiveDate::parse_from_str(e["date"].as_str().unwrap_or_default(), "%Y-%m-%d")? - auj).num_days();
            score *= 1.0 + e["poids"].as_f64().unwrap_or(1.0) * 3.0 / (1.0 + jours as f64);
            raisons.push(format!("{} dans {jours} j", e["type"].as_str().unwrap_or("éval")));
        }
        let prochaine: Option<String> = conn
            .query_row(
                "SELECT n.prochaine_revision FROM notions n JOIN chapitres c ON c.id = n.chapitre_id WHERE c.matiere_id = ?1 AND c.code = ?2 AND n.code = ?3",
                params![slug(&l.matiere), l.chapitre, l.notion],
                |r| r.get(0),
            )
            .optional()?
            .flatten();
        let due = prochaine.as_deref().is_some_and(|p| p <= auj_s.as_str()) && l.etat != Etat::NonTeste;
        if due {
            score += 0.3;
            raisons.push("révision prévue aujourd'hui".into());
        }
        if l.etat == Etat::Vert && !due {
            continue;
        }
        lignes.push(Priorite {
            score: (score * 100.0).round() / 100.0,
            matiere: l.matiere,
            chapitre: l.chapitre,
            notion: l.notion,
            intitule: l.intitule,
            etat: l.etat,
            niveaux_faibles: l.niveaux_faibles,
            raisons,
        });
    }
    lignes.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    lignes.truncate(n);
    Ok(Priorites { priorites: lignes, evals_a_venir: evals, evals_sans_contenu: sans_contenu })
}

#[derive(Serialize)]
pub struct ContenuMatiere {
    pub matiere: String,
    pub cours: usize,
    pub docs: usize,
    pub caracteres: usize,
    pub dernier_cours: String,
    pub suivie: bool,
}

/// Matières classées selon la quantité de cours récupérée sur Pronote (textes + documents).
pub fn contenu(conn: &Connection, dossier: &Path) -> Result<Vec<ContenuMatiere>> {
    let sync: Value = serde_json::from_str(&std::fs::read_to_string(dossier.join("pronote").join("sync.json")).context("pas encore de synchro Pronote")?)?;
    let auj = aujourdhui().to_string();
    let mut stats: BTreeMap<String, ContenuMatiere> = BTreeMap::new();
    let suivies: Vec<String> = exporter(conn)?["matieres"].as_array().into_iter().flatten().filter_map(|m| m["matiere"].as_str().map(slug)).collect();
    for c in sync["cours"].as_array().into_iter().flatten() {
        let ct = &c["contenu"];
        let debut = c["debut"].as_str().unwrap_or_default();
        if ct.is_null() || debut.get(..10).unwrap_or_default() > auj.as_str() {
            continue;
        }
        let m = c["matiere"].as_str().unwrap_or("Autre").to_string();
        let s = stats.entry(m.clone()).or_insert(ContenuMatiere {
            suivie: suivies.contains(&slug(&m)),
            matiere: m,
            cours: 0,
            docs: 0,
            caracteres: 0,
            dernier_cours: String::new(),
        });
        s.cours += 1;
        s.caracteres += ct["titre"].as_str().unwrap_or_default().len() + ct["description"].as_str().unwrap_or_default().len();
        s.docs += ct["fichiers"].as_array().map_or(0, |f| f.iter().filter(|x| x.get("chemin").is_some()).count());
        if debut[..10] > *s.dernier_cours {
            s.dernier_cours = debut[..10].to_string();
        }
    }
    let mut v: Vec<ContenuMatiere> = stats.into_values().collect();
    v.sort_by_key(|s| std::cmp::Reverse(s.caracteres + 1500 * s.docs));
    Ok(v)
}

// ---------------------------------------------------------------- progression des révisions

#[derive(Serialize)]
pub struct Semaine {
    /// Lundi de la semaine (AAAA-MM-JJ).
    pub debut: String,
    pub questions: usize,
    pub reussies: usize,
}

#[derive(Serialize)]
pub struct StatsRevision {
    /// Les 6 dernières semaines, de la plus ancienne à la plus récente.
    pub semaines: Vec<Semaine>,
    pub total_questions: usize,
    pub total_reussies: usize,
    pub notions_total: usize,
    pub notions_maitrisees: usize,
}

pub fn stats_revision(conn: &Connection) -> Result<StatsRevision> {
    use chrono::Datelike;
    let auj = aujourdhui();
    let lundi = auj - Duration::days(auj.weekday().num_days_from_monday() as i64);
    let mut semaines: Vec<Semaine> = (0..6)
        .rev()
        .map(|i| Semaine { debut: (lundi - Duration::weeks(i)).to_string(), questions: 0, reussies: 0 })
        .collect();
    let mut st = conn.prepare("SELECT date, ok FROM essais")?;
    let essais: Vec<(String, i64)> = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    for (date, ok) in &essais {
        let Ok(d) = NaiveDate::parse_from_str(date, "%Y-%m-%d") else { continue };
        let l = (d - Duration::days(d.weekday().num_days_from_monday() as i64)).to_string();
        if let Some(s) = semaines.iter_mut().find(|s| s.debut == l) {
            s.questions += 1;
            s.reussies += *ok as usize;
        }
    }
    let lignes = carte(conn, None, None)?;
    Ok(StatsRevision {
        semaines,
        total_questions: essais.len(),
        total_reussies: essais.iter().filter(|e| e.1 == 1).count(),
        notions_total: lignes.len(),
        notions_maitrisees: lignes.iter().filter(|l| l.etat == Etat::Vert).count(),
    })
}

// ---------------------------------------------------------------- import des anciens fichiers JSON

/// Au premier lancement, reprend ce que le skill avait écrit en JSON (matieres/*.json, evals.json).
pub fn importer_json_si_vide(conn: &Connection, dossier: &Path) -> Result<usize> {
    let deja: i64 = conn.query_row("SELECT COUNT(*) FROM chapitres", [], |r| r.get(0))?;
    if deja > 0 {
        return Ok(0);
    }
    let mut n = 0;
    if let Ok(entrees) = std::fs::read_dir(dossier.join("matieres")) {
        for f in entrees.flatten().map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e == "json")) {
            let m: Value = serde_json::from_str(&std::fs::read_to_string(&f)?)?;
            let nom = m["matiere"].as_str().unwrap_or_default();
            for (code, ch) in m["chapitres"].as_object().into_iter().flatten() {
                let notions: Vec<String> = ch["notions"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(c, n)| format!("{c}:{}", n["intitule"].as_str().unwrap_or(c)))
                    .collect();
                chapitre(conn, nom, code, ch["titre"].as_str(), &notions, None)?;
                for s in ch["sources"].as_array().into_iter().flatten().filter_map(|s| s.as_str()) {
                    chapitre(conn, nom, code, None, &[], Some(s))?;
                }
                for (ncode, notion) in ch["notions"].as_object().into_iter().flatten() {
                    let nid: i64 = conn.query_row(
                        "SELECT n.id FROM notions n JOIN chapitres c ON c.id = n.chapitre_id WHERE c.matiere_id = ?1 AND c.code = ?2 AND n.code = ?3",
                        params![slug(nom), code, slug(ncode)],
                        |r| r.get(0),
                    )?;
                    for e in notion["essais"].as_array().into_iter().flatten() {
                        conn.execute(
                            "INSERT INTO essais (notion_id, date, niveau, ok, erreur) VALUES (?1, ?2, ?3, ?4, ?5)",
                            params![nid, e["date"].as_str(), e["niveau"].as_str(), e["ok"].as_bool().unwrap_or(false) as i64, e["erreur"].as_str()],
                        )?;
                    }
                    conn.execute(
                        "UPDATE notions SET palier = ?2, prochaine_revision = ?3 WHERE id = ?1",
                        params![nid, notion["palier"].as_i64().unwrap_or(-1), notion["prochaine_revision"].as_str()],
                    )?;
                }
                n += 1;
            }
        }
    }
    if let Ok(t) = std::fs::read_to_string(dossier.join("evals.json")) {
        for e in serde_json::from_str::<Value>(&t)?.as_array().into_iter().flatten() {
            eval(
                conn,
                &NouvelleEval {
                    matiere: e["matiere"].as_str().unwrap_or_default(),
                    date: e["date"].as_str().unwrap_or_default(),
                    type_: e["type"].as_str(),
                    titre: e["titre"].as_str(),
                    chapitres: e["chapitres"].as_array().map(|c| c.iter().filter_map(|x| x.as_str().map(String::from)).collect()),
                    poids: e["poids"].as_i64(),
                    supprimer: false,
                },
            )?;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Connection {
        crate::db::ouvrir_memoire().unwrap()
    }

    fn e(niveau: &str, ok: bool) -> Essai {
        Essai { date: "2026-01-01".into(), niveau: niveau.into(), ok, erreur: None }
    }

    #[test]
    fn etats() {
        let auj = NaiveDate::from_ymd_opt(2026, 11, 24).unwrap();
        assert_eq!(etat_notion(&[], None, auj), Etat::NonTeste);
        assert_eq!(etat_notion(&[e("savoir", true)], None, auj), Etat::Orange, "un seul niveau ne suffit pas");
        assert_eq!(etat_notion(&[e("savoir", true), e("appliquer", true)], Some("2026-12-01"), auj), Etat::Vert);
        assert_eq!(etat_notion(&[e("savoir", true), e("appliquer", true)], Some("2026-11-20"), auj), Etat::ARevoir);
        assert_eq!(etat_notion(&[e("appliquer", false)], None, auj), Etat::Rouge);
    }

    #[test]
    fn parcours_complet() {
        let c = base();
        chapitre(&c, "SCIENCES VIE & TERRE", "genetique", Some("Génétique"), &["adn:Structure de l'ADN".into(), "mutations:Les mutations".into()], Some("test"))
            .unwrap();
        let auj = aujourdhui() + Duration::days(3);
        eval(&c, &NouvelleEval { matiere: "SCIENCES VIE & TERRE", date: &auj.to_string(), type_: Some("DS"), titre: None, chapitres: Some(vec!["genetique".into()]), poids: Some(2), supprimer: false })
            .unwrap();
        let r = NouveauResultat { matiere: "SCIENCES VIE & TERRE", chapitre: "genetique", notion: "mutations", niveau: "appliquer", ok: false, erreur: Some("confusion : mutation vs allèle"), question: None, reponse: None };
        assert!(resultat(&c, &r).unwrap().starts_with("🔴"));
        let p = priorites(&c, 8).unwrap();
        assert_eq!(p.priorites[0].notion, "mutations");
        assert!(p.evals_sans_contenu.is_empty());
        let export = exporter(&c).unwrap();
        assert_eq!(export["matieres"][0]["chapitres"]["genetique"]["notions"]["mutations"]["essais"][0]["erreur"], "confusion : mutation vs allèle");
    }

    #[test]
    fn slugs() {
        assert_eq!(slug("SC. ECONO.& SOCIALES"), "sc-econo-sociales");
        assert_eq!(slug("Éducation"), "education");
    }
}
