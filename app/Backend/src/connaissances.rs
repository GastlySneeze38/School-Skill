//! La base de connaissances : tout ce que l'IA peut lire sur les cours de l'élève, rangé pour qu'elle y pioche
//! vite et juste.
//!
//! 1. **Sources** (à chaque synchro, sans IA sauf l'OCR) : cahier de textes, devoirs, documents des profs.
//!    PDF → texte en Rust ; images → OCR (GLM-OCR). On ne refait le travail que si la source a changé.
//! 2. **Passages** de 100 à 300 mots, indexés deux fois : plein texte (FTS5, sans accents) et par le sens
//!    (embeddings sur le CPU). La recherche combine les deux (fusion des rangs).
//! 3. **Fiches de leçon** : écrites une fois par l'IA à partir du cours et de ses documents (notions, exemples,
//!    personnages, dates, textes étudiés, méthodes). C'est la fiche qu'on donne au modèle, pas le cours brut :
//!    le contexte reste court et précis.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::Local;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::{config, ia, suivi};

fn maintenant() -> String {
    Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn empreinte(t: &str) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    t.hash(&mut h);
    format!("{:016x}", h.finish())
}

// ---------------------------------------------------------------- extraction de texte

fn texte_pdf(chemin: &Path) -> String {
    // pdf-extract peut paniquer sur des PDF abîmés : un document illisible ne doit pas tout arrêter.
    let c = chemin.to_path_buf();
    std::panic::catch_unwind(move || pdf_extract::extract_text(&c).unwrap_or_default()).unwrap_or_default()
}

fn est_image(chemin: &Path) -> bool {
    matches!(
        chemin.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()).as_deref(),
        Some("jpg" | "jpeg" | "png" | "webp" | "bmp")
    )
}

/// Texte d'un document du prof. `None` si le format n'est pas (encore) lu (.doc, .odt…).
fn texte_document(chemin: &Path) -> Option<String> {
    let ext = chemin.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase())?;
    let brut = match ext.as_str() {
        "pdf" => texte_pdf(chemin),
        "txt" | "md" => std::fs::read_to_string(chemin).ok()?,
        _ if est_image(chemin) => ia::ocr(chemin).ok()?,
        _ => return None,
    };
    let propre = nettoyer(&brut);
    (propre.chars().filter(|c| c.is_alphanumeric()).count() >= 40).then_some(propre)
}

/// Espaces et lignes vides en trop (les PDF en sont pleins).
fn nettoyer(t: &str) -> String {
    let mut sortie = String::new();
    let mut vides = 0;
    for ligne in t.lines() {
        let l = ligne.split_whitespace().collect::<Vec<_>>().join(" ");
        if l.is_empty() {
            vides += 1;
            continue;
        }
        if !sortie.is_empty() {
            sortie.push_str(if vides > 0 { "\n\n" } else { "\n" });
        }
        sortie.push_str(&l);
        vides = 0;
    }
    sortie
}

/// Découpe en passages de ~120 à 300 mots, sans couper les paragraphes quand c'est possible.
pub fn decouper(texte: &str) -> Vec<String> {
    const MIN: usize = 120;
    const MAX: usize = 300;
    let mut morceaux: Vec<String> = Vec::new();
    for para in texte.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        let mots: Vec<&str> = para.split_whitespace().collect();
        if mots.len() <= MAX {
            morceaux.push(para.to_string());
        } else {
            for bloc in mots.chunks(MAX) {
                morceaux.push(bloc.join(" "));
            }
        }
    }
    let mut passages: Vec<String> = Vec::new();
    let mut courant = String::new();
    for m in morceaux {
        let n_courant = courant.split_whitespace().count();
        let n_m = m.split_whitespace().count();
        if !courant.is_empty() && (n_courant >= MIN || n_courant + n_m > MAX) {
            passages.push(std::mem::take(&mut courant));
        }
        if !courant.is_empty() {
            courant.push_str("\n\n");
        }
        courant.push_str(&m);
    }
    if !courant.is_empty() {
        passages.push(courant);
    }
    passages
}

// ---------------------------------------------------------------- mise à jour depuis Pronote

struct NouvelleSource {
    cle: String,
    parent: Option<String>,
    matiere: String,
    type_: &'static str,
    titre: String,
    date: Option<String>,
    chemin: Option<String>,
}

#[derive(Serialize, Default, Debug)]
pub struct Resume {
    pub sources: usize,
    pub nouvelles: usize,
    pub passages: usize,
    pub vecteurs: usize,
    pub illisibles: Vec<String>,
}

fn cle_cours(c: &Value) -> String {
    format!("cours:{}:{}", c["debut"].as_str().unwrap_or(""), suivi::slug(c["matiere"].as_str().unwrap_or("")))
}

/// Enregistre (ou met à jour) une source et ses passages. Renvoie vrai si elle est nouvelle ou a changé.
fn enregistrer(c: &Connection, s: &NouvelleSource, texte: &str, emp: &str) -> Result<bool> {
    let existant: Option<(i64, String)> =
        c.query_row("SELECT id, empreinte FROM sources WHERE cle = ?1", [&s.cle], |r| Ok((r.get(0)?, r.get(1)?))).optional()?;
    if let Some((_, e)) = &existant {
        if e == emp {
            return Ok(false);
        }
    }
    let tx = c.unchecked_transaction()?;
    if let Some((id, _)) = existant {
        tx.execute("DELETE FROM sources WHERE id = ?1", [id])?; // passages et fiche suivent (CASCADE)
    }
    tx.execute(
        "INSERT INTO sources (cle, parent, matiere, type, titre, date, chemin, texte, empreinte, ajoute_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![s.cle, s.parent, s.matiere, s.type_, s.titre, s.date, s.chemin, texte, emp, maintenant()],
    )?;
    let id = tx.last_insert_rowid();
    for (i, p) in decouper(texte).iter().enumerate() {
        tx.execute("INSERT INTO passages (source_id, matiere, ordre, texte) VALUES (?1, ?2, ?3, ?4)", params![id, s.matiere, i as i64, p])?;
    }
    tx.commit()?;
    Ok(true)
}

/// Lit la dernière synchro Pronote et met la base à jour (incrémental : rapide si rien n'a changé).
pub fn mettre_a_jour(c: &Connection) -> Result<Resume> {
    let dossier = config::dossier_donnees();
    let sync: Value = serde_json::from_str(&std::fs::read_to_string(dossier.join("pronote").join("sync.json")).context("pas encore de synchro Pronote")?)?;
    let mut r = Resume::default();
    let mut fichiers: Vec<(NouvelleSource, String)> = Vec::new();

    for cours in sync["cours"].as_array().into_iter().flatten() {
        if cours["annule"] == true || cours["contenu"].is_null() {
            continue;
        }
        let matiere = cours["matiere"].as_str().unwrap_or("").to_string();
        let titre = cours["contenu"]["titre"].as_str().unwrap_or("").trim().to_string();
        let desc = cours["contenu"]["description"].as_str().unwrap_or("").trim().to_string();
        let date = cours["debut"].as_str().and_then(|d| d.get(..10)).map(String::from);
        let cle = cle_cours(cours);
        let texte = nettoyer(&format!("{titre}\n\n{desc}"));
        if texte.chars().filter(|c| c.is_alphanumeric()).count() >= 15 {
            r.sources += 1;
            let s = NouvelleSource {
                cle: cle.clone(),
                parent: None,
                matiere: matiere.clone(),
                type_: "cours",
                titre: if titre.is_empty() { format!("Cours du {}", date.clone().unwrap_or_default()) } else { titre },
                date: date.clone(),
                chemin: None,
            };
            if enregistrer(c, &s, &texte, &empreinte(&texte))? {
                r.nouvelles += 1;
            }
        }
        for f in cours["contenu"]["fichiers"].as_array().into_iter().flatten() {
            if let Some(ch) = f["chemin"].as_str() {
                let s = NouvelleSource {
                    cle: format!("doc:{ch}"),
                    parent: Some(cle.clone()),
                    matiere: matiere.clone(),
                    type_: "document",
                    titre: f["nom"].as_str().unwrap_or("Document").to_string(),
                    date: date.clone(),
                    chemin: Some(ch.to_string()),
                };
                fichiers.push((s, ch.to_string()));
            }
        }
    }

    for d in sync["devoirs"].as_array().into_iter().flatten() {
        let matiere = d["matiere"].as_str().unwrap_or("").to_string();
        let date = d["pour_le"].as_str().map(String::from);
        let desc = nettoyer(d["description"].as_str().unwrap_or(""));
        let cle = format!("devoir:{}:{}:{}", date.clone().unwrap_or_default(), suivi::slug(&matiere), empreinte(&desc));
        if desc.chars().filter(|c| c.is_alphanumeric()).count() >= 15 {
            r.sources += 1;
            let s = NouvelleSource { cle: cle.clone(), parent: None, matiere: matiere.clone(), type_: "devoir", titre: "Devoir".into(), date: date.clone(), chemin: None };
            if enregistrer(c, &s, &desc, &empreinte(&desc))? {
                r.nouvelles += 1;
            }
        }
        for f in d["fichiers"].as_array().into_iter().flatten() {
            if let Some(ch) = f["chemin"].as_str() {
                let s = NouvelleSource {
                    cle: format!("doc:{ch}"),
                    parent: Some(cle.clone()),
                    matiere: matiere.clone(),
                    type_: "document",
                    titre: f["nom"].as_str().unwrap_or("Document").to_string(),
                    date: date.clone(),
                    chemin: Some(ch.to_string()),
                };
                fichiers.push((s, ch.to_string()));
            }
        }
    }

    // Documents : l'empreinte est la taille + la date du fichier, pour ne relire (et ré-OCRiser) que s'il change.
    for (s, ch) in fichiers {
        let chemin = Path::new(&ch);
        let Ok(meta) = std::fs::metadata(chemin) else { continue };
        let emp = format!("{}-{:?}", meta.len(), meta.modified().ok());
        let deja: Option<String> = c.query_row("SELECT empreinte FROM sources WHERE cle = ?1", [&s.cle], |r| r.get(0)).optional()?;
        r.sources += 1;
        if deja.as_deref() == Some(emp.as_str()) {
            continue;
        }
        match texte_document(chemin) {
            Some(t) => {
                if enregistrer(c, &s, &t, &emp)? {
                    r.nouvelles += 1;
                }
            }
            None => r.illisibles.push(s.titre.clone()),
        }
    }

    r.passages = c.query_row("SELECT COUNT(*) FROM passages", [], |x| x.get::<_, i64>(0))? as usize;
    r.vecteurs = calculer_vecteurs(c).unwrap_or_else(|e| {
        eprintln!("embeddings : {e:#}");
        0
    });
    Ok(r)
}

/// Calcule les vecteurs de sens des passages qui n'en ont pas encore (sur le CPU).
pub fn calculer_vecteurs(c: &Connection) -> Result<usize> {
    let manquants: Vec<(i64, String)> = {
        let mut st = c.prepare("SELECT id, texte FROM passages WHERE embedding IS NULL")?;
        let v = st.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
        v
    };
    let mut n = 0;
    for paquet in manquants.chunks(32) {
        let textes: Vec<String> = paquet.iter().map(|(_, t)| t.clone()).collect();
        let vecteurs = ia::embeddings(&textes, false)?;
        let tx = c.unchecked_transaction()?;
        for ((id, _), v) in paquet.iter().zip(vecteurs) {
            let octets: Vec<u8> = v.iter().flat_map(|x| x.to_le_bytes()).collect();
            tx.execute("UPDATE passages SET embedding = ?2 WHERE id = ?1", params![id, octets])?;
            n += 1;
        }
        tx.commit()?;
    }
    Ok(n)
}

// ---------------------------------------------------------------- recherche

#[derive(Serialize, Clone, Debug)]
pub struct Passage {
    pub id: i64,
    pub source_id: i64,
    pub matiere: String,
    pub type_: String,
    pub titre: String,
    pub date: Option<String>,
    pub texte: String,
}

fn lire_passage(c: &Connection, id: i64) -> Result<Passage> {
    Ok(c.query_row(
        "SELECT p.id, p.source_id, p.matiere, s.type, s.titre, s.date, p.texte FROM passages p JOIN sources s ON s.id = p.source_id WHERE p.id = ?1",
        [id],
        |r| Ok(Passage { id: r.get(0)?, source_id: r.get(1)?, matiere: r.get(2)?, type_: r.get(3)?, titre: r.get(4)?, date: r.get(5)?, texte: r.get(6)? }),
    )?)
}

/// Requête FTS5 sûre : les mots de 3 lettres et plus, entre guillemets, reliés par OR.
fn requete_fts(q: &str) -> Option<String> {
    let mots: Vec<String> = q
        .split(|c: char| !c.is_alphanumeric())
        .filter(|m| m.chars().count() >= 3)
        .map(|m| format!("\"{}\"", m.to_lowercase()))
        .collect();
    (!mots.is_empty()).then(|| mots.join(" OR "))
}

/// Recherche hybride : plein texte + sens, fusion des rangs (RRF). Sans le serveur d'embeddings, plein texte seul.
pub fn chercher(c: &Connection, requete: &str, matiere: Option<&str>, k: usize) -> Result<Vec<Passage>> {
    let mut scores: HashMap<i64, f64> = HashMap::new();
    if let Some(fts) = requete_fts(requete) {
        let mut st = c.prepare(
            "SELECT p.id FROM passages_fts f JOIN passages p ON p.id = f.rowid
             WHERE passages_fts MATCH ?1 AND (?2 IS NULL OR p.matiere = ?2) ORDER BY bm25(passages_fts) LIMIT 20",
        )?;
        let ids: Vec<i64> = st.query_map(params![fts, matiere], |r| r.get(0))?.collect::<rusqlite::Result<_>>()?;
        for (rang, id) in ids.into_iter().enumerate() {
            *scores.entry(id).or_default() += 1.0 / (60.0 + rang as f64);
        }
    }
    if let Ok(mut v) = ia::embeddings(&[requete.to_string()], true) {
        let q = v.remove(0);
        let mut st = c.prepare("SELECT id, embedding FROM passages WHERE embedding IS NOT NULL AND (?1 IS NULL OR matiere = ?1)")?;
        let mut proches: Vec<(i64, f32)> = st
            .query_map(params![matiere], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))?
            .filter_map(|x| x.ok())
            .map(|(id, o)| {
                let e: Vec<f32> = o.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
                (id, ia::cosinus(&q, &e))
            })
            .collect();
        proches.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (rang, (id, _)) in proches.into_iter().take(20).enumerate() {
            *scores.entry(id).or_default() += 1.0 / (60.0 + rang as f64);
        }
    }
    let mut tries: Vec<(i64, f64)> = scores.into_iter().collect();
    tries.sort_by(|a, b| b.1.total_cmp(&a.1));
    tries.into_iter().take(k).map(|(id, _)| lire_passage(c, id)).collect()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SourceCours {
    pub id: i64,
    pub cle: String,
    pub matiere: String,
    pub titre: String,
    pub date: Option<String>,
    pub texte: String,
}

/// Le cours d'une leçon (par sa date-heure de début et sa matière).
pub fn source_cours(c: &Connection, debut: &str, matiere: &str) -> Result<Option<SourceCours>> {
    let cle = format!("cours:{debut}:{}", suivi::slug(matiere));
    Ok(c.query_row("SELECT id, cle, matiere, titre, date, texte FROM sources WHERE cle = ?1", [cle], |r| {
        Ok(SourceCours { id: r.get(0)?, cle: r.get(1)?, matiere: r.get(2)?, titre: r.get(3)?, date: r.get(4)?, texte: r.get(5)? })
    })
    .optional()?)
}

/// Les derniers cours (avec contenu) d'une matière, du plus récent au plus ancien.
pub fn derniers_cours(c: &Connection, matiere: &str, n: usize) -> Result<Vec<SourceCours>> {
    let mut st = c.prepare(
        "SELECT id, cle, matiere, titre, date, texte FROM sources WHERE type = 'cours' AND matiere = ?1 AND date <= ?2 ORDER BY date DESC, cle DESC LIMIT ?3",
    )?;
    let auj = Local::now().date_naive().to_string();
    let v = st
        .query_map(params![matiere, auj, n as i64], |r| {
            Ok(SourceCours { id: r.get(0)?, cle: r.get(1)?, matiere: r.get(2)?, titre: r.get(3)?, date: r.get(4)?, texte: r.get(5)? })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

/// Les documents (texte extrait) attachés à un cours.
pub fn documents_du_cours(c: &Connection, cle_cours: &str) -> Result<Vec<SourceCours>> {
    let mut st = c.prepare("SELECT id, cle, matiere, titre, date, texte FROM sources WHERE parent = ?1 AND type = 'document'")?;
    let v = st
        .query_map([cle_cours], |r| Ok(SourceCours { id: r.get(0)?, cle: r.get(1)?, matiere: r.get(2)?, titre: r.get(3)?, date: r.get(4)?, texte: r.get(5)? }))?
        .collect::<rusqlite::Result<_>>()?;
    Ok(v)
}

// ---------------------------------------------------------------- fiches de leçon

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Notion {
    pub code: String,
    pub intitule: String,
    /// Ce qu'il faut savoir, en 1 ou 2 phrases précises (définition, règle, mécanisme, fait).
    pub essentiel: String,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default)]
pub struct Fiche {
    pub chapitre: String,
    pub titre_chapitre: String,
    pub notions: Vec<Notion>,
    #[serde(default)]
    pub exemples: Vec<String>,
    #[serde(default)]
    pub personnes: Vec<String>,
    #[serde(default)]
    pub dates: Vec<String>,
    #[serde(default)]
    pub textes_etudies: Vec<String>,
    #[serde(default)]
    pub methodes: Vec<String>,
    #[serde(default)]
    pub vocabulaire: Vec<String>,
    /// Ce qui vient du programme officiel et pas du cours (cours trop court) : à utiliser avec prudence.
    #[serde(default)]
    pub complements_programme: Vec<String>,
}

impl Fiche {
    /// Version compacte pour le contexte du modèle.
    pub fn resume(&self) -> String {
        let mut s = format!("Chapitre : {}\nNotions :\n", self.titre_chapitre);
        for n in &self.notions {
            s.push_str(&format!("- [{}] {} : {}\n", n.code, n.intitule, n.essentiel));
        }
        let liste = |nom: &str, v: &[String]| if v.is_empty() { String::new() } else { format!("{nom} : {}\n", v.join(" ; ")) };
        s.push_str(&liste("Exemples du cours", &self.exemples));
        s.push_str(&liste("Personnes", &self.personnes));
        s.push_str(&liste("Dates", &self.dates));
        s.push_str(&liste("Textes / œuvres étudiés", &self.textes_etudies));
        s.push_str(&liste("Méthodes", &self.methodes));
        s.push_str(&liste("Vocabulaire", &self.vocabulaire));
        s.push_str(&liste("Compléments du programme (pas vus explicitement en cours)", &self.complements_programme));
        s
    }
}

const SYSTEME_FICHE: &str = "Tu es un professeur de lycée qui rédige la fiche de connaissances d'une leçon de seconde, \
à partir du cahier de textes et des documents distribués. Tu es précis et factuel : chaque « essentiel » est une \
connaissance exacte qu'un élève doit savoir (définition, règle, mécanisme, fait, date), formulée simplement. Tu \
n'inventes rien : si le cours est trop maigre, tu complètes avec le programme officiel de seconde UNIQUEMENT dans \
« complements_programme ». Écris en français (les mots étudiés d'une langue vivante restent dans cette langue). \
Pas de LaTeX : symboles Unicode (ℝ, ², √, ≤). Réponds uniquement en JSON.";

fn schema_fiche() -> Value {
    let liste = json!({ "type": "array", "items": { "type": "string" }, "maxItems": 8 });
    json!({
        "type": "object",
        "properties": {
            "chapitre": { "type": "string", "description": "code court en minuscules et tirets" },
            "titre_chapitre": { "type": "string" },
            "notions": { "type": "array", "minItems": 1, "maxItems": 8, "items": {
                "type": "object",
                "properties": { "code": { "type": "string" }, "intitule": { "type": "string" }, "essentiel": { "type": "string" } },
                "required": ["code", "intitule", "essentiel"]
            }},
            "exemples": liste, "personnes": liste, "dates": liste, "textes_etudies": liste,
            "methodes": liste, "vocabulaire": liste, "complements_programme": liste
        },
        "required": ["chapitre", "titre_chapitre", "notions", "exemples", "personnes", "dates", "textes_etudies", "methodes", "vocabulaire", "complements_programme"]
    })
}

pub fn fiche_enregistree(c: &Connection, source_id: i64) -> Result<Option<Fiche>> {
    let t: Option<String> = c.query_row("SELECT contenu FROM fiches WHERE source_id = ?1", [source_id], |r| r.get(0)).optional()?;
    Ok(t.and_then(|t| serde_json::from_str(&t).ok()))
}

/// Les fiches d'un chapitre (pour la révision des notions faibles).
pub fn fiches_du_chapitre(c: &Connection, matiere: &str, chapitre: &str) -> Result<Vec<Fiche>> {
    let mut st = c.prepare("SELECT contenu FROM fiches WHERE matiere = ?1 AND chapitre = ?2")?;
    let v = st.query_map(params![matiere, chapitre], |r| r.get::<_, String>(0))?.filter_map(|x| x.ok()).filter_map(|t| serde_json::from_str(&t).ok()).collect();
    Ok(v)
}

/// La fiche d'un cours : enregistrée, ou écrite maintenant par l'IA (une seule fois), puis rattachée au suivi.
pub fn fiche(c: &Connection, cours: &SourceCours) -> Result<Fiche> {
    if let Some(f) = fiche_enregistree(c, cours.id)? {
        return Ok(f);
    }
    let mut contexte = format!("Matière : {}\nCours du {} — « {} »\n\nCahier de textes :\n{}\n", cours.matiere, cours.date.clone().unwrap_or_default(), cours.titre, cours.texte);
    for d in documents_du_cours(c, &cours.cle)? {
        contexte.push_str(&format!("\nDocument distribué « {} » :\n\"\"\"\n{}\n\"\"\"\n", d.titre, d.texte.chars().take(3500).collect::<String>()));
    }
    // Les cours précédents de la matière situent la leçon dans le chapitre (et évitent d'inventer).
    let avant: Vec<String> = derniers_cours(c, &cours.matiere, 4)?
        .into_iter()
        .filter(|x| x.id != cours.id)
        .map(|x| format!("- {} : {}", x.date.unwrap_or_default(), x.texte.chars().take(300).collect::<String>().replace('\n', " ")))
        .collect();
    if !avant.is_empty() {
        contexte.push_str(&format!("\nCours précédents (contexte) :\n{}\n", avant.join("\n")));
    }
    let chapitres: Vec<String> = suivi::carte(c, Some(&cours.matiere), None)?
        .into_iter()
        .map(|l| format!("{} ({})", l.chapitre, l.titre_chapitre))
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    contexte.push_str(&format!(
        "\nChapitres déjà suivis dans cette matière : {}\nSi la leçon continue l'un d'eux, réutilise EXACTEMENT son code dans \"chapitre\".",
        if chapitres.is_empty() { "aucun".into() } else { chapitres.join(", ") }
    ));
    let v = ia::demander_json(SYSTEME_FICHE, &contexte, &ia::Options { schema: Some(schema_fiche()), temperature: Some(0.3), max_jetons: Some(1800), ..Default::default() })?;
    let mut f: Fiche = serde_json::from_value(v).context("fiche de leçon illisible")?;
    f.chapitre = suivi::slug(&f.chapitre);
    for n in &mut f.notions {
        n.code = suivi::slug(&n.code);
    }
    f.notions.retain(|n| !n.code.is_empty() && !n.essentiel.trim().is_empty());
    let notions: Vec<String> = f.notions.iter().map(|n| format!("{}:{}", n.code, n.intitule)).collect();
    suivi::chapitre(c, &cours.matiere, &f.chapitre, Some(&f.titre_chapitre), &notions, Some(&format!("cours du {}", cours.date.clone().unwrap_or_default())))?;
    c.execute(
        "INSERT OR REPLACE INTO fiches (source_id, matiere, chapitre, contenu, cree_le) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![cours.id, cours.matiere, f.chapitre, serde_json::to_string(&f)?, maintenant()],
    )?;
    Ok(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoupage() {
        let long = (0..700).map(|i| format!("mot{i}")).collect::<Vec<_>>().join(" ");
        let p = decouper(&format!("Titre court\n\n{long}\n\nFin."));
        assert!(p.len() >= 3);
        assert!(p.iter().all(|x| x.split_whitespace().count() <= 300 + 2));
        assert_eq!(decouper("Un seul paragraphe."), vec!["Un seul paragraphe."]);
    }

    #[test]
    fn requete_sure() {
        assert_eq!(requete_fts("l'ÉCOLE des femmes ?"), Some("\"école\" OR \"des\" OR \"femmes\"".into()));
        assert_eq!(requete_fts("a b"), None);
    }

    #[test]
    fn recherche_plein_texte() {
        let c = crate::db::ouvrir_memoire().unwrap();
        let s = NouvelleSource { cle: "cours:x:svt".into(), parent: None, matiere: "SVT".into(), type_: "cours", titre: "Photosynthèse".into(), date: Some("2026-11-06".into()), chemin: None };
        enregistrer(&c, &s, "Les cellules chlorophylliennes réalisent la photosynthèse grâce aux chloroplastes.", "e1").unwrap();
        let s2 = NouvelleSource { cle: "cours:y:hg".into(), matiere: "HG".into(), titre: "Démographie".into(), ..s };
        enregistrer(&c, &s2, "La transition démographique comporte quatre phases.", "e2").unwrap();
        // Sans accents, et filtré par matière.
        let r = chercher(&c, "chloroplaste photosynthese", Some("SVT"), 3).unwrap();
        assert_eq!(r.len(), 1);
        assert!(r[0].texte.contains("chloroplastes"));
        assert!(chercher(&c, "chloroplastes", Some("HG"), 3).unwrap().is_empty());
        // Même empreinte : rien n'est refait.
        let s3 = NouvelleSource { cle: "cours:y:hg".into(), parent: None, matiere: "HG".into(), type_: "cours", titre: "D".into(), date: None, chemin: None };
        assert!(!enregistrer(&c, &s3, "autre", "e2").unwrap());
    }
}
