//! Ligne de commande, utilisée par le skill `revisions` (à la place des anciens scripts Python) :
//! même base, mêmes règles que l'app.
//!
//!   revisions sync [--forcer] [--sans-fichiers]
//!   revisions etat
//!   revisions contenu
//!   revisions chapitre  --matiere M --chapitre C [--titre T] [--source S] --notion "code:intitulé" …
//!   revisions resultat  --matiere M --chapitre C --notion N --niveau savoir|comprendre|appliquer --ok 1|0 [--erreur E]
//!   revisions resultats --fichier resultats.json
//!   revisions eval      --matiere M --date AAAA-MM-JJ [--type T] [--titre T] [--chapitres a,b] [--poids 1-3] [--supprimer]
//!   revisions carte     [--matiere M] [--chapitre C]
//!   revisions priorites [--n 8] [--json]
//!   revisions moyennes  [--analyser]   moyennes, progression et conseils de l'IA
//!   revisions jour      [--preparer] [--matiere M]   séance du jour (préparée par l'IA locale)
//!   revisions connaissances           met à jour la base de connaissances (cours, documents, OCR, vecteurs)
//!   revisions chercher  --requete R [--matiere M]   recherche dans les cours (celle qu'utilise l'IA)
//!   revisions essai     --matiere M [--n 6]   écrit des questions sur les derniers cours, sans les enregistrer

use std::collections::HashMap;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::{config, connaissances, db, generation, moyennes, pronote, revision_jour, suivi};

struct Args {
    valeurs: HashMap<String, Vec<String>>,
    drapeaux: Vec<String>,
}

impl Args {
    fn lire(brut: &[String]) -> Args {
        let (mut valeurs, mut drapeaux): (HashMap<String, Vec<String>>, Vec<String>) = (HashMap::new(), vec![]);
        let mut i = 0;
        while i < brut.len() {
            let a = &brut[i];
            if let Some(nom) = a.strip_prefix("--") {
                match brut.get(i + 1).filter(|v| !v.starts_with("--")) {
                    Some(v) => {
                        valeurs.entry(nom.to_string()).or_default().push(v.clone());
                        i += 1;
                    }
                    None => drapeaux.push(nom.to_string()),
                }
            }
            i += 1;
        }
        Args { valeurs, drapeaux }
    }
    fn opt(&self, n: &str) -> Option<&str> {
        self.valeurs.get(n).and_then(|v| v.last()).map(|s| s.as_str())
    }
    fn req(&self, n: &str) -> Result<&str> {
        self.opt(n).with_context(|| format!("--{n} manquant"))
    }
    fn tous(&self, n: &str) -> Vec<String> {
        self.valeurs.get(n).cloned().unwrap_or_default()
    }
    fn drapeau(&self, n: &str) -> bool {
        self.drapeaux.iter().any(|d| d == n)
    }
}

/// Exécute une commande et renvoie le code de sortie du programme.
pub fn executer(brut: &[String]) -> i32 {
    match executer_interne(brut) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("❌ {e:#}");
            1
        }
    }
}

fn base() -> Result<rusqlite::Connection> {
    let dossier = config::dossier_donnees();
    let conn = db::ouvrir(&db::chemin_base(&dossier))?;
    suivi::importer_json_si_vide(&conn, &dossier)?;
    Ok(conn)
}

fn executer_interne(brut: &[String]) -> Result<i32> {
    let Some(commande) = brut.first() else { bail!("commande manquante (sync, chapitre, resultat, eval, carte, priorites, contenu, etat)") };
    let a = Args::lire(&brut[1..]);
    match commande.as_str() {
        "sync" => {
            let rt = tokio::runtime::Runtime::new()?;
            let r = rt.block_on(pronote::synchroniser(a.drapeau("forcer"), !a.drapeau("sans-fichiers")));
            if r.ok {
                if !r.ignoree {
                    if let Some(sync) = std::fs::read_to_string(config::dossier_donnees().join("pronote").join("sync.json")).ok().and_then(|t| serde_json::from_str(&t).ok()) {
                        moyennes::enregistrer_instantane(&base()?, &sync)?;
                    }
                }
                println!("{} {}", if r.ignoree { "⏸" } else { "✅" }, r.message);
                if let Ok(t) = std::fs::read_to_string(config::dossier_donnees().join("pronote").join("sync.json")) {
                    if !r.ignoree {
                        let v: Value = serde_json::from_str(&t)?;
                        for c in v["controles_a_venir"].as_array().into_iter().flatten().take(8) {
                            println!("   📝 {}  {}  ({})", c["date"].as_str().unwrap_or(""), c["matiere"].as_str().unwrap_or(""), c["source"].as_str().unwrap_or(""));
                        }
                    }
                }
                if !r.alertes.is_empty() {
                    println!("   ⚠ {} élément(s) non récupéré(s) : {}", r.alertes.len(), r.alertes.join(" ; "));
                }
                Ok(0)
            } else {
                eprintln!("❌ {}", r.message);
                // 5 : IP bloquée → surtout ne pas réessayer tout de suite.
                Ok(if r.message.contains("bloqué temporairement") { 5 } else { 3 })
            }
        }
        "etat" => {
            let c = config::lire_config();
            println!("Données : {}", config::dossier_donnees().display());
            match &c {
                Some(c) => println!(
                    "Pronote : {} ({}) — mot de passe {}",
                    c.pronote_url.as_deref().unwrap_or("?"),
                    c.ent.as_deref().unwrap_or("atrium_educonnect"),
                    if config::mot_de_passe_present(c) { "dans le coffre Windows" } else { "ABSENT : à renseigner dans l'app (Paramètres)" }
                ),
                None => println!("Pronote : pas configuré (ouvre l'app, Paramètres)"),
            }
            Ok(0)
        }
        "contenu" => {
            let conn = base()?;
            println!("Matières par quantité de contenu de cours disponible :");
            for s in suivi::contenu(&conn, &config::dossier_donnees())? {
                println!(
                    "  {:<26} {:>2} cours avec contenu · {} doc(s) · {} car. · dernier cours {}{}",
                    s.matiere, s.cours, s.docs, s.caracteres, s.dernier_cours, if s.suivie { "  (déjà suivie)" } else { "" }
                );
            }
            Ok(0)
        }
        "chapitre" => {
            let conn = base()?;
            println!("{}", suivi::chapitre(&conn, a.req("matiere")?, a.req("chapitre")?, a.opt("titre"), &a.tous("notion"), a.opt("source"))?);
            Ok(0)
        }
        "resultat" => {
            let conn = base()?;
            let ok = matches!(a.req("ok")?, "1" | "true" | "oui" | "o");
            println!(
                "{}",
                suivi::resultat(
                    &conn,
                    &suivi::NouveauResultat {
                        matiere: a.req("matiere")?,
                        chapitre: a.req("chapitre")?,
                        notion: a.req("notion")?,
                        niveau: a.req("niveau")?,
                        ok,
                        erreur: a.opt("erreur"),
                        question: None,
                        reponse: None,
                    }
                )?
            );
            Ok(0)
        }
        "resultats" => {
            let conn = base()?;
            let liste: Vec<Value> = serde_json::from_str(&std::fs::read_to_string(a.req("fichier")?)?)?;
            for r in liste {
                let s = |k: &str| r[k].as_str().map(String::from).unwrap_or_default();
                let ok = matches!(&r["ok"], Value::Bool(true)) || r["ok"] == 1 || matches!(r["ok"].as_str(), Some("1" | "true" | "oui"));
                println!(
                    "{}",
                    suivi::resultat(
                        &conn,
                        &suivi::NouveauResultat {
                            matiere: &s("matiere"),
                            chapitre: &s("chapitre"),
                            notion: &s("notion"),
                            niveau: &s("niveau"),
                            ok,
                            erreur: r["erreur"].as_str(),
                            question: None,
                            reponse: None,
                        }
                    )?
                );
            }
            Ok(0)
        }
        "eval" => {
            let conn = base()?;
            println!(
                "{}",
                suivi::eval(
                    &conn,
                    &suivi::NouvelleEval {
                        matiere: a.req("matiere")?,
                        date: a.req("date")?,
                        type_: a.opt("type"),
                        titre: a.opt("titre"),
                        chapitres: a.opt("chapitres").map(|c| c.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect()),
                        poids: a.opt("poids").map(str::parse).transpose()?,
                        supprimer: a.drapeau("supprimer"),
                    }
                )?
            );
            Ok(0)
        }
        "carte" => {
            let conn = base()?;
            let lignes = suivi::carte(&conn, a.opt("matiere"), a.opt("chapitre"))?;
            if lignes.is_empty() {
                println!("Aucune notion suivie pour l'instant.");
            }
            let mut titre = String::new();
            for l in lignes {
                let t = format!("{} – {}  [{}]", l.matiere, l.titre_chapitre, l.chapitre);
                if t != titre {
                    println!("\n{t}");
                    titre = t;
                }
                let faibles = if l.niveaux_faibles.is_empty() { String::new() } else { format!("  ✘ {}", l.niveaux_faibles.join(", ")) };
                let err = l.derniere_erreur.map(|e| format!("  · dernière erreur : {e}")).unwrap_or_default();
                println!("  {} {} [{}]{faibles}{err}", l.etat.emoji(), l.intitule, l.notion);
            }
            Ok(0)
        }
        "priorites" => {
            let conn = base()?;
            let n = a.opt("n").map(str::parse).transpose()?.unwrap_or(8);
            let p = suivi::priorites(&conn, n)?;
            if a.drapeau("json") {
                println!("{}", serde_json::to_string_pretty(&p)?);
                return Ok(0);
            }
            let auj = chrono::Local::now().date_naive();
            if !p.evals_a_venir.is_empty() {
                println!("Évaluations à venir :");
                for e in &p.evals_a_venir {
                    let d = chrono::NaiveDate::parse_from_str(e["date"].as_str().unwrap_or_default(), "%Y-%m-%d")?;
                    println!("  📝 {} (J-{})  {}  {} {}", d, (d - auj).num_days(), e["matiere"].as_str().unwrap_or(""), e["type"].as_str().unwrap_or(""), e["titre"].as_str().unwrap_or(""));
                }
            }
            if !p.evals_sans_contenu.is_empty() {
                println!("\nÀ préparer (on ne connaît pas encore les notions de ces évals) :");
                for e in &p.evals_sans_contenu {
                    let ch: Vec<&str> = e["chapitres"].as_array().into_iter().flatten().filter_map(|c| c.as_str()).collect();
                    println!("  ❓ {} le {} — chapitres : {}", e["matiere"].as_str().unwrap_or(""), e["date"].as_str().unwrap_or(""), if ch.is_empty() { "inconnus".into() } else { ch.join(", ") });
                }
            }
            println!("{}", if p.priorites.is_empty() { "\nRien d'urgent côté notions." } else { "\nPriorités :" });
            for (i, l) in p.priorites.iter().enumerate() {
                println!("  {}. {} – {} [{}/{}]  ({})  score {}", i + 1, l.matiere, l.intitule, l.chapitre, l.notion, l.raisons.join(", "), l.score);
            }
            Ok(0)
        }
        "moyennes" => {
            let conn = base()?;
            let Some(m) = moyennes::lire(&conn)? else {
                println!("Pas encore de moyennes : lance d'abord `revisions sync`.");
                return Ok(0);
            };
            let f = |x: Option<f64>| x.map_or("—".to_string(), |v| format!("{v:.2}").replace('.', ","));
            let evo = |x: Option<f64>| x.map_or(String::new(), |v| format!("  ({}{:.2})", if v >= 0.0 { "+" } else { "" }, v).replace('.', ","));
            println!(
                "Moyenne générale ({}) : {}{}  · classe {}{}",
                m.generale.periode_actuelle.as_deref().unwrap_or("?"),
                f(m.generale.actuelle),
                evo(m.generale.evolution),
                f(m.generale.classe),
                if m.generale_calculee { "  (calculée)" } else { "" }
            );
            println!("Au-dessus de la classe dans {} matière(s) sur {}.\n", m.au_dessus_classe, m.matieres.len());
            for l in &m.matieres {
                println!("  {:<26} {:>6}{}   classe {}", l.matiere, f(l.actuelle), evo(l.evolution), f(l.classe));
            }
            let analyse = if a.drapeau("analyser") { Some(moyennes::analyser(&conn)?) } else { moyennes::derniere_analyse(&conn)? };
            match analyse {
                Some(an) => {
                    println!("\nConseils de Claude (du {}) : {}", an.cree_le.replace('T', " à "), an.message);
                    for c in an.matieres.iter().filter(|c| c.priorite <= 2) {
                        println!(
                            "  {} {} : +{:.1} pt possible{} — {} → {}",
                            if c.priorite == 1 { "🔺" } else { "•" },
                            c.matiere,
                            c.potentiel,
                            c.objectif.map_or(String::new(), |o| format!(", objectif {o:.1}")),
                            c.pourquoi,
                            c.a_reviser
                        );
                    }
                }
                None => println!("\nPas encore d'analyse : `revisions moyennes --analyser` demande à l'IA locale où gagner des points."),
            }
            Ok(0)
        }
        "jour" => {
            let conn = base()?;
            if a.drapeau("preparer") {
                revision_jour::preparer(&conn, a.opt("matiere"))?;
            }
            let e = revision_jour::etat(&conn)?;
            println!("Séance du {} — leçons du {}", e.jour, e.date_lecons.as_deref().unwrap_or("?"));
            for m in &e.matieres {
                let titres: Vec<&str> = m.matiere.lecons.iter().map(|l| l.titre.as_str()).collect();
                println!(
                    "  📖 {} {} : {}{} — {} question(s), {} faite(s){}",
                    m.matiere.heure,
                    m.matiere.matiere,
                    titres.join(", "),
                    if m.matiere.a_contenu { "" } else { " (sans contenu)" },
                    m.nb_questions,
                    m.nb_repondues,
                    m.comprehension.map(|p| format!(", compréhension {p} %")).unwrap_or_default()
                );
            }
            if let Some(err) = &e.erreur {
                println!("  ⚠ {err}");
            }
            for (i, q) in e.questions.iter().enumerate() {
                let fait = e.reponses.iter().find(|r| r.idx == i).map(|r| format!(" [{}]", r.statut)).unwrap_or_default();
                println!("  {:>2}. ({} · {} · {}) {} — {}{fait}", i + 1, q.phase, q.format, q.niveau, q.matiere, q.enonce);
            }
            Ok(0)
        }
        "connaissances" => {
            let conn = base()?;
            let r = connaissances::mettre_a_jour(&conn)?;
            println!("📚 {} sources ({} nouvelles ou modifiées), {} passages, {} vecteurs calculés", r.sources, r.nouvelles, r.passages, r.vecteurs);
            if !r.illisibles.is_empty() {
                println!("   ⚠ non lus (format pas encore pris en charge) : {}", r.illisibles.join(", "));
            }
            Ok(0)
        }
        "chercher" => {
            let conn = base()?;
            for p in connaissances::chercher(&conn, a.req("requete")?, a.opt("matiere"), a.opt("n").and_then(|n| n.parse().ok()).unwrap_or(5))? {
                let extrait = p.texte.chars().take(300).collect::<String>().replace('\n', " ");
                println!("— {} · {} · {} ({})\n  {extrait}\n", p.matiere, p.type_, p.titre, p.date.unwrap_or_default());
            }
            Ok(0)
        }
        "essai" => {
            let conn = base()?;
            let matiere = a.req("matiere")?;
            let n = a.opt("n").and_then(|n| n.parse().ok()).unwrap_or(6);
            connaissances::mettre_a_jour(&conn)?;
            let debut = std::time::Instant::now();
            let mut k = 0;
            generation::serie_comprendre(&conn, matiere, &[], n, |q| {
                k += 1;
                println!("\n{k}. [{} · {}] ({:.0} s) {}", q.format, q.notion, debut.elapsed().as_secs_f64(), q.enonce);
                if let Some(s) = &q.support {
                    println!("   📄 {}", s.chars().take(160).collect::<String>().replace('\n', " "));
                }
                for (i, c) in q.choix.iter().enumerate() {
                    println!("   {} {}", if Some(i) == q.bonne { "✔" } else { "·" }, c);
                }
                if !q.reponses_acceptees.is_empty() {
                    println!("   → {}", q.reponses_acceptees.join(" | "));
                }
                if let Some(v) = q.valeur {
                    println!("   → {v} {}", q.unite.clone().unwrap_or_default());
                }
                for at in &q.attendus {
                    println!("   • {at}");
                }
                println!("   🧠 {}", q.connaissance.clone().unwrap_or_default());
                println!("   💬 {}", q.explication);
                Ok(())
            })?;
            println!("\n{k} question(s) en {:.0} s", debut.elapsed().as_secs_f64());
            Ok(0)
        }
        autre => bail!("commande inconnue : {autre}"),
    }
}
