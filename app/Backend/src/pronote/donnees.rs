//! Récupération des données de l'élève et écriture de `pronote/sync.json`
//! (même format que l'ancien script Python : l'interface et le skill le lisent tel quel).

use std::collections::{HashMap, HashSet};
use std::path::Path;

use anyhow::{Context, Result};
use chrono::{Duration, Local, NaiveDate, NaiveDateTime, NaiveTime};
use regex::Regex;
use serde_json::{Value, json};

use super::crypto::bourrer;
use super::session::Session;

const JOURS_PASSES_COURS: i64 = 21;
const JOURS_FUTURS_COURS: i64 = 35;
const JOURS_PASSES_DEVOIRS: i64 = 7;
const JOURS_FUTURS_DEVOIRS: i64 = 35;
const JOURS_INFOS: i64 = 30;
const MAX_INFOS_DETAILLEES: usize = 15;

const NOTES_SPECIALES: [&str; 8] =
    ["Absent", "Dispense", "NonNote", "Inapte", "NonRendu", "AbsentZero", "NonRenduZero", "Felicitations"];

// ---------------------------------------------------------------- petits outils de lecture

fn texte(v: &Value) -> Option<String> {
    match v {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn html_en_texte(html: &str) -> String {
    let sans_balises = Regex::new(r"(?s)<.*?>").unwrap().replace_all(html, "");
    html_escape::decode_html_entities(&sans_balises).trim().to_string()
}

fn note(v: &Value) -> Option<String> {
    let s = texte(v)?;
    if s.contains('|') {
        let i: usize = s.chars().nth(1)?.to_digit(10)? as usize;
        return NOTES_SPECIALES.get(i.wrapping_sub(1)).map(|n| n.to_string());
    }
    Some(s)
}

fn date_heure(s: &str) -> Option<NaiveDateTime> {
    for f in ["%d/%m/%Y %H:%M:%S", "%d/%m/%y %Hh%M"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(s, f) {
            return Some(d);
        }
    }
    for f in ["%d/%m/%Y", "%d/%m/%y"] {
        if let Ok(d) = NaiveDate::parse_from_str(s, f) {
            return Some(d.and_time(NaiveTime::MIN));
        }
    }
    None
}

fn iso(d: NaiveDateTime) -> String {
    d.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn nom_fichier_propre(nom: &str) -> String {
    let n: String = nom.chars().map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c }).collect();
    let n = n.trim_matches(|c: char| c == ' ' || c == '.' || c == '_').chars().take(120).collect::<String>();
    if n.is_empty() { "sans_nom".into() } else { n }
}

/// `urllib.parse.quote(nom, safe="~()*!.'")`.
fn encoder_nom(nom: &str) -> String {
    let mut s = String::new();
    for b in nom.as_bytes() {
        if b.is_ascii_alphanumeric() || b"_.-~()*!'".contains(b) {
            s.push(*b as char);
        } else {
            s.push_str(&format!("%{b:02X}"));
        }
    }
    s
}

// ---------------------------------------------------------------- récupération

pub struct Options<'a> {
    pub dossier: &'a Path,
    pub telecharger: bool,
}

pub async fn recuperer(s: &mut Session, opt: &Options<'_>) -> Result<Value> {
    let auj = Local::now().date_naive();
    let mut alertes: Vec<String> = Vec::new();
    let ressource = s.utilisateur["dataSec"]["data"]["ressource"].clone();
    let general = s.parametres["dataSec"]["data"]["General"].clone();

    // --- Cours : emploi du temps semaine par semaine + contenu du cahier de textes,
    //     lui aussi par semaine (une requête par semaine, pas une par cours).
    let debut = auj - Duration::days(JOURS_PASSES_COURS);
    let fin = auj + Duration::days(JOURS_FUTURS_COURS);
    let mut cours = Vec::new();
    for semaine in s.semaine(debut)..=s.semaine(fin) {
        let mut d = json!({
            "ressource": ressource, "avecAbsencesEleve": false, "avecConseilDeClasse": true,
            "estEDTPermanence": false, "avecAbsencesRessource": true, "avecDisponibilites": true,
            "avecInfosPrefsGrille": true, "Ressource": ressource,
        });
        d["NumeroSemaine"] = json!(semaine);
        d["numeroSemaine"] = json!(semaine);
        let liste = match s.requete("PageEmploiDuTemps", Some(16), Some(d)).await {
            Ok(r) => r["dataSec"]["data"]["ListeCours"].as_array().cloned().unwrap_or_default(),
            Err(e) => {
                alertes.push(format!("emploi du temps semaine {semaine} : {e}"));
                continue;
            }
        };
        if liste.is_empty() {
            continue;
        }
        let contenus = if s.semaine(auj) >= semaine {
            match s
                .requete("PageCahierDeTexte", Some(89), Some(json!({ "domaine": { "_T": 8, "V": format!("[{semaine}..{semaine}]") } })))
                .await
            {
                Ok(r) => contenus_par_cours(&r),
                Err(e) => {
                    alertes.push(format!("cahier de textes semaine {semaine} : {e}"));
                    HashMap::new()
                }
            }
        } else {
            HashMap::new()
        };
        for l in liste {
            match lire_cours(s, &l, &general, &contenus, opt).await {
                Ok(Some(c)) if c.0 >= debut.and_time(NaiveTime::MIN) && c.0 <= fin.and_time(NaiveTime::MIN) => cours.push(c.1),
                Ok(_) => {}
                Err(e) => alertes.push(format!("un cours illisible : {e}")),
            }
        }
    }
    cours.sort_by(|a, b| a["debut"].as_str().cmp(&b["debut"].as_str()));

    // --- Devoirs
    let mut devoirs = Vec::new();
    let (d1, d2) = (auj - Duration::days(JOURS_PASSES_DEVOIRS), auj + Duration::days(JOURS_FUTURS_DEVOIRS));
    match s
        .requete(
            "PageCahierDeTexte",
            Some(88),
            Some(json!({ "domaine": { "_T": 8, "V": format!("[{}..{}]", s.semaine(d1), s.semaine(d2)) } })),
        )
        .await
    {
        Ok(r) => {
            for h in r["dataSec"]["data"]["ListeTravauxAFaire"]["V"].as_array().cloned().unwrap_or_default() {
                let Some(pour) = h["PourLe"]["V"].as_str().and_then(date_heure).map(|d| d.date()) else { continue };
                if pour < d1 || pour > d2 {
                    continue;
                }
                let matiere = h["Matiere"]["V"]["L"].as_str().unwrap_or("Autre").to_string();
                let fichiers = fichiers(s, &h["ListePieceJointe"]["V"], &matiere, &format!("devoir_{pour}"), opt).await;
                devoirs.push(json!({
                    "matiere": matiere,
                    "pour_le": pour.to_string(),
                    "fait": h["TAFFait"].as_bool().unwrap_or(false),
                    "description": html_en_texte(h["descriptif"]["V"].as_str().unwrap_or_default()),
                    "fichiers": fichiers,
                }));
            }
        }
        Err(e) => alertes.push(format!("devoirs : {e}")),
    }
    devoirs.sort_by(|a, b| a["pour_le"].as_str().cmp(&b["pour_le"].as_str()));

    let controles = reperer_controles(&cours, &devoirs, auj);

    // --- Notes et moyennes de toutes les périodes (dédoublonnées : une note peut être
    //     dans le trimestre ET dans le semestre ou l'année).
    let periodes = general["ListePeriodes"].as_array().cloned().unwrap_or_default();
    let (mut notes, mut moyennes, mut generales) = (Vec::new(), Vec::new(), Vec::new());
    let mut vues = HashSet::new();
    for p in &periodes {
        let (id, nom) = (texte(&p["N"]).unwrap_or_default(), texte(&p["L"]).unwrap_or_default());
        match s.requete("DernieresNotes", Some(198), Some(json!({ "Periode": { "N": id, "L": nom } }))).await {
            Ok(r) => {
                let data = &r["dataSec"]["data"];
                for g in data["listeDevoirs"]["V"].as_array().cloned().unwrap_or_default() {
                    let date = g["date"]["V"].as_str().and_then(date_heure).map(|d| d.date().to_string());
                    let n = json!({
                        "periode": nom, "matiere": g["service"]["V"]["L"].as_str().unwrap_or("Autre"),
                        "date": date, "note": note(&g["note"]["V"]), "sur": note(&g["bareme"]["V"]),
                        "coef": texte(&g["coefficient"]), "moyenne_classe": note(&g["moyenne"]["V"]),
                        "commentaire": texte(&g["commentaire"]),
                    });
                    let cle = format!("{}|{}|{}|{}|{}|{}", n["matiere"], n["date"], n["note"], n["sur"], n["coef"], n["commentaire"]);
                    if vues.insert(cle) {
                        notes.push(n);
                    }
                }
                // Moyenne générale du trimestre (élève et classe), quand Pronote la donne.
                if let Some(g) = note(&data["moyGenerale"]["V"]) {
                    generales.push(json!({ "periode": nom, "eleve": g, "classe": note(&data["moyGeneraleClasse"]["V"]) }));
                }
                for m in data["listeServices"]["V"].as_array().cloned().unwrap_or_default() {
                    moyennes.push(json!({
                        "periode": nom, "matiere": m["L"].as_str().unwrap_or("Autre"),
                        "eleve": note(&m["moyEleve"]["V"]), "classe": note(&m["moyClasse"]["V"]),
                    }));
                }
            }
            Err(e) => alertes.push(format!("notes {nom} : {e}")),
        }
    }
    notes.sort_by(|a, b| b["date"].as_str().cmp(&a["date"].as_str()));

    // --- Informations de l'établissement (le détail coûte une requête par info : on se limite)
    let mut infos = Vec::new();
    match s.requete("PageActualites", Some(8), Some(json!({ "modesAffActus": { "_T": 26, "V": "[0..3]" } }))).await {
        Ok(r) => {
            let limite = auj - Duration::days(JOURS_INFOS);
            let mut liste: Vec<Value> = r["dataSec"]["data"]["listeModesAff"]
                .as_array()
                .into_iter()
                .flatten()
                .flat_map(|m| m["listeActualites"]["V"].as_array().cloned().unwrap_or_default())
                .filter(|i| {
                    i["dateCreation"]["V"].as_str().and_then(date_heure).is_some_and(|d| d.date() >= limite)
                })
                .collect();
            liste.sort_by(|a, b| {
                let d = |v: &Value| v["dateCreation"]["V"].as_str().and_then(date_heure);
                d(b).cmp(&d(a))
            });
            let id_eleve = texte(&ressource["N"]).unwrap_or_default();
            for i in liste.into_iter().take(MAX_INFOS_DETAILLEES) {
                let contenu = match s
                    .requete(
                        "PageActualites",
                        Some(8),
                        Some(json!({
                            "actualite": { "N": i["N"], "genrePublic": 4, "public": { "N": id_eleve, "G": 4 } },
                            "genreRequeteActualite": 1, "modeAffActu": 0,
                        })),
                    )
                    .await
                {
                    Ok(r) => r["dataSec"]["data"]["detailsActualite"]["listeQuestions"]["V"][0]["texte"]["V"]
                        .as_str()
                        .map(html_en_texte)
                        .unwrap_or_default(),
                    Err(_) => "(contenu illisible, voir Pronote)".into(),
                };
                infos.push(json!({
                    "titre": texte(&i["L"]), "auteur": texte(&i["auteur"]),
                    "date": i["dateCreation"]["V"].as_str().and_then(date_heure).map(iso),
                    "lu": i["lue"].as_bool().unwrap_or(false),
                    "contenu": contenu.chars().take(2000).collect::<String>(),
                }));
            }
        }
        Err(e) => alertes.push(format!("informations : {e}")),
    }

    Ok(json!({
        "synchronise_le": Local::now().format("%Y-%m-%dT%H:%M").to_string(),
        "eleve": {
            "nom": texte(&ressource["L"]),
            "classe": texte(&ressource["classeDEleve"]["L"]),
            "etablissement": texte(&ressource["Etablissement"]["V"]["L"]),
        },
        "periode": periode_courante(&ressource, &periodes),
        "controles_a_venir": controles,
        "devoirs": devoirs,
        "cours": cours,
        "notes": notes,
        "moyennes": moyennes,
        "moyennes_generales": generales,
        "periodes": periodes.iter().map(|p| json!({
            "nom": texte(&p["L"]),
            "debut": p["dateDebut"]["V"].as_str().and_then(date_heure).map(iso),
            "fin": p["dateFin"]["V"].as_str().and_then(date_heure).map(iso),
        })).collect::<Vec<_>>(),
        "informations": infos,
        "alertes": alertes,
    }))
}

/// Contenu du cahier de textes d'une semaine, indexé par identifiant de cours.
fn contenus_par_cours(r: &Value) -> HashMap<String, Value> {
    let mut m = HashMap::new();
    for c in r["dataSec"]["data"]["ListeCahierDeTextes"]["V"].as_array().into_iter().flatten() {
        let (Some(id), Some(premier)) = (texte(&c["cours"]["V"]["N"]), c["listeContenus"]["V"].get(0)) else { continue };
        m.insert(id, premier.clone());
    }
    m
}

async fn lire_cours(
    s: &Session,
    l: &Value,
    general: &Value,
    contenus: &HashMap<String, Value>,
    opt: &Options<'_>,
) -> Result<Option<(NaiveDateTime, Value)>> {
    let debut = l["DateDuCours"]["V"].as_str().and_then(date_heure).context("date de cours absente")?;
    let fin = match l["DateDuCoursFin"]["V"].as_str().and_then(date_heure) {
        Some(f) => f,
        None => fin_par_place(l, general, debut).unwrap_or(debut + Duration::hours(1)),
    };
    let (mut matiere, mut profs, mut salles) = (None, Vec::new(), Vec::new());
    for d in l["ListeContenus"]["V"].as_array().into_iter().flatten() {
        match d["G"].as_i64() {
            Some(16) => matiere = texte(&d["L"]),
            Some(3) => profs.extend(texte(&d["L"])),
            Some(17) => salles.extend(texte(&d["L"])),
            _ => {}
        }
    }
    let matiere = matiere.unwrap_or_else(|| "Autre".into());
    let contenu = match texte(&l["N"]).and_then(|id| contenus.get(&id)) {
        Some(c) => {
            let f = fichiers(s, &c["ListePieceJointe"]["V"], &matiere, &debut.format("%Y-%m-%d").to_string(), opt).await;
            json!({
                "titre": texte(&c["L"]),
                "description": c["descriptif"]["V"].as_str().map(html_en_texte),
                "categorie": texte(&c["categorie"]["V"]["L"]),
                "fichiers": f,
            })
        }
        None => Value::Null,
    };
    Ok(Some((
        debut,
        json!({
            "matiere": matiere,
            "debut": iso(debut),
            "fin": iso(fin),
            "prof": (!profs.is_empty()).then(|| profs.join(", ")),
            "salle": (!salles.is_empty()).then(|| salles.join(", ")),
            "annule": l["estAnnule"].as_bool().unwrap_or(false),
            "controle": l["cahierDeTextes"]["V"]["estDevoir"].as_bool().unwrap_or(false),
            "statut": texte(&l["Statut"]),
            "memo": texte(&l["memo"]),
            "contenu": contenu,
        }),
    )))
}

/// Heure de fin quand Pronote ne la donne pas : calculée depuis la « place » du cours dans la grille.
fn fin_par_place(l: &Value, general: &Value, debut: NaiveDateTime) -> Option<NaiveDateTime> {
    let heures = general["ListeHeuresFin"]["V"].as_array()?;
    let n = heures.len() as i64;
    let mut place = l["place"].as_i64()? % (n - 1) + l["duree"].as_i64()? - 1;
    if place > n {
        place %= n - 1;
    }
    let h = heures.iter().find(|h| h["G"].as_i64() == Some(place))?;
    let t = NaiveTime::parse_from_str(h["L"].as_str()?, "%Hh%M").ok()?;
    Some(debut.date().and_time(t))
}

/// Pièces jointes : liens gardés tels quels, fichiers téléchargés (une seule fois).
async fn fichiers(s: &Session, liste: &Value, matiere: &str, prefixe: &str, opt: &Options<'_>) -> Vec<Value> {
    let mut sortie = Vec::new();
    for f in liste.as_array().into_iter().flatten() {
        let nom = f["L"].as_str().unwrap_or_default().to_string();
        if f["G"].as_i64() == Some(0) {
            let lien = f["url"].as_str().map(String::from).unwrap_or_else(|| nom.clone());
            sortie.push(json!({ "nom": nom, "lien": lien }));
            continue;
        }
        let dossier = opt.dossier.join("pronote").join("fichiers").join(nom_fichier_propre(matiere));
        let chemin = dossier.join(nom_fichier_propre(&format!("{prefixe}_{nom}")));
        if chemin.exists() {
            sortie.push(json!({ "nom": nom, "chemin": chemin }));
            continue;
        }
        if !opt.telecharger {
            sortie.push(json!({ "nom": nom, "non_telecharge": true }));
            continue;
        }
        let id = texte(&f["N"]).unwrap_or_default();
        // Pronote veut l'identifiant chiffré dans l'URL (avec un double bourrage, comme eleve.js).
        let magique = hex::encode(s.chiffrement().chiffrer(&bourrer(format!(r#"{{"N":"{id}","Actif":true}}"#).as_bytes())));
        let url = format!("{}/FichiersExternes/{magique}/{}?Session={}", s.racine(), encoder_nom(&nom), s.attributs["h"]);
        let resultat: Result<()> = async {
            let r = s.http().get(&url).send().await?.error_for_status()?;
            let octets = r.bytes().await?;
            std::fs::create_dir_all(&dossier)?;
            std::fs::write(&chemin, &octets)?;
            Ok(())
        }
        .await;
        sortie.push(match resultat {
            Ok(()) => json!({ "nom": nom, "chemin": chemin }),
            Err(e) => json!({ "nom": nom, "erreur_telechargement": e.to_string() }),
        });
    }
    sortie
}

/// Évaluations à venir : cours marqués « devoir surveillé », ou mots-clés dans les devoirs et mémos.
fn reperer_controles(cours: &[Value], devoirs: &[Value], auj: NaiveDate) -> Vec<Value> {
    let mots = Regex::new(
        r"(?i)\b(contr[oô]les?|DS|DST|devoirs? surveill[ée]s?|interros?|interrogations?|[ée]valuations?|[ée]val|bac blanc|examens?|QCM|oral|exposés?|test)\b",
    )
    .unwrap();
    let auj = auj.to_string();
    let mut c = Vec::new();
    for l in cours {
        let debut = l["debut"].as_str().unwrap_or_default();
        if &debut[..10.min(debut.len())] < auj.as_str() || l["annule"].as_bool() == Some(true) {
            continue;
        }
        let morceaux: Vec<&str> = [&l["memo"], &l["statut"], &l["contenu"]["titre"], &l["contenu"]["description"]]
            .iter()
            .filter_map(|v| v.as_str())
            .collect();
        let t = morceaux.join(" ");
        let marque = l["controle"].as_bool() == Some(true);
        if marque || mots.is_match(&t) {
            c.push(json!({
                "matiere": l["matiere"], "date": &debut[..10],
                "source": if marque { "emploi_du_temps" } else { "mot_cle_cours" },
                "indice": if t.is_empty() { "marqué contrôle dans Pronote".to_string() } else { t.chars().take(300).collect() },
            }));
        }
    }
    for d in devoirs {
        let pour = d["pour_le"].as_str().unwrap_or_default();
        let desc = d["description"].as_str().unwrap_or_default();
        if pour >= auj.as_str() && mots.is_match(desc) {
            c.push(json!({ "matiere": d["matiere"], "date": pour, "source": "mot_cle_devoir", "indice": desc.chars().take(300).collect::<String>() }));
        }
    }
    c.sort_by(|a, b| a["date"].as_str().cmp(&b["date"].as_str()));
    c
}

fn periode_courante(ressource: &Value, periodes: &[Value]) -> Value {
    let onglets = ressource["listeOngletsPourPeriodes"]["V"].as_array();
    let onglet = onglets.and_then(|o| o.iter().find(|x| x["G"].as_i64() == Some(198)).or(o.first()));
    let id = onglet.and_then(|o| texte(&o["periodeParDefaut"]["V"]["N"]));
    let p = periodes.iter().find(|p| texte(&p["N"]) == id);
    match p {
        Some(p) => json!({
            "nom": texte(&p["L"]),
            "debut": p["dateDebut"]["V"].as_str().and_then(date_heure).map(iso),
            "fin": p["dateFin"]["V"].as_str().and_then(date_heure).map(iso),
        }),
        None => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_speciales() {
        assert_eq!(note(&json!("|1")).as_deref(), Some("Absent"));
        assert_eq!(note(&json!("14,5")).as_deref(), Some("14,5"));
    }

    #[test]
    fn dates() {
        assert_eq!(iso(date_heure("19/11/2026 08:00:00").unwrap()), "2026-11-19T08:00:00");
        assert_eq!(date_heure("30/11/2026").unwrap().date().to_string(), "2026-11-30");
    }

    #[test]
    fn html() {
        assert_eq!(html_en_texte("<p>R&eacute;viser <b>tout</b></p>"), "Réviser tout");
    }

    #[test]
    fn encodage_nom() {
        assert_eq!(encoder_nom("Activité 2.pdf"), "Activit%C3%A9%202.pdf");
    }
}
