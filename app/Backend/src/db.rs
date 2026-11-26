//! Base SQLite locale : tout ce que l'app et le skill savent des révisions.
//!
//! Aucun mot de passe ici : celui de Pronote reste dans le coffre Windows, et Claude
//! passe par Claude Code, déjà connecté au forfait.

use std::path::{Path, PathBuf};

use rusqlite::Connection;

/// Une entrée par version du schéma ; on n'en modifie jamais une déjà publiée,
/// on en ajoute une nouvelle (le numéro est `PRAGMA user_version`).
const MIGRATIONS: &[&str] = &[
    // 1 — schéma initial
    r#"
    CREATE TABLE matieres (
        id          TEXT PRIMARY KEY,          -- slug : 'sciences-vie-terre'
        nom         TEXT NOT NULL              -- tel que dans Pronote
    );

    CREATE TABLE chapitres (
        id          INTEGER PRIMARY KEY,
        matiere_id  TEXT NOT NULL REFERENCES matieres(id) ON DELETE CASCADE,
        code        TEXT NOT NULL,             -- 'genetique'
        titre       TEXT NOT NULL,
        sources     TEXT NOT NULL DEFAULT '[]', -- JSON : d'où vient le contenu
        UNIQUE (matiere_id, code)
    );

    CREATE TABLE notions (
        id                  INTEGER PRIMARY KEY,
        chapitre_id         INTEGER NOT NULL REFERENCES chapitres(id) ON DELETE CASCADE,
        code                TEXT NOT NULL,
        intitule            TEXT NOT NULL,
        palier              INTEGER NOT NULL DEFAULT -1,  -- répétition espacée
        prochaine_revision  TEXT,                         -- AAAA-MM-JJ
        UNIQUE (chapitre_id, code)
    );

    -- Chaque réponse à une question : c'est de là qu'on calcule 🟢🟠🔴.
    CREATE TABLE essais (
        id          INTEGER PRIMARY KEY,
        notion_id   INTEGER NOT NULL REFERENCES notions(id) ON DELETE CASCADE,
        date        TEXT NOT NULL,
        niveau      TEXT NOT NULL CHECK (niveau IN ('savoir', 'comprendre', 'appliquer')),
        ok          INTEGER NOT NULL CHECK (ok IN (0, 1)),
        erreur      TEXT,                      -- 'confusion : mutation vs allèle'
        question    TEXT,                      -- JSON du QCM posé (énoncé, choix, bonne réponse)
        reponse     TEXT
    );
    CREATE INDEX essais_notion ON essais(notion_id, date);

    CREATE TABLE evals (
        id          INTEGER PRIMARY KEY,
        matiere_id  TEXT NOT NULL REFERENCES matieres(id) ON DELETE CASCADE,
        date        TEXT NOT NULL,
        type        TEXT NOT NULL DEFAULT 'contrôle',
        titre       TEXT NOT NULL DEFAULT '',
        poids       INTEGER NOT NULL DEFAULT 1,
        chapitres   TEXT NOT NULL DEFAULT '[]', -- JSON : codes de chapitres
        UNIQUE (matiere_id, date)
    );

    CREATE TABLE flashcards (
        id          INTEGER PRIMARY KEY,
        notion_id   INTEGER NOT NULL REFERENCES notions(id) ON DELETE CASCADE,
        recto       TEXT NOT NULL,
        verso       TEXT NOT NULL
    );

    -- Le « papier » de l'élève : rédactions et dessins faits dans l'app.
    CREATE TABLE brouillons (
        id          INTEGER PRIMARY KEY,
        matiere_id  TEXT REFERENCES matieres(id) ON DELETE SET NULL,
        essai_id    INTEGER REFERENCES essais(id) ON DELETE SET NULL,
        type        TEXT NOT NULL CHECK (type IN ('texte', 'dessin')),
        contenu     TEXT NOT NULL,             -- texte, ou image en data-URL
        cree_le     TEXT NOT NULL,
        modifie_le  TEXT NOT NULL
    );
    "#,
    // 2 — progression des moyennes et conseils de Claude
    r#"
    -- Une photo des moyennes à chaque synchro (une par jour) : c'est ce qui permet de
    -- montrer la progression au fil du trimestre, pas seulement d'un trimestre à l'autre.
    CREATE TABLE historique_moyennes (
        jour        TEXT NOT NULL,             -- AAAA-MM-JJ
        periode     TEXT NOT NULL,             -- 'Trimestre 1'
        matiere     TEXT NOT NULL,             -- nom Pronote, ou '__generale__'
        eleve       REAL,
        classe      REAL,
        PRIMARY KEY (jour, periode, matiere)
    );

    -- Analyses des moyennes par Claude (où gagner des points), la plus récente sert.
    CREATE TABLE analyses_moyennes (
        id          INTEGER PRIMARY KEY,
        cree_le     TEXT NOT NULL,
        contenu     TEXT NOT NULL              -- JSON
    );
    "#,
    // 3 — séance du jour (écran principal)
    r#"
    -- Une séance par jour : leçons du jour, questions (tous formats), préparées à l'avance.
    CREATE TABLE seances_jour (
        jour        TEXT PRIMARY KEY,          -- AAAA-MM-JJ
        lecons      TEXT NOT NULL DEFAULT '[]', -- JSON : leçons dont on teste la compréhension
        questions   TEXT NOT NULL DEFAULT '[]', -- JSON : questions dans l'ordre où elles sont posées
        erreur      TEXT,                      -- dernière erreur de préparation
        maj         TEXT NOT NULL
    );

    -- Réponses de la séance du jour (pour reprendre où on en était si l'app se ferme).
    CREATE TABLE reponses_jour (
        jour        TEXT NOT NULL,
        idx         INTEGER NOT NULL,          -- position de la question
        reponse     TEXT NOT NULL,             -- JSON : ce que l'élève a répondu
        statut      TEXT NOT NULL CHECK (statut IN ('juste', 'partiel', 'faux', 'en_correction')),
        score       REAL NOT NULL DEFAULT 0,   -- 0 à 1
        retour      TEXT,                      -- correction / commentaire
        cree_le     TEXT NOT NULL,
        PRIMARY KEY (jour, idx)
    );
    "#,
    // 4 — base de connaissances pour l'IA locale : sources, passages indexés, fiches de leçon
    r#"
    -- Tout ce qu'on sait des cours : cahier de textes, devoirs, documents des profs (texte extrait ou OCR).
    CREATE TABLE sources (
        id          INTEGER PRIMARY KEY,
        cle         TEXT NOT NULL UNIQUE,      -- 'cours:2026-11-24T08:00:00:italien-lv2', 'doc:<chemin>'
        parent      TEXT,                      -- document : clé du cours auquel il est attaché
        matiere     TEXT NOT NULL,             -- nom Pronote
        type        TEXT NOT NULL,             -- cours | devoir | document
        titre       TEXT NOT NULL,
        date        TEXT,                      -- AAAA-MM-JJ
        chemin      TEXT,
        texte       TEXT NOT NULL,
        empreinte   TEXT NOT NULL,             -- pour ne refaire le travail que si la source change
        ajoute_le   TEXT NOT NULL
    );
    CREATE INDEX sources_matiere ON sources(matiere, date);

    -- Les sources découpées en passages de 100 à 300 mots : ce que l'IA lit et cite.
    CREATE TABLE passages (
        id          INTEGER PRIMARY KEY,
        source_id   INTEGER NOT NULL REFERENCES sources(id) ON DELETE CASCADE,
        matiere     TEXT NOT NULL,
        ordre       INTEGER NOT NULL,
        texte       TEXT NOT NULL,
        embedding   BLOB                       -- vecteur de sens (f32), NULL tant qu'il n'est pas calculé
    );
    CREATE INDEX passages_source ON passages(source_id);
    CREATE INDEX passages_matiere ON passages(matiere);

    -- Recherche plein texte (sans accents) : rapide et sans IA.
    CREATE VIRTUAL TABLE passages_fts USING fts5(
        texte, content='passages', content_rowid='id', tokenize='unicode61 remove_diacritics 2'
    );
    CREATE TRIGGER passages_ai AFTER INSERT ON passages BEGIN
        INSERT INTO passages_fts(rowid, texte) VALUES (new.id, new.texte);
    END;
    CREATE TRIGGER passages_ad AFTER DELETE ON passages BEGIN
        INSERT INTO passages_fts(passages_fts, rowid, texte) VALUES ('delete', old.id, old.texte);
    END;

    -- Une fiche de connaissances par cours, écrite une fois par l'IA (JSON) : c'est elle qu'on donne au modèle,
    -- pas le cours brut.
    CREATE TABLE fiches (
        source_id   INTEGER PRIMARY KEY REFERENCES sources(id) ON DELETE CASCADE,
        matiere     TEXT NOT NULL,
        chapitre    TEXT NOT NULL,
        contenu     TEXT NOT NULL,
        cree_le     TEXT NOT NULL
    );
    CREATE INDEX fiches_chapitre ON fiches(matiere, chapitre);
    "#,
];

pub fn chemin_base(dossier_donnees: &Path) -> PathBuf {
    dossier_donnees.join("revisions.db")
}

pub fn ouvrir(chemin: &Path) -> rusqlite::Result<Connection> {
    if let Some(parent) = chemin.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let mut conn = Connection::open(chemin)?;
    // WAL : le skill (Python) et l'app peuvent lire/écrire en même temps.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    migrer(&mut conn)?;
    Ok(conn)
}

/// Base vide en mémoire (tests).
#[cfg(test)]
pub fn ouvrir_memoire() -> rusqlite::Result<Connection> {
    let mut conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    migrer(&mut conn)?;
    Ok(conn)
}

fn migrer(conn: &mut Connection) -> rusqlite::Result<()> {
    let version: usize = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", i + 1)?;
        tx.commit()?;
    }
    Ok(())
}

pub fn version_schema(conn: &Connection) -> rusqlite::Result<usize> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
}

pub fn tables(conn: &Connection) -> rusqlite::Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name",
    )?;
    let noms = stmt.query_map([], |r| r.get(0))?.collect();
    noms
}
