//! Connexion à l'ENT Atrium (Région Sud), avant d'ouvrir Pronote :
//! - via ÉduConnect (bouton « élève ») : Atrium → requête SAML → ÉduConnect → réponse SAML → Atrium ;
//! - ou avec un compte Atrium (formulaire `fm1`).
//! Le client HTTP garde les cookies : une fois connecté à Atrium, Pronote nous laisse entrer.

use anyhow::{Result, anyhow, bail};
use reqwest::{Client, Url};
use scraper::{ElementRef, Html, Selector};

use super::session::{ErreurPronote, USER_AGENT};

const ATRIUM_LOGIN: &str = "https://www.atrium-sud.fr/connexion/login";

struct Page {
    url: Url,
    html: String,
}

pub fn client_http() -> Result<Client> {
    Ok(Client::builder()
        .cookie_store(true)
        .user_agent(USER_AGENT)
        .redirect(reqwest::redirect::Policy::limited(20))
        .timeout(std::time::Duration::from_secs(30))
        .build()?)
}

async fn lire(r: reqwest::Response) -> Result<Page> {
    let url = r.url().clone();
    Ok(Page { url, html: r.text().await? })
}

/// Champs d'un formulaire, dans l'ordre (nom → valeur).
fn champs(form: ElementRef) -> Vec<(String, String)> {
    let sel = Selector::parse("input").unwrap();
    form.select(&sel)
        .filter_map(|i| {
            let nom = i.value().attr("name")?;
            Some((nom.to_string(), i.value().attr("value").unwrap_or_default().to_string()))
        })
        .collect()
}

fn remplacer(champs: &mut Vec<(String, String)>, nom: &str, valeur: &str) {
    match champs.iter_mut().find(|(n, _)| n == nom) {
        Some(c) => c.1 = valeur.to_string(),
        None => champs.push((nom.to_string(), valeur.to_string())),
    }
}

/// Formulaire (action absolue + champs) choisi par un sélecteur, ou celui qui contient un champ donné.
fn trouver_form(page: &Page, selecteur: &str) -> Option<(Url, Vec<(String, String)>)> {
    let doc = Html::parse_document(&page.html);
    let sel = Selector::parse(selecteur).ok()?;
    let el = doc.select(&sel).next()?;
    // Remonter jusqu'au <form> si le sélecteur vise un champ.
    let form = if el.value().name() == "form" {
        el
    } else {
        el.ancestors().filter_map(ElementRef::wrap).find(|e| e.value().name() == "form")?
    };
    let action = form.value().attr("action").unwrap_or("");
    let url = if action.is_empty() { page.url.clone() } else { page.url.join(action).ok()? };
    Some((url, champs(form)))
}

async fn soumettre(http: &Client, url: Url, champs: &[(String, String)]) -> Result<Page> {
    lire(http.post(url).form(champs).send().await?).await
}

/// Suit les pages qui renvoient automatiquement un formulaire SAML (normalement soumis en JS).
async fn suivre_saml(http: &Client, mut page: Page, cle: &str) -> Result<Page> {
    for _ in 0..5 {
        match trouver_form(&page, &format!("input[name=\"{cle}\"]")) {
            Some((url, c)) => page = soumettre(http, url, &c).await?,
            None => break,
        }
    }
    Ok(page)
}

fn message_erreur(page: &Page) -> Option<String> {
    let doc = Html::parse_document(&page.html);
    let sel = Selector::parse("[class*=error], [class*=erreur], .alert").ok()?;
    let t: String = doc.select(&sel).next()?.text().collect::<Vec<_>>().join(" ");
    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
    (!t.is_empty()).then_some(t)
}

/// Atrium via le bouton ÉduConnect « élève ».
pub async fn connexion_educonnect(http: &Client, identifiant: &str, mot_de_passe: &str) -> Result<()> {
    let page = lire(http.get(ATRIUM_LOGIN).send().await?).await?;
    let (url, mut c) = trouver_form(&page, "form#educonnectQuickForm")
        .ok_or_else(|| anyhow!("bouton ÉduConnect introuvable sur la page Atrium (la page a changé ?)"))?;
    remplacer(&mut c, "educonnect_profile", "eleve");
    let mut page = suivre_saml(http, soumettre(http, url, &c).await?, "SAMLRequest").await?;
    if !page.url.as_str().contains("educonnect") {
        bail!("redirection vers ÉduConnect ratée (arrivé sur {})", page.url.path());
    }

    // ÉduConnect affiche d'abord une page de « stockage local » soumise en JS, puis le login.
    let mut login = None;
    for _ in 0..3 {
        if let Some(f) = trouver_form(&page, "input[name=\"j_username\"]") {
            login = Some(f);
            break;
        }
        let Some((url, mut c)) = trouver_form(&page, "form") else { break };
        remplacer(&mut c, "shib_idp_ls_supported", "");
        page = soumettre(http, url, &c).await?;
    }
    let (url, mut c) = login.ok_or_else(|| anyhow!("formulaire de connexion ÉduConnect introuvable (la page a changé ?)"))?;
    remplacer(&mut c, "j_username", identifiant);
    remplacer(&mut c, "j_password", mot_de_passe);
    remplacer(&mut c, "_eventId_proceed", "");
    let page = soumettre(http, url, &c).await?;

    if trouver_form(&page, "input[name=\"j_password\"]").is_some() {
        let detail = message_erreur(&page).unwrap_or_else(|| "identifiant ou mot de passe incorrect".into());
        return Err(ErreurPronote::Identifiants(format!("ÉduConnect : {detail}")).into());
    }
    if !page.html.contains("SAMLResponse") {
        bail!("ÉduConnect demande une étape inattendue : connecte-toi une fois dans un navigateur pour la valider");
    }
    let page = suivre_saml(http, page, "SAMLResponse").await?;
    if !page.url.as_str().contains("atrium-sud.fr") {
        bail!("retour vers Atrium raté (arrivé sur {})", page.url.path());
    }
    Ok(())
}

/// Atrium avec un compte Atrium (formulaire classique).
pub async fn connexion_compte_atrium(http: &Client, identifiant: &str, mot_de_passe: &str) -> Result<()> {
    let page = lire(http.get(ATRIUM_LOGIN).send().await?).await?;
    let (url, mut c) = trouver_form(&page, "form#fm1").ok_or_else(|| anyhow!("formulaire Atrium introuvable"))?;
    remplacer(&mut c, "username", identifiant);
    remplacer(&mut c, "password", mot_de_passe);
    let page = soumettre(http, url, &c).await?;
    if trouver_form(&page, "form#fm1 input[name=\"password\"]").is_some() {
        let detail = message_erreur(&page).unwrap_or_else(|| "identifiant ou mot de passe incorrect".into());
        return Err(ErreurPronote::Identifiants(format!("Atrium : {detail}")).into());
    }
    Ok(())
}
