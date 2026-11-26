//! Écrire les questions avec l'IA locale : le code décide, le modèle rédige, le code vérifie.
//!
//! - **Le code décide** : quelle notion (fiche de la leçon, ou notions faibles), quel format (selon la matière :
//!   en maths que des formats vérifiables), quel raisonnement demander, quel passage du cours sert de document.
//! - **Le modèle rédige UNE question** à partir d'un contexte court (fiche + extraits du cours) et peut aller
//!   chercher dans le cours avec un outil s'il lui manque une info.
//! - **Le code vérifie** : pas de récitation, l'énoncé ne donne pas la réponse, rien de hors programme, maths
//!   lisibles, calcul recalculé par le code, QCM résolu à l'aveugle par le modèle (réflexion). Une question
//!   rejetée est réécrite une fois, sinon on passe.
//!
//! Le principe des questions (voir SYSTEME) : une question de **réflexion qui exige une connaissance précise** du
//! cours. Sans la connaissance, on ne peut pas répondre juste ; en la récitant, non plus.

use std::sync::LazyLock;

use anyhow::{Context, Result, bail};
use regex::Regex;
use rusqlite::Connection;
use serde_json::{Value, json};

use crate::connaissances::{self, Fiche, Notion, SourceCours};
use crate::revision_jour::QuestionJour;
use crate::{ia, suivi};

// ---------------------------------------------------------------- matières et formats

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Categorie {
    Maths,
    Sciences,
    Langue,
    Francais,
    Humanites,
}

pub fn categorie(matiere: &str) -> Categorie {
    let s = suivi::slug(matiere);
    let a = |mots: &[&str]| mots.iter().any(|m| s.contains(m));
    if a(&["math"]) {
        Categorie::Maths
    } else if a(&["physique", "chimie", "vie-terre", "svt", "snt", "numerique", "ingenieur", "opt-si", "sciences-de-l"]) {
        Categorie::Sciences
    } else if a(&["anglais", "italien", "espagnol", "allemand", "lv1", "lv2", "lv3", "latin", "portugais", "chinois", "arabe"]) {
        Categorie::Langue
    } else if a(&["francais", "litterature"]) {
        Categorie::Francais
    } else {
        Categorie::Humanites
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    /// QCM à 4 choix, vérifié par une résolution à l'aveugle.
    Qcm,
    /// Résultat chiffré, recalculé par le code.
    Calcul,
    /// Réponse courte en appliquant une règle (langues).
    Courte,
    /// Réponse rédigée courte : expliquer une cause, un mécanisme.
    Pourquoi,
    /// Réponse rédigée à partir d'un document du cours (fourni par le code).
    Document,
}

/// Les formats d'une série de 6, dans l'ordre. Le 1er ne demande pas de vérification par le modèle :
/// la première question arrive vite.
fn plan(cat: Categorie, a_document: bool) -> Vec<Format> {
    use Format::*;
    let doc = if a_document { Document } else { Pourquoi };
    match cat {
        Categorie::Maths => vec![Calcul, Qcm, Calcul, Qcm, Calcul, Qcm],
        Categorie::Sciences => vec![doc, Qcm, Pourquoi, Qcm, Calcul, Qcm],
        Categorie::Langue => vec![Courte, Qcm, doc, Courte, Qcm, Courte],
        Categorie::Francais => vec![doc, Qcm, Pourquoi, Qcm, doc, Pourquoi],
        Categorie::Humanites => vec![doc, Qcm, Pourquoi, Qcm, doc, Pourquoi],
    }
}

/// Les opérations de réflexion qu'on fait tourner, pour varier les questions.
const RAISONNEMENTS: &[&str] = &[
    "appliquer la connaissance à un cas nouveau (une situation, un exemple, une phrase que le cours n'a pas traités)",
    "expliquer une cause ou un mécanisme (pourquoi / comment), en s'appuyant sur la connaissance",
    "prévoir une conséquence ou un résultat dans une situation donnée",
    "repérer et corriger une erreur de raisonnement plausible d'un élève",
    "comparer deux notions, deux cas ou deux situations proches et dire ce qui change",
    "choisir entre deux interprétations et justifier avec la connaissance",
];

const HORS_PROGRAMME_MATHS: &str = r"(?i)d[ée]riv[ée]e|f\s*'\s*\(|\blimite|exponentielle|logarithme|\bln\s*\(|int[ée]grale|primitive|produit scalaire|nombres? complexes?|suite (g[ée]om[ée]trique|arithm[ée]tique)|loi binomiale|probabilit[ée]s? conditionnelles?";

// ---------------------------------------------------------------- consignes

const SYSTEME: &str = r#"Tu écris UNE question de révision pour un élève de seconde générale, à partir de SON cours (la fiche de la leçon et les extraits fournis).

LE PRINCIPE : une question de réflexion qui EXIGE une connaissance précise du cours.
Commence par choisir :
- "connaissance_requise" : LA connaissance précise du cours, indispensable pour répondre (définition, règle, mécanisme, fait, date, personnage, méthode), en une phrase ;
- "raisonnement" : ce que l'élève doit FAIRE avec cette connaissance pour trouver la réponse.
Puis écris l'énoncé. Il doit passer le TEST DES DEUX ÉLÈVES :
1. Un élève malin qui n'a PAS appris ce cours ne peut pas répondre juste : l'énoncé ne contient pas la connaissance, ne la laisse pas deviner, et le bon sens ou la culture générale ne suffisent pas.
2. Un élève qui a appris par cœur sans comprendre ne peut pas répondre juste non plus : il faut se SERVIR de la connaissance, pas la réciter.

Interdit : faire réciter (« Donne la définition », « Qu'est-ce que », « Cite », « Qui a… », « En quelle année… »), les questions de bon sens, les questions sur l'organisation du cours ou de l'évaluation, les questions dont la réponse est dans l'énoncé.

Exemples :
- Mauvais (récitation) : « Qu'est-ce que la photosynthèse ? »
- Mauvais (bon sens) : « Pourquoi une plante a-t-elle besoin de lumière ? »
- Bon : « Une algue reste 2 h dans le noir, dans une eau riche en CO₂. Comment évoluent l'O₂ et le CO₂ de l'eau, et pourquoi ? » (il faut savoir qu'à l'obscurité seule la respiration a lieu)
- Mauvais : « Que signifie f(−x) = −f(x) ? »  Bon : « f est impaire et f(2) = 5. Que vaut f(−2) ? »
- Bon : « Un pays a une natalité de 40 ‰ et une mortalité de 12 ‰ qui baisse vite. Dans quelle phase de la transition démographique est-il, et qu'arrive-t-il à sa population ? »

Règles :
- JUSTESSE AVANT TOUT : tout ce que la question affirme ou suppose est vrai, et la réponse attendue est exacte. Fais le calcul ou applique la règle toi-même avant d'écrire. Si tu as un doute, choisis une autre question dont tu es sûr.
- Niveau seconde : rien de hors programme.
- Reste dans ce que le cours de l'élève a traité. S'il te manque une information du cours, appelle l'outil chercher_dans_le_cours.
- Tutoie l'élève. Phrases courtes et claires.
- Langue : tout en français, SAUF en langue vivante où l'énoncé, les choix et les réponses attendues sont dans la langue étudiée ; "connaissance_requise", "raisonnement" et "explication" restent en français.
- Maths : texte simple avec les symboles Unicode (ℝ, ∞, x², √, ≤, ≥, ≠, ∈, π, −). Jamais de LaTeX, jamais de $.
- N'écris jamais la connaissance dans l'énoncé, même comme rappel ou hypothèse (« Sachant que… », « Si tu sais que… », « On rappelle que… ») : c'est elle qu'on teste.
- "explication" : 1 à 3 phrases adressées à l'élève (tu), qui rappellent la connaissance et montrent comment on s'en sert ici.

Réponds avec un objet JSON contenant, dans cet ordre, "connaissance_requise", "raisonnement", puis les champs demandés pour le format."#;

fn consigne_format(f: Format, cat: Categorie) -> &'static str {
    match f {
        Format::Qcm => "Format : QCM à 4 choix, une seule bonne réponse. Les 3 mauvais choix sont des erreurs typiques d'un élève qui a mal appris ou mal compris (confusion de notions, règle mal appliquée, cause et conséquence inversées) : plausibles, de même longueur et de même forme que la bonne. Champs : \"enonce\", \"choix\" (4), \"bonne\" (0 à 3), \"erreurs\" (4 éléments : l'erreur que révèle chaque choix, null pour la bonne réponse), \"explication\".",
        Format::Calcul => "Format : réponse numérique. Champs : \"enonce\", \"calcul\" (l'expression qui donne le résultat, avec seulement des nombres, + - * / ^, des parenthèses et sqrt() — le code va la recalculer), \"valeur\" (le résultat), \"unite\" (ou null), \"explication\".",
        Format::Courte => if cat == Categorie::Langue {
            "Format : réponse courte (un mot, une forme, une courte phrase) qu'on obtient en appliquant une règle ou le sens étudiés à une phrase NOUVELLE, dans la langue étudiée. Champs : \"enonce\", \"reponses_acceptees\" (toutes les variantes justes, 1 à 6), \"explication\"."
        } else {
            "Format : réponse courte (un mot, un nombre, une courte phrase). Champs : \"enonce\", \"reponses_acceptees\" (toutes les variantes justes, 1 à 6), \"explication\"."
        },
        Format::Pourquoi => "Format : réponse rédigée courte (2 à 4 phrases) qui explique une cause, un mécanisme ou une conséquence. Champs : \"enonce\", \"attendus\" (2 à 4 points précis qu'une bonne réponse contient), \"explication\".",
        Format::Document => "Format : question sur le DOCUMENT ci-dessus (c'est un document du cours, il sera affiché à l'élève à côté de sa réponse). Pour répondre juste, il faut exploiter le document ET la connaissance du cours : ni le document seul, ni le cours seul ne suffisent. Champs : \"enonce\", \"plan\" (2 ou 3 parties courtes de la réponse), \"attendus\" (3 à 5 points, dont au moins un qui exploite précisément le document : citation, chiffre, détail), \"explication\".",
    }
}

fn schema(f: Format) -> Value {
    let s = |d: &str| json!({ "type": "string", "description": d });
    let liste = |min: u32, max: u32| json!({ "type": "array", "items": { "type": "string" }, "minItems": min, "maxItems": max });
    let mut props = serde_json::Map::new();
    props.insert("connaissance_requise".into(), s("la connaissance précise du cours indispensable pour répondre"));
    props.insert("raisonnement".into(), s("ce que l'élève doit faire avec cette connaissance"));
    props.insert("enonce".into(), s("la question posée à l'élève"));
    let mut requis = vec!["connaissance_requise", "raisonnement", "enonce"];
    match f {
        Format::Qcm => {
            props.insert("choix".into(), liste(4, 4));
            props.insert("bonne".into(), json!({ "type": "integer", "minimum": 0, "maximum": 3 }));
            props.insert("erreurs".into(), json!({ "type": "array", "items": { "type": ["string", "null"] }, "minItems": 4, "maxItems": 4 }));
            requis.extend(["choix", "bonne", "erreurs"]);
        }
        Format::Calcul => {
            props.insert("calcul".into(), s("expression numérique recalculable"));
            props.insert("valeur".into(), json!({ "type": "number" }));
            props.insert("unite".into(), json!({ "type": ["string", "null"] }));
            requis.extend(["calcul", "valeur", "unite"]);
        }
        Format::Courte => {
            props.insert("reponses_acceptees".into(), liste(1, 6));
            requis.push("reponses_acceptees");
        }
        Format::Pourquoi => {
            props.insert("attendus".into(), liste(2, 4));
            requis.push("attendus");
        }
        Format::Document => {
            props.insert("plan".into(), liste(2, 3));
            props.insert("attendus".into(), liste(3, 5));
            requis.extend(["plan", "attendus"]);
        }
    }
    props.insert("explication".into(), s("la correction en 1 à 3 phrases"));
    requis.push("explication");
    json!({ "type": "object", "properties": props, "required": requis })
}

fn outil_recherche() -> Value {
    json!([{
        "type": "function",
        "function": {
            "name": "chercher_dans_le_cours",
            "description": "Cherche dans les cours, devoirs et documents de l'élève pour cette matière. Renvoie les passages les plus proches de la requête.",
            "parameters": {
                "type": "object",
                "properties": { "requete": { "type": "string", "description": "ce que tu cherches (mots-clés ou question)" } },
                "required": ["requete"]
            }
        }
    }])
}

// ---------------------------------------------------------------- contrôles par le code

static RECITATION: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^\s*(donne(z)? la d[ée]finition|quelle est la d[ée]finition|qu'est[- ]ce qu|que signifie|que veut dire|cite(z)? |rappelle(z)? |qui a |qui est |qui était |en quelle ann[ée]e|quand a eu lieu|what is the definition|define )").unwrap()
});
/// Les tournures qui donnent la connaissance au lieu de la demander.
static INDICE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(sachant qu|si tu sais qu|tu sais qu|on (te )?rappelle|rappel ?:|on sait qu|comme tu l'as vu|d'apr[eè]s le cours,? )").unwrap()
});
static HORS_PROGRAMME: LazyLock<Regex> = LazyLock::new(|| Regex::new(HORS_PROGRAMME_MATHS).unwrap());

const MOTS_VIDES: &[&str] = &[
    "alors", "avant", "avoir", "cette", "comme", "dans", "donc", "elles", "entre", "est-ce", "faire", "leurs", "lorsque", "mais",
    "parce", "pendant", "permet", "peuvent", "plus", "pour", "quand", "quelle", "quels", "sans", "selon", "sont", "sous", "toujours",
    "tous", "toutes", "très", "une", "avec", "aussi", "elle", "nous", "vous", "leur", "même", "être", "dont", "cela", "ceci", "chaque",
    "doit", "peut", "fait", "ainsi", "celui", "celle", "ceux", "autre", "autres", "deux", "trois",
];

fn mots_cles(t: &str) -> Vec<String> {
    let n = suivi::slug(t).replace('-', " ");
    let mut v: Vec<String> = n.split_whitespace().filter(|m| m.chars().count() >= 5 && !MOTS_VIDES.contains(m)).map(String::from).collect();
    v.sort();
    v.dedup();
    v
}

/// LaTeX qui s'est glissé malgré la consigne → Unicode. Renvoie None si ça reste illisible.
pub fn maths_lisibles(t: &str) -> Option<String> {
    if !t.contains('$') && !t.contains('\\') {
        return Some(t.to_string());
    }
    let mut s = t.replace('$', "");
    for (a, b) in [
        ("\\mathbb{R}", "ℝ"), ("\\mathbb{N}", "ℕ"), ("\\mathbb{Z}", "ℤ"), ("\\mathbb{Q}", "ℚ"), ("\\infty", "∞"), ("\\leq", "≤"), ("\\le", "≤"),
        ("\\geq", "≥"), ("\\ge", "≥"), ("\\neq", "≠"), ("\\ne", "≠"), ("\\in", "∈"), ("\\times", "×"), ("\\pi", "π"), ("\\cdot", "·"),
        ("\\left", ""), ("\\right", ""), ("\\,", " "), ("\\;", " "), ("\\{", "{"), ("\\}", "}"), ("\\setminus", "∖"), ("\\cup", "∪"), ("\\cap", "∩"),
    ] {
        s = s.replace(a, b);
    }
    static FRAC: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\frac\{([^{}]*)\}\{([^{}]*)\}").unwrap());
    static RACINE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\\sqrt\{([^{}]*)\}").unwrap());
    s = FRAC.replace_all(&s, "($1)/($2)").to_string();
    s = RACINE.replace_all(&s, "√($1)").to_string();
    s = s.replace("^2", "²").replace("^3", "³").replace("^{2}", "²").replace("^{3}", "³");
    (!s.contains('\\')).then_some(s)
}

/// Évalue l'expression de calcul fournie par le modèle (nombres décimaux, + - * / ^, sqrt).
pub fn evaluer(expr: &str) -> Option<f64> {
    static ENTIER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(\d+(?:\.\d+)?)").unwrap());
    let e = expr.replace(['×', '·'], "*").replace('÷', "/").replace('−', "-").replace("√(", "sqrt(").replace(',', ".");
    if e.chars().any(|c| c.is_alphabetic() && !"sqrt".contains(c)) {
        return None; // une variable ou une fonction inconnue : pas recalculable
    }
    // Tous les nombres en flottants (sinon 7/2 = 3 en division entière).
    let e = ENTIER.replace_all(&e, |c: &regex::Captures| if c[1].contains('.') { c[1].to_string() } else { format!("{}.0", &c[1]) }).to_string();
    let e = e.replace("sqrt(", "math::sqrt(");
    evalexpr::eval_float(&e).ok().filter(|x| x.is_finite())
}

/// Mélange les choix d'un QCM (le modèle met trop souvent la bonne réponse en premier ou en deuxième).
fn melanger(q: &mut QuestionJour) {
    let n = q.choix.len();
    let mut ordre: Vec<usize> = (0..n).collect();
    for i in (1..n).rev() {
        let j = (rand::random::<u32>() as usize) % (i + 1);
        ordre.swap(i, j);
    }
    let choix = ordre.iter().map(|&i| q.choix[i].clone()).collect();
    let erreurs = ordre.iter().map(|&i| q.erreurs.get(i).cloned().flatten()).collect();
    q.bonne = q.bonne.and_then(|b| ordre.iter().position(|&i| i == b));
    q.choix = choix;
    q.erreurs = erreurs;
}

/// Part des mots-clés d'un texte qu'on retrouve dans l'énoncé.
fn part_presente(texte: &str, enonce: &str) -> f64 {
    let cles = mots_cles(texte);
    if cles.len() < 4 {
        return 0.0;
    }
    let e = suivi::slug(enonce).replace('-', " ");
    cles.iter().filter(|m| e.contains(m.as_str())).count() as f64 / cles.len() as f64
}

/// Contrôles sans IA. Err(raison) → la question est réécrite.
/// `connaissances` : la connaissance requise annoncée par le modèle, et l'essentiel de la notion dans la fiche.
fn controler(q: &mut QuestionJour, f: Format, cat: Categorie, connaissances: &[&str]) -> Result<()> {
    for champ in [&mut q.enonce, &mut q.explication] {
        *champ = maths_lisibles(champ).context("formule illisible (LaTeX)")?;
    }
    for c in q.choix.iter_mut().chain(q.reponses_acceptees.iter_mut()).chain(q.attendus.iter_mut()) {
        *c = maths_lisibles(c).context("formule illisible (LaTeX)")?;
    }
    let e = q.enonce.trim();
    if e.chars().count() < 15 || e.chars().count() > 700 {
        bail!("énoncé trop court ou trop long");
    }
    if RECITATION.is_match(e) {
        bail!("question de récitation");
    }
    if cat == Categorie::Maths && HORS_PROGRAMME.is_match(&format!("{} {} {}", e, q.explication, q.choix.join(" "))) {
        bail!("hors programme de seconde");
    }
    // L'énoncé ne doit pas contenir la connaissance (sinon on répond sans l'avoir apprise).
    if INDICE.is_match(e) {
        bail!("l'énoncé donne la connaissance (« sachant que… ») au lieu de la demander");
    }
    if connaissances.iter().any(|k| part_presente(k, e) >= 0.6) {
        bail!("l'énoncé contient la connaissance à tester");
    }
    match f {
        Format::Qcm => {
            let distincts: std::collections::HashSet<String> = q.choix.iter().map(|c| suivi::slug(c)).collect();
            if q.choix.len() != 4 || distincts.len() != 4 || q.bonne.is_none_or(|b| b > 3) {
                bail!("QCM mal formé");
            }
            // Deux choix qui disent la même chose (« consomme du CO₂ » / « absorbe du CO₂ ») : deux bonnes réponses.
            if let Ok(v) = ia::embeddings(&q.choix, false) {
                for i in 0..v.len() {
                    for j in i + 1..v.len() {
                        if ia::cosinus(&v[i], &v[j]) >= 0.95 {
                            bail!("deux choix disent la même chose : « {} » et « {} »", q.choix[i], q.choix[j]);
                        }
                    }
                }
            }
            melanger(q);
        }
        Format::Calcul => {
            let attendu = q.valeur.context("pas de valeur")?;
            let calcul = q.attendus.first().cloned().unwrap_or_default();
            let v = evaluer(&calcul).with_context(|| format!("calcul non recalculable : {calcul}"))?;
            if (v - attendu).abs() > (attendu.abs() * 0.01).max(1e-6) {
                bail!("calcul faux : {calcul} = {v}, pas {attendu}");
            }
            q.valeur = Some((v * 10_000.0).round() / 10_000.0);
            q.tolerance = Some((v.abs() * 0.01).max(0.01));
            q.attendus.clear();
        }
        Format::Courte => {
            if q.reponses_acceptees.iter().all(|r| r.trim().is_empty()) {
                bail!("pas de réponse attendue");
            }
        }
        Format::Pourquoi | Format::Document => {
            if q.attendus.is_empty() {
                bail!("pas de points attendus");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------- vérification par le modèle

const SYSTEME_VERIF: &str = "Tu es un professeur de lycée exigeant. Tu résous la question d'un élève de seconde en t'appuyant sur le cours fourni. Réponds uniquement en JSON.";

/// QCM résolu à l'aveugle (sans connaître la bonne réponse). Vrai si on retrouve la même bonne réponse.
/// Maths, sciences, langues : avec réflexion (calculs et règles, là où les erreurs se cachent, ~25 s).
/// Ailleurs : un raisonnement court écrit AVANT le choix dans la réponse imposée (~5 s).
fn qcm_confirme(contexte: &str, q: &QuestionJour, cat: Categorie) -> Result<bool> {
    let choix = q.choix.iter().enumerate().map(|(i, c)| format!("{i}. {c}")).collect::<Vec<_>>().join("\n");
    let texte = format!(
        "{contexte}\n\nQuestion :\n{}\n{choix}\n\nQuelle est la bonne réponse ? S'il n'y a pas exactement une bonne réponse (aucune, ou plusieurs), mets \"choix\": null.",
        q.enonce
    );
    // « justification » avant « choix » : le modèle raisonne avant de trancher.
    let schema = json!({ "type": "object", "properties": { "justification": { "type": "string" }, "choix": { "type": ["integer", "null"] } }, "required": ["justification", "choix"] });
    let reflexion = matches!(cat, Categorie::Maths | Categorie::Sciences | Categorie::Langue);
    let v = ia::demander_json(SYSTEME_VERIF, &texte, &ia::Options { schema: Some(schema), reflexion, temperature: Some(0.2), ..Default::default() })?;
    Ok(v["choix"].as_u64().map(|x| x as usize) == q.bonne)
}

// ---------------------------------------------------------------- une question

pub struct Cible<'a> {
    pub matiere: &'a str,
    pub lecon: Option<&'a str>,
    pub phase: &'a str,
    pub fiche: &'a Fiche,
    pub notion: &'a Notion,
    pub format: Format,
    pub raisonnement: &'a str,
    /// Document du cours (choisi par le code) pour le format Document.
    pub support: Option<String>,
}

fn contexte(c: &Connection, cible: &Cible) -> Result<String> {
    // La fiche d'abord (identique pour toutes les questions d'une leçon : le serveur la garde en cache).
    let mut s = format!("Matière : {}\n\nFICHE DE LA LEÇON\n{}\n", cible.matiere, cible.fiche.resume());
    let requete = format!("{} {}", cible.notion.intitule, cible.notion.essentiel);
    let extraits = connaissances::chercher(c, &requete, Some(cible.matiere), 3).unwrap_or_default();
    if !extraits.is_empty() {
        s.push_str("\nEXTRAITS DU COURS DE L'ÉLÈVE\n");
        for p in extraits {
            s.push_str(&format!("[{} — {}]\n{}\n\n", p.titre, p.date.unwrap_or_default(), p.texte.chars().take(1200).collect::<String>()));
        }
    }
    Ok(s)
}

fn texte_tache(cible: &Cible, cat: Categorie) -> String {
    let mut t = format!(
        "\nNOTION VISÉE : [{}] {} — {}\nRAISONNEMENT À DEMANDER : {}\n",
        cible.notion.code, cible.notion.intitule, cible.notion.essentiel, cible.raisonnement
    );
    if let Some(d) = &cible.support {
        t.push_str(&format!("\nDOCUMENT (affiché à l'élève)\n\"\"\"\n{d}\n\"\"\"\n"));
    }
    t.push_str(&format!("\n{}", consigne_format(cible.format, cat)));
    t
}

/// Rédige, contrôle et vérifie une question. Réessaie une fois si elle est rejetée.
pub fn question(c: &Connection, cible: &Cible) -> Result<QuestionJour> {
    let cat = categorie(cible.matiere);
    let ctx = contexte(c, cible)?;
    let mut derniere = String::new();
    for essai in 0..2 {
        let mut tache = texte_tache(cible, cat);
        if essai > 0 {
            tache.push_str(&format!("\n\nUne première version a été refusée ({derniere}). Écris une question différente qui respecte toutes les règles."));
        }
        let t0 = std::time::Instant::now();
        let redige = rediger(c, cible, &ctx, &tache);
        let t_redaction = t0.elapsed().as_secs_f64();
        match redige {
            Ok((mut q, connaissance)) => match controler(&mut q, cible.format, cat, &[&connaissance, &cible.notion.essentiel]) {
                Ok(()) if !crate::revision_jour::valide(&q) => derniere = "format incomplet".into(),
                Ok(()) => {
                    if cible.format == Format::Qcm {
                        let t1 = std::time::Instant::now();
                        let ok = qcm_confirme(&ctx, &q, cat)?;
                        eprintln!("    · essai {} : rédaction {t_redaction:.0} s, vérification {:.0} s → {}", essai + 1, t1.elapsed().as_secs_f64(), if ok { "ok" } else { "rejeté" });
                        if !ok {
                            derniere = "la bonne réponse n'est pas sûre".into();
                            continue;
                        }
                    } else {
                        eprintln!("    · essai {} : rédaction {t_redaction:.0} s → ok", essai + 1);
                    }
                    return Ok(q);
                }
                Err(e) => derniere = e.to_string(),
            },
            Err(e) => derniere = format!("{e:#}"),
        }
        eprintln!("    · essai {} : rédaction {t_redaction:.0} s → rejeté ({derniere})", essai + 1);
    }
    bail!("question abandonnée ({derniere})")
}

/// Conversation avec le modèle : il peut chercher dans le cours (2 fois max), puis écrit la question.
fn rediger(c: &Connection, cible: &Cible, contexte: &str, tache: &str) -> Result<(QuestionJour, String)> {
    let sch = schema(cible.format);
    let mut messages = vec![json!({"role": "system", "content": SYSTEME}), json!({"role": "user", "content": format!("{contexte}{tache}")})];
    let mut brut: Option<Value> = None;
    for _ in 0..2 {
        let r = ia::discuter(&messages, &ia::Options { outils: Some(outil_recherche()), ..Default::default() })?;
        if r.appels.is_empty() {
            brut = ia::extraire_json(&r.contenu).ok().filter(|v| sch["required"].as_array().is_some_and(|req| req.iter().all(|k| !v[k.as_str().unwrap_or("")].is_null())));
            break;
        }
        messages.push(r.message.clone());
        for (id, nom, args) in r.appels {
            let resultat = if nom == "chercher_dans_le_cours" {
                let req = args["requete"].as_str().unwrap_or("");
                let ps = connaissances::chercher(c, req, Some(cible.matiere), 3).unwrap_or_default();
                if ps.is_empty() { "Rien trouvé dans le cours.".to_string() } else { ps.iter().map(|p| format!("[{} — {}]\n{}", p.titre, p.date.clone().unwrap_or_default(), p.texte)).collect::<Vec<_>>().join("\n\n") }
            } else {
                "Outil inconnu.".to_string()
            };
            messages.push(json!({"role": "tool", "tool_call_id": id, "content": resultat}));
        }
    }
    // Pas de JSON exploitable sans contrainte : on redemande avec le schéma imposé (toujours valide).
    let v = match brut {
        Some(v) => v,
        None => {
            let r = ia::discuter(&messages, &ia::Options { schema: Some(sch), ..Default::default() })?;
            ia::extraire_json(&r.contenu)?
        }
    };
    let connaissance = v["connaissance_requise"].as_str().unwrap_or("").to_string();
    let chaines = |k: &str| -> Vec<String> { v[k].as_array().into_iter().flatten().filter_map(|x| x.as_str().map(String::from)).collect() };
    let (format, niveau) = match cible.format {
        Format::Qcm => ("qcm", "appliquer"),
        Format::Calcul => ("calcul", "appliquer"),
        Format::Courte => ("reponse_courte", "appliquer"),
        Format::Pourquoi => ("developpement", "comprendre"),
        Format::Document => ("developpement", "comprendre"),
    };
    let q = QuestionJour {
        phase: cible.phase.to_string(),
        matiere: cible.matiere.to_string(),
        lecon: cible.lecon.map(String::from),
        chapitre: cible.fiche.chapitre.clone(),
        notion: cible.notion.code.clone(),
        niveau: niveau.into(),
        format: format.into(),
        enonce: v["enonce"].as_str().unwrap_or("").trim().to_string(),
        support: cible.support.clone(),
        choix: chaines("choix"),
        bonne: v["bonne"].as_u64().map(|x| x as usize),
        reponses_acceptees: chaines("reponses_acceptees"),
        valeur: v["valeur"].as_f64(),
        tolerance: None,
        unite: v["unite"].as_str().filter(|u| !u.trim().is_empty()).map(String::from),
        plan: match cible.format {
            Format::Document => chaines("plan"),
            Format::Pourquoi => vec!["Ta réponse".into()],
            _ => vec![],
        },
        // Pour un calcul, l'expression passe par « attendus » le temps du contrôle.
        attendus: if cible.format == Format::Calcul { vec![v["calcul"].as_str().unwrap_or("").to_string()] } else { chaines("attendus") },
        erreurs: v["erreurs"].as_array().into_iter().flatten().map(|x| x.as_str().map(String::from)).collect(),
        explication: v["explication"].as_str().unwrap_or("").trim().to_string(),
        connaissance: (!connaissance.is_empty()).then(|| connaissance.clone()),
    };
    Ok((q, connaissance))
}

// ---------------------------------------------------------------- séries

/// Un passage de document qui parle de la notion (pour le format Document), coupé proprement.
fn document_pour(c: &Connection, cours: &[SourceCours], notion: &Notion, matiere: &str) -> Option<String> {
    let requete = format!("{} {}", notion.intitule, notion.essentiel);
    let mut candidats = connaissances::chercher(c, &requete, Some(matiere), 8).ok()?;
    // Les documents du prof d'abord, puis le texte du cours s'il est assez long pour être lu comme un document.
    candidats.sort_by_key(|p| if p.type_ == "document" { 0 } else { 1 });
    let ids: Vec<i64> = cours.iter().map(|s| s.id).collect();
    let p = candidats.into_iter().find(|p| p.texte.split_whitespace().count() >= 60 && (p.type_ == "document" || ids.contains(&p.source_id)))?;
    let mut t: String = p.texte.chars().take(1400).collect();
    if t.len() < p.texte.len() {
        if let Some(fin) = t.rfind(['.', '!', '?']) {
            t.truncate(fin + 1);
        }
        t.push_str(" […]");
    }
    Some(format!("{t}\n\n({}, {})", p.titre, p.date.unwrap_or_default()))
}

/// Les questions de compréhension d'une matière du jour, livrées une par une dès qu'elles sont prêtes.
pub fn serie_comprendre(
    c: &Connection,
    matiere: &str,
    debuts: &[(String, String)], // (début du cours, titre de la leçon)
    nb: usize,
    mut livrer: impl FnMut(QuestionJour) -> Result<()>,
) -> Result<usize> {
    let mut cours: Vec<SourceCours> = debuts.iter().filter_map(|(d, _)| connaissances::source_cours(c, d, matiere).ok().flatten()).collect();
    if cours.iter().map(|s| s.texte.len()).sum::<usize>() < 80 {
        // Le prof n'a rien écrit aujourd'hui : on part des derniers cours de la matière.
        cours.extend(connaissances::derniers_cours(c, matiere, 2)?);
    }
    if cours.is_empty() {
        bail!("aucun contenu de cours pour {matiere}");
    }
    let t = std::time::Instant::now();
    let fiches: Vec<(Fiche, String)> = cours.iter().filter_map(|s| connaissances::fiche(c, s).ok().map(|f| (f, s.titre.clone()))).collect();
    eprintln!("  fiches de leçon : {} en {:.0} s", fiches.len(), t.elapsed().as_secs_f64());
    let notions: Vec<(&Fiche, &Notion, &str)> = fiches.iter().flat_map(|(f, t)| f.notions.iter().map(move |n| (f, n, t.as_str()))).collect();
    if notions.is_empty() {
        bail!("pas de notions trouvées dans le cours de {matiere}");
    }
    let cat = categorie(matiere);
    let a_document = cours.iter().any(|s| s.texte.split_whitespace().count() >= 60) || cours.iter().any(|s| connaissances::documents_du_cours(c, &s.cle).is_ok_and(|d| !d.is_empty()));
    let mut faites = 0;
    for (i, format) in plan(cat, a_document).into_iter().cycle().take(nb).enumerate() {
        let (fiche, notion, titre) = notions[i % notions.len()];
        let support = if format == Format::Document { document_pour(c, &cours, notion, matiere) } else { None };
        let format = if format == Format::Document && support.is_none() { Format::Pourquoi } else { format };
        let lecon = debuts.iter().find(|(_, t)| t == titre).map(|(_, t)| t.as_str()).or(Some(titre));
        let cible = Cible { matiere, lecon, phase: "comprendre", fiche, notion, format, raisonnement: RAISONNEMENTS[i % RAISONNEMENTS.len()], support };
        match question(c, &cible) {
            Ok(q) => {
                livrer(q)?;
                faites += 1;
            }
            Err(e) => eprintln!("{matiere} : {e:#}"),
        }
    }
    if faites == 0 {
        bail!("aucune question n'a passé les vérifications pour {matiere}");
    }
    Ok(faites)
}

/// Une série de révision sur les notions prioritaires (à revoir, en difficulté, évals proches).
pub fn serie_reviser(c: &Connection, nb: usize, deja: &[String], mut livrer: impl FnMut(QuestionJour) -> Result<()>) -> Result<usize> {
    let prio = suivi::priorites(c, nb * 2)?;
    let mut faites = 0;
    for (i, p) in prio.priorites.iter().enumerate() {
        if faites >= nb {
            break;
        }
        // La fiche du chapitre si on l'a ; sinon une fiche minimale tirée du suivi et des passages du cours.
        let fiche = connaissances::fiches_du_chapitre(c, &p.matiere, &p.chapitre)?.into_iter().next().unwrap_or_else(|| Fiche {
            chapitre: p.chapitre.clone(),
            titre_chapitre: p.chapitre.clone(),
            notions: vec![Notion { code: p.notion.clone(), intitule: p.intitule.clone(), essentiel: p.intitule.clone() }],
            ..Default::default()
        });
        let notion = fiche.notions.iter().find(|n| n.code == p.notion).cloned().unwrap_or(Notion { code: p.notion.clone(), intitule: p.intitule.clone(), essentiel: p.intitule.clone() });
        let cat = categorie(&p.matiere);
        let format = match (cat, i % 3) {
            (Categorie::Maths, 0) => Format::Calcul,
            (Categorie::Maths, _) => Format::Qcm,
            (Categorie::Langue, 1) => Format::Courte,
            (_, 2) => Format::Pourquoi,
            _ => Format::Qcm,
        };
        let raisonnement = RAISONNEMENTS[(i + 1) % RAISONNEMENTS.len()];
        let cible = Cible { matiere: &p.matiere, lecon: None, phase: "reviser", fiche: &fiche, notion: &notion, format, raisonnement, support: None };
        match question(c, &cible) {
            Ok(q) if !deja.contains(&q.enonce) => {
                livrer(q)?;
                faites += 1;
            }
            Ok(_) => {}
            Err(e) => eprintln!("révision {} : {e:#}", p.matiere),
        }
    }
    if faites == 0 {
        bail!("pas de question de révision possible pour l'instant");
    }
    Ok(faites)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn categories() {
        assert_eq!(categorie("MATHEMATIQUES"), Categorie::Maths);
        assert_eq!(categorie("SCIENCES VIE & TERRE"), Categorie::Sciences);
        assert_eq!(categorie("PHYSIQUE-CHIMIE"), Categorie::Sciences);
        assert_eq!(categorie("ITALIEN LV2"), Categorie::Langue);
        assert_eq!(categorie("FRANCAIS"), Categorie::Francais);
        assert_eq!(categorie("HISTOIRE-GEOGRAPHIE"), Categorie::Humanites);
        assert_eq!(categorie("SC. ECONO.& SOCIALES"), Categorie::Humanites);
    }

    #[test]
    fn calculs_recalcules() {
        assert_eq!(evaluer("(100 - 80) / 80 * 100"), Some(25.0));
        assert_eq!(evaluer("7/2"), Some(3.5));
        assert_eq!(evaluer("2^3 + sqrt(16)"), Some(12.0));
        assert_eq!(evaluer("3,5 × 2"), Some(7.0));
        assert_eq!(evaluer("x + 2"), None);
    }

    #[test]
    fn latex_nettoye() {
        assert_eq!(maths_lisibles("f définie sur $\\mathbb{R}$ par $x^2 - 4$").unwrap(), "f définie sur ℝ par x² - 4");
        assert_eq!(maths_lisibles("$\\frac{x+3}{2}$ ≤ 1").unwrap(), "(x+3)/(2) ≤ 1");
        assert!(maths_lisibles("\\begin{cases} x \\end{cases}").is_none());
    }

    fn q(format: &str) -> QuestionJour {
        QuestionJour {
            phase: "comprendre".into(), matiere: "SVT".into(), lecon: None, chapitre: "c".into(), notion: "n".into(), niveau: "appliquer".into(),
            format: format.into(), enonce: "Une algue reste deux heures dans le noir : comment évolue l'oxygène de l'eau ?".into(), support: None,
            choix: vec![], bonne: None, reponses_acceptees: vec![], valeur: None, tolerance: None, unite: None, plan: vec![], attendus: vec!["respiration".into()],
            erreurs: vec![], explication: "À l'obscurité seule la respiration a lieu.".into(), connaissance: None,
        }
    }

    #[test]
    fn controles() {
        let mut r = q("developpement");
        r.enonce = "Qu'est-ce que la photosynthèse ?".into();
        assert!(controler(&mut r, Format::Pourquoi, Categorie::Sciences, &[]).is_err(), "récitation refusée");

        let mut fuite = q("developpement");
        fuite.enonce = "Sachant qu'à l'obscurité les cellules chlorophylliennes réalisent seulement la respiration cellulaire, que se passe-t-il ?".into();
        assert!(controler(&mut fuite, Format::Pourquoi, Categorie::Sciences, &["À l'obscurité, les cellules chlorophylliennes réalisent seulement la respiration cellulaire"]).is_err(), "connaissance donnée dans l'énoncé");

        let mut indice = q("developpement");
        indice.enonce = "Si tu sais que les racines n'ont pas de chloroplastes, explique l'évolution des gaz dans le bioréacteur.".into();
        assert!(controler(&mut indice, Format::Pourquoi, Categorie::Sciences, &[]).is_err(), "« si tu sais que » refusé");

        let mut bon = q("developpement");
        assert!(controler(&mut bon, Format::Pourquoi, Categorie::Sciences, &["À l'obscurité, les cellules chlorophylliennes réalisent seulement la respiration cellulaire"]).is_ok());

        let mut derivee = q("qcm");
        derivee.enonce = "Quelle est la fonction dérivée de f(x) = x² − 4x + 3 ?".into();
        derivee.choix = vec!["2x − 4".into(), "x − 4".into(), "2x + 4".into(), "x² + 3".into()];
        derivee.bonne = Some(0);
        assert!(controler(&mut derivee, Format::Qcm, Categorie::Maths, &[]).is_err(), "hors programme");

        let mut calc = q("calcul");
        calc.enonce = "Un prix passe de 80 € à 100 €. De quel pourcentage a-t-il augmenté ?".into();
        calc.valeur = Some(25.0);
        calc.attendus = vec!["(100-80)/80*100".into()];
        assert!(controler(&mut calc, Format::Calcul, Categorie::Maths, &[]).is_ok());
        let mut faux = q("calcul");
        faux.enonce = calc.enonce.clone();
        faux.valeur = Some(20.0);
        faux.attendus = vec!["(100-80)/80*100".into()];
        assert!(controler(&mut faux, Format::Calcul, Categorie::Maths, &[]).is_err(), "calcul faux refusé");
    }

    #[test]
    fn melange_garde_la_bonne() {
        let mut m = q("qcm");
        m.choix = vec!["a".into(), "b".into(), "c".into(), "d".into()];
        m.erreurs = vec![None, Some("eb".into()), Some("ec".into()), Some("ed".into())];
        m.bonne = Some(0);
        for _ in 0..20 {
            melanger(&mut m);
            let b = m.bonne.unwrap();
            assert_eq!(m.choix[b], "a");
            assert!(m.erreurs[b].is_none());
        }
    }
}
