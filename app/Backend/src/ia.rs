//! L'IA locale : llama.cpp (`llama-server`) lancé par l'app, sans rien installer à côté.
//!
//! - **GPU** : un seul modèle d'écriture (Qwen 3.5 9B, 4 bits), chargé à la demande et déchargé après
//!   10 minutes sans usage pour rendre la carte graphique (jeux, vidéo…). `--fit` règle tout seul le nombre
//!   de couches selon la VRAM libre (Windows en garde déjà ~1 Go).
//! - **CPU** : le petit modèle d'embeddings (recherche par le sens), qui ne prend jamais de VRAM.
//! - **OCR** : GLM-OCR via `llama-mtmd-cli`, ponctuel ; le serveur GPU est arrêté le temps de la lecture.
//!
//! Les processus sont rattachés à un « job » Windows : si l'app se ferme ou plante, ils s'arrêtent avec elle.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{LazyLock, Mutex, Once, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Serialize;
use serde_json::{Value, json};

use crate::config;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

const MODELE_ECRITURE: &str = "Qwen3.5-9B-UD-Q4_K_XL.gguf";
const MODELE_EMBEDDINGS: &str = "embeddinggemma-300m-qat-Q8_0.gguf";
const MODELE_OCR: &str = "GLM-OCR-Q8_0.gguf";
const VISION_OCR: &str = "mmproj-GLM-OCR-Q8_0.gguf";

const PORT_GPU: u16 = 8181;
const PORT_EMB: u16 = 8182;
/// Au-delà, on décharge le modèle de la carte graphique.
const INACTIVITE_MAX: Duration = Duration::from_secs(10 * 60);
/// Réflexion plafonnée : sans plafond Qwen réfléchit 3 000 jetons et plus (75 s) pour une question simple ;
/// à 800 il reste juste et met ~20 s (banc d'essai du 26/11, docs/ia-locale.md).
const BUDGET_REFLEXION: &str = "800";

pub fn dossier_ia() -> PathBuf {
    config::dossier_utilisateur().join(".revisions").join("ia")
}

fn bin(nom: &str) -> PathBuf {
    dossier_ia().join("bin").join(nom)
}

fn modele(nom: &str) -> PathBuf {
    dossier_ia().join("modeles").join(nom)
}

// ---------------------------------------------------------------- processus

struct Serveur {
    enfant: Child,
    pret: bool,
}

static GPU: Mutex<Option<Serveur>> = Mutex::new(None);
static EMB: Mutex<Option<Serveur>> = Mutex::new(None);
/// Les requêtes au modèle prennent ce verrou en lecture ; l'OCR, qui a besoin de toute la VRAM, en écriture.
static CARTE: RwLock<()> = RwLock::new(());
static DERNIER_USAGE: AtomicU64 = AtomicU64::new(0);
static EN_VOL: AtomicUsize = AtomicUsize::new(0);
static SURVEILLANCE: Once = Once::new();

fn maintenant_s() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn commande(exe: &Path) -> Command {
    let mut c = Command::new(exe);
    #[cfg(windows)]
    c.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    c.stdin(Stdio::null());
    c
}

/// Rattache un processus au job de l'app : il meurt avec elle.
#[cfg(windows)]
fn rattacher(enfant: &Child) {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::*;
    static JOB: LazyLock<usize> = LazyLock::new(|| unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            &info as *const _ as *const _,
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        );
        job as usize
    });
    unsafe {
        AssignProcessToJobObject(*JOB as _, enfant.as_raw_handle() as _);
    }
}

#[cfg(not(windows))]
fn rattacher(_enfant: &Child) {}

fn journal(nom: &str) -> Stdio {
    let f = dossier_ia().join(format!("{nom}.log"));
    std::fs::File::create(f).map(Stdio::from).unwrap_or_else(|_| Stdio::null())
}

fn vivant(s: &mut Option<Serveur>) -> bool {
    match s {
        Some(srv) => match srv.enfant.try_wait() {
            Ok(None) => true,
            _ => {
                *s = None;
                false
            }
        },
        None => false,
    }
}

fn attendre_sante(port: u16, enfant: &mut Child, delai: Duration) -> Result<()> {
    let debut = Instant::now();
    while debut.elapsed() < delai {
        if let Ok(Some(code)) = enfant.try_wait() {
            bail!("le serveur IA s'est arrêté au démarrage ({code}) : voir {}", dossier_ia().display());
        }
        if let Ok(r) = CLIENT.get(format!("http://127.0.0.1:{port}/health")).timeout(Duration::from_secs(2)).send() {
            if r.status().is_success() {
                return Ok(());
            }
        }
        std::thread::sleep(Duration::from_millis(300));
    }
    bail!("le serveur IA ne répond pas après {} s", delai.as_secs())
}

fn verifier_fichiers(noms: &[&str]) -> Result<()> {
    if !bin("llama-server.exe").exists() {
        bail!("llama.cpp est introuvable dans {} : voir docs/ia-locale.md", dossier_ia().join("bin").display());
    }
    for n in noms {
        if !modele(n).exists() {
            bail!("modèle manquant : {}", modele(n).display());
        }
    }
    Ok(())
}

/// Démarre le serveur d'écriture s'il ne tourne pas (et lance la surveillance d'inactivité).
fn assurer_gpu() -> Result<()> {
    SURVEILLANCE.call_once(|| {
        std::thread::spawn(|| loop {
            std::thread::sleep(Duration::from_secs(30));
            let inactif = maintenant_s().saturating_sub(DERNIER_USAGE.load(Ordering::Relaxed));
            if EN_VOL.load(Ordering::SeqCst) == 0 && inactif > INACTIVITE_MAX.as_secs() {
                arreter_gpu();
            }
        });
    });
    let mut g = GPU.lock().unwrap();
    if vivant(&mut g) {
        return Ok(());
    }
    verifier_fichiers(&[MODELE_ECRITURE])?;
    let mut enfant = commande(&bin("llama-server.exe"))
        .args(["-m"])
        .arg(modele(MODELE_ECRITURE))
        .args(["-c", "8192", "--flash-attn", "on", "-ctk", "q8_0", "-ctv", "q8_0", "--jinja", "--parallel", "1"])
        .args(["--fit-target", "256"])
        .args(["--reasoning-budget", BUDGET_REFLEXION])
        .args(["--reasoning-budget-message", " Je conclus maintenant et j'écris la réponse."])
        .args(["--host", "127.0.0.1", "--port", &PORT_GPU.to_string()])
        .stdout(Stdio::null())
        .stderr(journal("serveur-gpu")) // llama-server écrit son journal sur stderr
        .spawn()
        .context("lancement de llama-server")?;
    rattacher(&enfant);
    attendre_sante(PORT_GPU, &mut enfant, Duration::from_secs(120))?;
    *g = Some(Serveur { enfant, pret: true });
    Ok(())
}

/// Décharge le modèle d'écriture (rend la VRAM).
pub fn arreter_gpu() {
    if let Some(mut s) = GPU.lock().unwrap().take() {
        let _ = s.enfant.kill();
        let _ = s.enfant.wait();
    }
}

fn assurer_embeddings() -> Result<()> {
    let mut e = EMB.lock().unwrap();
    if vivant(&mut e) {
        return Ok(());
    }
    verifier_fichiers(&[MODELE_EMBEDDINGS])?;
    let fils = std::thread::available_parallelism().map(|n| (n.get() / 2).max(2)).unwrap_or(4);
    let mut enfant = commande(&bin("llama-server.exe"))
        .args(["-m"])
        .arg(modele(MODELE_EMBEDDINGS))
        .args(["--embedding", "--pooling", "mean", "-ngl", "0", "-c", "2048", "-b", "2048", "-ub", "2048"])
        .args(["-t", &fils.to_string(), "--host", "127.0.0.1", "--port", &PORT_EMB.to_string()])
        .stdout(Stdio::null())
        .stderr(journal("serveur-embeddings"))
        .spawn()
        .context("lancement du serveur d'embeddings")?;
    rattacher(&enfant);
    attendre_sante(PORT_EMB, &mut enfant, Duration::from_secs(60))?;
    *e = Some(Serveur { enfant, pret: true });
    Ok(())
}

static CLIENT: LazyLock<reqwest::blocking::Client> = LazyLock::new(|| {
    reqwest::blocking::Client::builder().timeout(Duration::from_secs(600)).build().expect("client HTTP")
});

// ---------------------------------------------------------------- discussion

#[derive(Default, Clone)]
pub struct Options {
    /// Schéma JSON imposé à la réponse (grammaire : toujours valide). Ignoré en mode réflexion.
    pub schema: Option<Value>,
    /// Le modèle réfléchit avant de répondre (plafonné) : pour vérifier un calcul, résoudre un QCM.
    pub reflexion: bool,
    /// Outils (format OpenAI) que le modèle peut appeler.
    pub outils: Option<Value>,
    pub max_jetons: Option<u32>,
    pub temperature: Option<f64>,
}

pub struct Reponse {
    /// Le message brut de l'assistant (à remettre dans la conversation après un appel d'outil).
    pub message: Value,
    pub contenu: String,
    /// (id, nom, arguments JSON)
    pub appels: Vec<(String, String, Value)>,
    #[allow(dead_code)] // pour le suivi des performances
    pub jetons: u64,
    #[allow(dead_code)]
    pub duree: Duration,
}

/// Une requête au modèle d'écriture. Bloquante : à appeler depuis un fil de travail.
pub fn discuter(messages: &[Value], o: &Options) -> Result<Reponse> {
    let _carte = CARTE.read().unwrap();
    EN_VOL.fetch_add(1, Ordering::SeqCst);
    DERNIER_USAGE.store(maintenant_s(), Ordering::Relaxed);
    let r = (|| {
        assurer_gpu()?;
        // Réglages recommandés par Qwen : réflexion 0.6/0.95/20, sans réflexion 0.7/0.8/20.
        let mut corps = json!({
            "messages": messages,
            "chat_template_kwargs": { "enable_thinking": o.reflexion },
            "cache_prompt": true,
            "temperature": o.temperature.unwrap_or(if o.reflexion { 0.6 } else { 0.7 }),
            "top_p": if o.reflexion { 0.95 } else { 0.8 },
            "top_k": 20,
            "presence_penalty": 1.0,
            "max_tokens": o.max_jetons.unwrap_or(if o.reflexion { 2500 } else { 1200 }),
        });
        if let Some(s) = &o.schema {
            if !o.reflexion {
                corps["response_format"] = json!({ "type": "json_schema", "json_schema": { "name": "reponse", "schema": s } });
            }
        }
        if let Some(t) = &o.outils {
            corps["tools"] = t.clone();
        }
        let debut = Instant::now();
        let r: Value = CLIENT
            .post(format!("http://127.0.0.1:{PORT_GPU}/v1/chat/completions"))
            .json(&corps)
            .send()
            .context("le serveur IA ne répond pas")?
            .error_for_status()
            .context("le serveur IA a refusé la requête")?
            .json()?;
        let m = &r["choices"][0]["message"];
        let appels = m["tool_calls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|a| {
                let args = a["function"]["arguments"].as_str().and_then(|s| serde_json::from_str(s).ok()).unwrap_or(Value::Null);
                Some((a["id"].as_str().unwrap_or("").to_string(), a["function"]["name"].as_str()?.to_string(), args))
            })
            .collect();
        Ok(Reponse {
            message: m.clone(),
            contenu: m["content"].as_str().unwrap_or("").to_string(),
            appels,
            jetons: r["usage"]["completion_tokens"].as_u64().unwrap_or(0),
            duree: debut.elapsed(),
        })
    })();
    DERNIER_USAGE.store(maintenant_s(), Ordering::Relaxed);
    EN_VOL.fetch_sub(1, Ordering::SeqCst);
    r
}

/// Raccourci : un message système + un message utilisateur → objet JSON.
pub fn demander_json(systeme: &str, texte: &str, o: &Options) -> Result<Value> {
    let mut texte = texte.to_string();
    if o.reflexion {
        if let Some(s) = &o.schema {
            // Pas de grammaire en mode réflexion (elle gênerait la réflexion) : on décrit la forme attendue.
            texte.push_str(&format!("\n\nRéponds uniquement avec un objet JSON conforme à ce schéma :\n{s}"));
        }
    }
    let r = discuter(&[json!({"role": "system", "content": systeme}), json!({"role": "user", "content": texte})], o)?;
    extraire_json(&r.contenu)
}

/// Le premier objet JSON d'une réponse (le modèle l'entoure parfois de texte ou de ```json).
pub fn extraire_json(texte: &str) -> Result<Value> {
    let debut = texte.find('{').context("pas de JSON dans la réponse du modèle")?;
    let fin = texte.rfind('}').context("JSON incomplet dans la réponse du modèle")?;
    if fin <= debut {
        bail!("JSON incomplet dans la réponse du modèle");
    }
    serde_json::from_str(&texte[debut..=fin]).context("JSON illisible dans la réponse du modèle")
}

// ---------------------------------------------------------------- embeddings

/// Vecteurs de sens (768 dimensions, normalisés). `requete` : préfixe de recherche, sinon de document.
pub fn embeddings(textes: &[String], requete: bool) -> Result<Vec<Vec<f32>>> {
    if cfg!(test) {
        bail!("pas de serveur d'embeddings pendant les tests");
    }
    if textes.is_empty() {
        return Ok(vec![]);
    }
    assurer_embeddings()?;
    let mut tout = Vec::with_capacity(textes.len());
    for paquet in textes.chunks(16) {
        let entrees: Vec<String> = paquet
            .iter()
            .map(|t| {
                let t: String = t.chars().take(1800).collect();
                if requete { format!("task: search result | query: {t}") } else { format!("title: none | text: {t}") }
            })
            .collect();
        let r: Value = CLIENT
            .post(format!("http://127.0.0.1:{PORT_EMB}/v1/embeddings"))
            .json(&json!({ "input": entrees }))
            .send()?
            .error_for_status()?
            .json()?;
        for d in r["data"].as_array().context("réponse d'embeddings illisible")? {
            let mut v: Vec<f32> = d["embedding"].as_array().into_iter().flatten().filter_map(|x| x.as_f64()).map(|x| x as f32).collect();
            let n = v.iter().map(|x| x * x).sum::<f32>().sqrt().max(1e-9);
            v.iter_mut().for_each(|x| *x /= n);
            tout.push(v);
        }
    }
    Ok(tout)
}

pub fn cosinus(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

// ---------------------------------------------------------------- OCR

/// Lit le texte d'une image (document scanné, photo de cours). Libère la carte graphique le temps de la lecture.
pub fn ocr(image: &Path) -> Result<String> {
    verifier_fichiers(&[MODELE_OCR, VISION_OCR])?;
    let _carte = CARTE.write().unwrap();
    arreter_gpu();
    let sortie = commande(&bin("llama-mtmd-cli.exe"))
        .arg("-m")
        .arg(modele(MODELE_OCR))
        .arg("--mmproj")
        .arg(modele(VISION_OCR))
        .args(["-ngl", "99", "--temp", "0", "-n", "2500", "-p", "Text Recognition:"])
        .arg("--image")
        .arg(image)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("lancement de l'OCR")?;
    if !sortie.status.success() {
        bail!("l'OCR a échoué sur {}", image.display());
    }
    Ok(String::from_utf8_lossy(&sortie.stdout).trim().to_string())
}

// ---------------------------------------------------------------- état (pour l'interface)

#[derive(Serialize)]
pub struct EtatIa {
    /// llama.cpp et les modèles sont là.
    pub installe: bool,
    pub manquants: Vec<String>,
    /// arrete | pret
    pub ecriture: String,
    pub recherche: String,
    pub modele: String,
    pub dossier: String,
}

pub fn etat() -> EtatIa {
    let mut manquants = Vec::new();
    if !bin("llama-server.exe").exists() {
        manquants.push("llama.cpp (llama-server.exe)".to_string());
    }
    for n in [MODELE_ECRITURE, MODELE_EMBEDDINGS, MODELE_OCR, VISION_OCR] {
        if !modele(n).exists() {
            manquants.push(n.to_string());
        }
    }
    let statut = |m: &Mutex<Option<Serveur>>| {
        let mut g = m.lock().unwrap();
        if vivant(&mut g) && g.as_ref().is_some_and(|s| s.pret) { "pret" } else { "arrete" }.to_string()
    };
    EtatIa {
        installe: manquants.is_empty(),
        manquants,
        ecriture: statut(&GPU),
        recherche: statut(&EMB),
        modele: MODELE_ECRITURE.trim_end_matches(".gguf").to_string(),
        dossier: dossier_ia().display().to_string(),
    }
}

/// Prépare le modèle d'écriture à l'avance (à l'ouverture de l'app) pour que la 1re question arrive vite.
pub fn prechauffer() {
    std::thread::spawn(|| {
        DERNIER_USAGE.store(maintenant_s(), Ordering::Relaxed);
        if let Err(e) = assurer_gpu() {
            eprintln!("IA locale : {e:#}");
        }
        let _ = assurer_embeddings();
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_entoure() {
        let v = extraire_json("Voici :\n```json\n{\"a\": {\"b\": 1}}\n```").unwrap();
        assert_eq!(v["a"]["b"], 1);
    }
}
