//! Session Pronote : initialisation, requêtes chiffrées/compressées, authentification.
//! Port fidèle de `pronotepy.pronoteAPI._Communication` et de `ClientBase._login`.

use std::collections::HashMap;
use std::io::{Read, Write};

use anyhow::{Context, Result, anyhow, bail};
use chrono::NaiveDate;
use flate2::Compression;
use flate2::read::DeflateDecoder;
use flate2::write::DeflateEncoder;
use serde_json::{Value, json};

use super::crypto::{Chiffrement, md5, sha256_hex_maj};

pub const USER_AGENT: &str = "Mozilla/5.0 (X11; Ubuntu; Linux x86_64; rv:73.0) Gecko/20100101 Firefox/73.0";

/// Erreurs qu'on veut reconnaître pour donner un message clair.
#[derive(Debug)]
pub enum ErreurPronote {
    IpBloquee,
    Identifiants(String),
    Serveur { code: i64, titre: String },
}

impl std::fmt::Display for ErreurPronote {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurPronote::IpBloquee => write!(
                f,
                "Pronote a bloqué temporairement cette adresse IP (trop de connexions rapprochées). Attends quelques heures."
            ),
            ErreurPronote::Identifiants(m) => write!(f, "Connexion refusée : {m}"),
            ErreurPronote::Serveur { code: 25, .. } => {
                write!(f, "Pronote refuse : trop de tentatives de connexion. Attends avant de réessayer.")
            }
            ErreurPronote::Serveur { code, titre } => write!(f, "Erreur Pronote {code} : {titre}"),
        }
    }
}
impl std::error::Error for ErreurPronote {}

pub struct Session {
    http: reqwest::Client,
    racine: String,
    page: String,
    pub attributs: HashMap<String, String>,
    chiffrement: Chiffrement,
    numero: u64,
    chiffrer_requetes: bool,
    compresser_requetes: bool,
    /// Réponse de FonctionParametres (calendrier, périodes, horaires…).
    pub parametres: Value,
    /// Réponse de ParametresUtilisateur (élève, classe, onglets…).
    pub utilisateur: Value,
    pub premier_lundi: NaiveDate,
}

impl Session {
    /// Ouvre une session sur la page élève, avec un client HTTP déjà authentifié auprès de l'ENT
    /// (cookies d'Atrium), puis se connecte.
    pub async fn ouvrir(http: reqwest::Client, url_pronote: &str) -> Result<Session> {
        let url = url_pronote.replace("login=true", "");
        let (racine, page) = url.rsplit_once('/').ok_or_else(|| anyhow!("adresse Pronote invalide"))?;

        let html = http.get(&url).send().await?.text().await?;
        let attributs = analyser_page(&html)?;

        let iv_temp: [u8; 16] = rand::random();
        if attributs.contains_key("http") {
            bail!("Pronote en http non chiffré n'est pas pris en charge (utilise l'adresse en https)");
        }
        let mut session = Session {
            http,
            racine: racine.to_string(),
            page: page.to_string(),
            chiffrer_requetes: attributs.contains_key("CrA"),
            compresser_requetes: attributs.contains_key("CoA"),
            attributs,
            chiffrement: Chiffrement::initial(),
            numero: 1,
            parametres: Value::Null,
            utilisateur: Value::Null,
            premier_lundi: NaiveDate::MIN,
        };

        // L'IV temporaire est annoncé au serveur ; toute la suite est chiffrée avec MD5(iv_temp).
        use base64_mini::encoder;
        let donnees = json!({ "data": { "Uuid": encoder(&iv_temp), "identifiantNav": Value::Null } });
        session.parametres = session.envoyer("FonctionParametres", donnees, Some(md5(&iv_temp))).await?;
        let lundi = session.parametres["dataSec"]["data"]["General"]["PremierLundi"]["V"]
            .as_str()
            .context("PremierLundi absent")?;
        session.premier_lundi = NaiveDate::parse_from_str(lundi, "%d/%m/%Y")?;

        session.connexion_ent().await?;
        Ok(session)
    }

    /// Envoie une requête à Pronote. `nouvel_iv` : IV à utiliser à partir de la réponse.
    async fn envoyer(&mut self, fonction: &str, donnees: Value, nouvel_iv: Option<[u8; 16]>) -> Result<Value> {
        let mut corps: Value = donnees.clone();
        let mut octets: Option<Vec<u8>> = None;

        if self.compresser_requetes {
            // JSON → hex de l'UTF-8 → deflate brut (niveau 6) ; pronotepy fait pareil.
            let hex_json = hex::encode(serde_json::to_string(&donnees)?);
            let mut enc = DeflateEncoder::new(Vec::new(), Compression::new(6));
            enc.write_all(hex_json.as_bytes())?;
            let compresse = enc.finish()?;
            corps = Value::String(hex::encode_upper(&compresse));
            octets = Some(compresse);
        }
        if self.chiffrer_requetes {
            let clair = match octets {
                Some(o) => o,
                None => serde_json::to_vec(&donnees)?,
            };
            corps = Value::String(hex::encode_upper(self.chiffrement.chiffrer(&clair)));
        }

        let no = hex::encode(self.chiffrement.chiffrer(self.numero.to_string().as_bytes()));
        let session_id: i64 = self.attributs["h"].parse()?;
        let url = format!("{}/appelfonction/{}/{}/{}", self.racine, self.attributs["a"], self.attributs["h"], no);
        let reponse = self
            .http
            .post(&url)
            .json(&json!({ "session": session_id, "no": no, "id": fonction, "dataSec": corps }))
            .send()
            .await?;
        self.numero += 2;

        if !reponse.status().is_success() {
            bail!("Pronote a répondu {} à {fonction}", reponse.status());
        }
        let mut r: Value = reponse.json().await?;
        if let Some(e) = r.get("Erreur") {
            return Err(ErreurPronote::Serveur {
                code: e["G"].as_i64().unwrap_or(0),
                titre: e["Titre"].as_str().unwrap_or_default().to_string(),
            }
            .into());
        }
        if let Some(iv) = nouvel_iv {
            self.chiffrement.iv = iv;
        }

        let mut brut: Option<Vec<u8>> = None;
        if self.chiffrer_requetes {
            let chiffre = hex::decode(r["dataSec"].as_str().context("dataSec absent")?)?;
            let clair = self.chiffrement.dechiffrer(&chiffre)?;
            if self.compresser_requetes {
                brut = Some(clair);
            } else {
                r["dataSec"] = serde_json::from_slice(&clair)?;
            }
        }
        if self.compresser_requetes {
            let compresse = match brut {
                Some(o) => o,
                None => hex::decode(r["dataSec"].as_str().context("dataSec absent")?)?,
            };
            let mut texte = String::new();
            DeflateDecoder::new(&compresse[..]).read_to_string(&mut texte)?;
            r["dataSec"] = serde_json::from_str(&texte).context("réponse Pronote illisible")?;
        }
        Ok(r)
    }

    /// Requête « normale » d'une page de l'élève : signature (onglet) + données.
    pub async fn requete(&mut self, fonction: &str, onglet: Option<u32>, donnees: Option<Value>) -> Result<Value> {
        let mut corps = serde_json::Map::new();
        if let Some(o) = onglet {
            corps.insert("Signature".into(), json!({ "onglet": o }));
        }
        if let Some(d) = donnees {
            corps.insert("data".into(), d);
        }
        self.envoyer(fonction, Value::Object(corps), None).await
    }

    /// Connexion après passage par l'ENT : Pronote fournit lui-même un identifiant (`e`)
    /// et un mot de passe (`f`) de session dans la page.
    async fn connexion_ent(&mut self) -> Result<()> {
        let identifiant = self.attributs.get("e").cloned().ok_or_else(|| {
            ErreurPronote::Identifiants("la page Pronote ne contient pas de jeton ENT (connexion Atrium échouée ?)".into())
        })?;
        let mot_de_passe = self.attributs.get("f").cloned().unwrap_or_default();
        let espace: i64 = self.attributs["a"].parse()?;

        let idr = self
            .requete(
                "Identification",
                None,
                Some(json!({
                    "genreConnexion": 0,
                    "genreEspace": espace,
                    "identifiant": identifiant,
                    "pourENT": true,
                    "enConnexionAuto": false,
                    "demandeConnexionAuto": false,
                    "demandeConnexionAppliMobile": false,
                    "demandeConnexionAppliMobileJeton": false,
                    "enConnexionAppliMobile": false,
                    "uuidAppliMobile": "",
                    "loginTokenSAV": "",
                })),
            )
            .await?;
        let defi = idr["dataSec"]["data"]["challenge"].as_str().context("défi Pronote absent")?;

        // Clé d'authentification ENT : MD5(SHA256(mot de passe) en hexadécimal majuscule).
        let auth = Chiffrement { cle: md5(sha256_hex_maj(&mot_de_passe).as_bytes()), iv: self.chiffrement.iv };
        let reponse_defi = hex::encode(auth.chiffrer(defi.as_bytes()));

        let rep = self
            .requete("Authentification", None, Some(json!({ "connexion": 0, "challenge": reponse_defi, "espace": espace })))
            .await?;
        let cle = rep["dataSec"]["data"]["cle"]
            .as_str()
            .ok_or_else(|| ErreurPronote::Identifiants("Pronote n'a pas accepté l'authentification".into()))?;

        // Nouvelle clé de session : la « clé » reçue est une liste d'octets « 12,34,… », hachée en MD5.
        let decode = auth.dechiffrer(&hex::decode(cle)?)?;
        let liste = String::from_utf8(decode)?;
        let octets: Vec<u8> = liste.split(',').filter_map(|n| n.trim().parse().ok()).collect();
        self.chiffrement.cle = md5(&octets);

        if rep["dataSec"]["data"].get("actionsDoubleAuth").is_some_and(|a| !a.is_null()) {
            bail!("Le compte demande une double authentification (code PIN) : pas encore prise en charge");
        }

        self.utilisateur = self.requete("ParametresUtilisateur", None, None).await?;
        Ok(())
    }

    /// Semaine scolaire d'une date (1 = semaine du premier lundi).
    pub fn semaine(&self, date: NaiveDate) -> i64 {
        1 + (date - self.premier_lundi).num_days().div_euclid(7)
    }

    pub fn racine(&self) -> &str {
        &self.racine
    }

    pub fn http(&self) -> &reqwest::Client {
        &self.http
    }

    /// Chiffrement courant (sert à construire les URL des fichiers joints).
    pub fn chiffrement(&self) -> &Chiffrement {
        &self.chiffrement
    }

    #[allow(dead_code)]
    pub fn page(&self) -> &str {
        &self.page
    }
}

/// Lit les paramètres `Start({h:…, a:…, …})` de la page HTML de Pronote.
pub fn analyser_page(html: &str) -> Result<HashMap<String, String>> {
    // Même test que pronotepy : la page normale ne contient jamais « IP » en majuscules,
    // la page de suspension si.
    if html.contains("IP") {
        return Err(ErreurPronote::IpBloquee.into());
    }
    let re = regex::Regex::new(r#"Start ?\(\{(?P<p>[^}]*)\}\)"#)?;
    let p = re
        .captures(html)
        .and_then(|c| c.name("p"))
        .ok_or_else(|| anyhow!("page Pronote inattendue : vérifie que l'adresse mène bien à la page élève"))?
        .as_str();
    let mut attributs = HashMap::new();
    for paire in p.split(',') {
        if let Some((k, v)) = paire.split_once(':') {
            let nettoie = |s: &str| s.trim().trim_matches(|c| c == '\'' || c == '"').to_string();
            attributs.insert(nettoie(k), nettoie(v));
        }
    }
    if !attributs.contains_key("h") {
        bail!("page Pronote sans identifiant de session");
    }
    Ok(attributs)
}

/// Base64 standard minimal (pour l'IV) : évite une dépendance de plus.
mod base64_mini {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    pub fn encoder(d: &[u8]) -> String {
        let mut s = String::new();
        for c in d.chunks(3) {
            let b = [c[0], *c.get(1).unwrap_or(&0), *c.get(2).unwrap_or(&0)];
            let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
            for i in 0..4 {
                if i <= c.len() {
                    s.push(T[(n >> (18 - 6 * i) & 63) as usize] as char);
                } else {
                    s.push('=');
                }
            }
        }
        s
    }

    #[cfg(test)]
    #[test]
    fn connu() {
        assert_eq!(encoder(b"Man"), "TWFu");
        assert_eq!(encoder(b"Ma"), "TWE=");
        assert_eq!(encoder(b"M"), "TQ==");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_start() {
        let html = "<body onload=\"try { Start ({h:'1234567',a:3,d:true,e:'ABC',f:'XYZ',CrA:true}) } catch (e) {}\">";
        let a = analyser_page(html).unwrap();
        assert_eq!(a["h"], "1234567");
        assert_eq!(a["a"], "3");
        assert_eq!(a["e"], "ABC");
        assert!(a.contains_key("CrA"));
    }
}
