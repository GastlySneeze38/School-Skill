//! Lancer des programmes externes (claude) sans fenêtre de console qui clignote.

use std::path::Path;
use std::process::{Command, Output};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Variables à ne jamais transmettre à `claude`. Quand l'app est lancée depuis l'app Claude
/// (son panneau Terminal, par exemple), elle hérite de variables internes : `CLAUDE_CODE_SIMPLE=1`
/// fait ignorer la connexion au forfait et `ANTHROPIC_BASE_URL` détourne les requêtes,
/// d'où des « identifiants invalides ». On repart d'un environnement propre.
fn nettoyer_env(c: &mut Command) {
    for (k, _) in std::env::vars_os() {
        let k = k.to_string_lossy().to_string();
        if k.starts_with("ANTHROPIC_") || k.starts_with("CLAUDE") {
            c.env_remove(&k);
        }
    }
}

/// Prépare un appel à `claude`. Sous Windows c'est un script npm (`claude.cmd`) que
/// `Command::new("claude")` ne trouve pas : on passe par `cmd /C`. Les textes longs (prompts)
/// passent par l'entrée standard, jamais en argument (guillemets et caractères spéciaux de cmd).
pub fn commande_claude(args: &[&str]) -> Command {
    #[cfg(windows)]
    let mut c = {
        let mut c = Command::new("cmd");
        c.arg("/C").arg("claude").args(args);
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(windows))]
    let mut c = {
        let mut c = Command::new("claude");
        c.args(args);
        c
    };
    nettoyer_env(&mut c);
    c
}

/// Ouvre une vraie fenêtre de terminal (visible) qui lance une commande `claude`,
/// pour les étapes où l'élève doit agir lui-même (connexion au compte).
pub fn terminal_claude(titre: &str, args: &str) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        let mut c = Command::new("cmd");
        // `start "titre" cmd /K …` : nouvelle fenêtre qui reste ouverte à la fin.
        c.args(["/C", "start", titre, "cmd", "/K", &format!("claude {args}")]);
        nettoyer_env(&mut c);
        c.spawn().map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = titre;
        let mut c = Command::new("x-terminal-emulator");
        c.args(["-e", &format!("claude {args}")]);
        nettoyer_env(&mut c);
        c.spawn().map(|_| ())
    }
}

pub struct Resultat {
    pub ok: bool,
    pub sortie: String,
    pub erreurs: String,
}

impl From<Output> for Resultat {
    fn from(o: Output) -> Self {
        Resultat {
            ok: o.status.success(),
            sortie: String::from_utf8_lossy(&o.stdout).trim().to_string(),
            erreurs: String::from_utf8_lossy(&o.stderr).trim().to_string(),
        }
    }
}

pub fn lancer(mut c: Command, dossier: Option<&Path>) -> Resultat {
    if let Some(d) = dossier {
        c.current_dir(d);
    }
    match c.output() {
        Ok(o) => o.into(),
        Err(e) => Resultat { ok: false, sortie: String::new(), erreurs: e.to_string() },
    }
}
