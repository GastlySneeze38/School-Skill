"""Banc d'essai des modèles locaux pour écrire des questions de révision.

Usage : python banc.py <nom-du-modele>   (voir MODELES)
Lit les cours réels dans donnees/pronote, écrit les résultats dans donnees/banc/ (données personnelles : jamais
dans git). Le serveur llama.cpp est lancé et arrêté par le script.

Ce banc imite la chaîne prévue : le code choisit le cours, le passage et le type de question ; le modèle n'écrit
qu'UNE question, avec une sortie imposée par un schéma JSON.
"""

import json
import os
import re
import subprocess
import sys
import threading
import time
import unicodedata
import urllib.request
from pathlib import Path

IA = Path.home() / ".revisions" / "ia"
RACINE = Path(__file__).resolve().parents[2]
DONNEES = RACINE / "donnees"
SORTIE = DONNEES / "banc"
PORT = 8091

MODELES = {
    "qwen9b": {
        "fichier": "Qwen3.5-9B-UD-Q4_K_XL.gguf", "ngl": 99,
        "echantillonnage": {"temperature": 0.7, "top_p": 0.8, "top_k": 20, "presence_penalty": 1.5},
    },
    "gemma12b": {
        "fichier": "gemma-4-12b-it-qat-q4_0.gguf", "ngl": 48,
        "echantillonnage": {"temperature": 1.0, "top_p": 0.95, "top_k": 64},
    },
    # v2 : consignes corrigées + réflexion (thinking) + 2e passe de vérification, 1 nouvel essai si rejetée.
    "qwen9b-v2": {
        "fichier": "Qwen3.5-9B-UD-Q4_K_XL.gguf", "ngl": 99, "reflexion": True, "verification": True,
        # Réglages recommandés par Qwen en mode réflexion.
        "echantillonnage": {"temperature": 0.6, "top_p": 0.95, "top_k": 20, "presence_penalty": 1.0},
    },
    "qwen4b": {
        "fichier": "Qwen3.5-4B-UD-Q4_K_XL.gguf", "ngl": 99,
        "echantillonnage": {"temperature": 0.7, "top_p": 0.8, "top_k": 20, "presence_penalty": 1.5},
    },
}

# ---------------------------------------------------------------- consignes (identiques à chaque appel : en cache)

SYSTEME = """Tu écris UNE question de révision pour un élève de seconde générale, en français (ou dans la langue
étudiée pour une matière de langue), à partir de son cours réel.

Règles :
- La question fait RÉFLÉCHIR : expliquer un pourquoi, interpréter un document, appliquer à un cas nouveau, comparer.
  Interdit : « Donne la définition de… », « Qu'est-ce que… », « Qui a… », « En quelle année… », « Cite… ».
- Elle colle à CE cours : ses exemples, ses textes, ses exercices. Pas une question générique de manuel.
- Elle se répond en 1 à 5 minutes, sans autre ressource que le cours et le document fourni.
- Français correct, phrases courtes, vocabulaire du lycée.
- "explication" : la correction en 1 à 3 phrases, qui fait comprendre (pas seulement la réponse).
Réponds uniquement avec l'objet JSON demandé."""

# Ajouts de la v2 (défauts vus au premier banc d'essai).
SYSTEME_V2 = SYSTEME.replace("Réponds uniquement avec l'objet JSON demandé.", """- Tutoie l'élève.
- Langue : tout est en français, SAUF en langue vivante (anglais, italien…) où l'énoncé, les choix et les
  réponses attendues sont dans la langue étudiée ; l'explication reste en français.
- Maths : écris en texte simple avec les symboles Unicode (ℝ, ∞, x², √, ≤, ≠, ∈). Jamais de LaTeX ni de $.
- JUSTESSE AVANT TOUT : tout ce que la question affirme ou suppose doit être vrai, et la réponse attendue doit être
  exacte. Fais le calcul ou applique la règle toi-même avant d'écrire. En cas de doute sur une règle, choisis une
  autre question dont tu es sûr.
Réponds uniquement avec l'objet JSON demandé.""")

TYPES = {
    "document": {
        "consigne": "Type : INTERPRÉTER LE DOCUMENT ci-dessus. La question oblige à exploiter le document (relever, "
                    "expliquer, mettre en relation avec le cours). Réponse rédigée en quelques phrases.",
        "schema": {
            "type": "object",
            "properties": {
                "enonce": {"type": "string"},
                "plan": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 3},
                "attendus": {"type": "array", "items": {"type": "string"}, "minItems": 3, "maxItems": 5},
                "explication": {"type": "string"},
            },
            "required": ["enonce", "plan", "attendus", "explication"],
        },
    },
    "pourquoi": {
        "consigne": "Type : POURQUOI / COMMENT. La question demande d'expliquer une cause, un mécanisme ou un "
                    "enjeu du cours. Réponse en 2 à 4 phrases.",
        "schema": {
            "type": "object",
            "properties": {
                "enonce": {"type": "string"},
                "attendus": {"type": "array", "items": {"type": "string"}, "minItems": 2, "maxItems": 4},
                "explication": {"type": "string"},
            },
            "required": ["enonce", "attendus", "explication"],
        },
    },
    "appliquer": {
        "consigne": "Type : APPLIQUER À UN CAS NOUVEAU. Invente une situation ou une phrase nouvelle (pas celle du "
                    "cours) où l'élève doit utiliser ce qu'il a appris. Réponse courte (un mot, un nombre, une phrase).",
        "schema": {
            "type": "object",
            "properties": {
                "enonce": {"type": "string"},
                "reponses_acceptees": {"type": "array", "items": {"type": "string"}, "minItems": 1, "maxItems": 5},
                "explication": {"type": "string"},
            },
            "required": ["enonce", "reponses_acceptees", "explication"],
        },
    },
    "qcm": {
        "consigne": "Type : QCM DE RÉFLEXION à 4 choix. Les 3 mauvais choix correspondent à de vraies erreurs "
                    "d'élève (confusion, contresens, étape oubliée), plausibles et de même longueur que le bon.",
        "schema": {
            "type": "object",
            "properties": {
                "enonce": {"type": "string"},
                "choix": {"type": "array", "items": {"type": "string"}, "minItems": 4, "maxItems": 4},
                "bonne": {"type": "integer", "minimum": 0, "maximum": 3},
                "erreurs": {"type": "array", "items": {"type": ["string", "null"]}, "minItems": 4, "maxItems": 4},
                "explication": {"type": "string"},
            },
            "required": ["enonce", "choix", "bonne", "erreurs", "explication"],
        },
    },
}

# ---------------------------------------------------------------- les cas : cours et documents réels


def texte_pdf(chemin: Path) -> str:
    try:
        import pypdf  # outil de dev, pas une dépendance de l'app
    except ImportError:
        return ""
    lecteur = pypdf.PdfReader(str(chemin))
    return " ".join(" ".join((p.extract_text() or "").split()) for p in lecteur.pages)


def cas_reels() -> list[dict]:
    sync = json.loads((DONNEES / "pronote" / "sync.json").read_text(encoding="utf-8"))
    vus, cas = set(), []
    cours = sorted(
        (c for c in sync["cours"] if c.get("contenu") and len(c["contenu"].get("description") or "") >= 100),
        key=lambda c: -len(c["contenu"]["description"]),
    )
    for c in cours:
        desc = c["contenu"]["description"].strip()
        cle = desc[:80]
        if cle in vus:
            continue
        vus.add(cle)
        cas.append({"matiere": c["matiere"], "lecon": c["contenu"].get("titre") or "Cours", "date": c["debut"][:10],
                    "cours": desc[:1500], "document": None})
    cas = cas[:17]

    fichiers = DONNEES / "pronote" / "fichiers"
    # Les noms venus de Pronote peuvent avoir des accents décomposés (NFD) : on cherche par motif.
    docs = [
        ("SCIENCES VIE & TERRE", "La photosynthèse", "SCIENCES VIE & TERRE", "2026-11-06_*photosynth*.pdf"),
        ("ITALIEN LV2", "Les possessifs", "ITALIEN LV2", "devoir_*possessifs*.pdf"),
    ]
    for matiere, lecon, dossier, motif in docs:
        trouves = sorted((fichiers / dossier).glob(motif))
        t = texte_pdf(trouves[0]) if trouves else ""
        f = trouves[0] if trouves else Path(motif)
        if t:
            cas.append({"matiere": matiere, "lecon": lecon, "date": f.name[:10], "cours": "", "document": t[:2500]})
    ocr = SORTIE / "ocr_transition.txt"  # produit par GLM-OCR (voir docs/ia-locale.md)
    if ocr.exists():
        cas.append({"matiere": "HISTOIRE-GEOGRAPHIE", "lecon": "La transition démographique", "date": "2026-11-23",
                    "cours": "", "document": ocr.read_text(encoding="utf-8")[:2500]})
    return cas


def message(cas: dict, type_q: str) -> str:
    morceaux = [f"Matière : {cas['matiere']}", f"Leçon : {cas['lecon']} (cours du {cas['date']})"]
    # Sans document du prof, c'est le texte du cahier de textes qui sert de document.
    document = cas["document"] or (cas["cours"] if type_q == "document" else None)
    if cas["cours"] and document != cas["cours"]:
        morceaux.append(f"Ce qui a été fait en cours (cahier de textes) :\n{cas['cours']}")
    if document:
        morceaux.append(f"Document du cours :\n\"\"\"\n{document}\n\"\"\"")
    morceaux.append(TYPES[type_q]["consigne"])
    return "\n\n".join(morceaux)


# ---------------------------------------------------------------- serveur et mesure de la VRAM


class Vram(threading.Thread):
    def __init__(self):
        super().__init__(daemon=True)
        self.max, self.stop = 0, False

    def run(self):
        while not self.stop:
            try:
                v = subprocess.run(["nvidia-smi", "--query-gpu=memory.used", "--format=csv,noheader,nounits"],
                                   capture_output=True, text=True, timeout=5).stdout.strip()
                self.max = max(self.max, int(v.splitlines()[0]))
            except Exception:
                pass
            time.sleep(0.5)


def lancer(modele: dict) -> subprocess.Popen:
    cmd = [str(IA / "bin" / "llama-server.exe"), "-m", str(IA / "modeles" / modele["fichier"]),
           "-ngl", str(modele["ngl"]), "-c", str(modele.get("contexte", 4096)), "--flash-attn", "on", "-ctk", "q8_0", "-ctv", "q8_0",
           "--host", "127.0.0.1", "--port", str(PORT), "--parallel", "1", "--jinja"]
    if not modele.get("reflexion"):
        cmd += ["--reasoning", "off"]
    else:
        # Sans plafond, Qwen réfléchit 3 000 jetons et plus (75 s) ; à 800, ~22 s et une réponse juste.
        cmd += ["--reasoning-budget", "800", "--reasoning-budget-message", " Je dois maintenant conclure et écrire le JSON."]
    journal = open(SORTIE / "serveur.log", "w", encoding="utf-8")
    p = subprocess.Popen(cmd, stdout=journal, stderr=subprocess.STDOUT)
    for _ in range(180):
        try:
            if json.load(urllib.request.urlopen(f"http://127.0.0.1:{PORT}/health", timeout=2)).get("status") == "ok":
                return p
        except Exception:
            time.sleep(1)
    p.kill()
    raise SystemExit("le serveur n'a pas démarré (voir donnees/banc/serveur.log)")


def demander(modele: dict, texte: str, schema: dict, systeme: str = SYSTEME) -> dict:
    reflexion = bool(modele.get("reflexion"))
    corps = {
        "messages": [{"role": "system", "content": systeme}, {"role": "user", "content": texte}],
        "chat_template_kwargs": {"enable_thinking": reflexion},
        "cache_prompt": True,
        **modele["echantillonnage"],
    }
    if reflexion:
        # La grammaire JSON gênerait la réflexion : on décrit la forme attendue et on extrait le JSON après coup.
        corps["messages"][1]["content"] += ("\n\nRéfléchis, puis réponds uniquement avec un objet JSON qui respecte ce "
                                            f"schéma :\n{json.dumps(schema, ensure_ascii=False)}")
        corps["max_tokens"] = 2000
    else:
        corps["max_tokens"] = 900
        corps["response_format"] = {"type": "json_schema", "json_schema": {"name": "question", "schema": schema}}
    req = urllib.request.Request(f"http://127.0.0.1:{PORT}/v1/chat/completions", json.dumps(corps).encode(),
                                 {"Content-Type": "application/json"})
    t = time.time()
    r = json.load(urllib.request.urlopen(req, timeout=600))
    contenu = r["choices"][0]["message"]["content"] or ""
    if reflexion:
        debut, fin = contenu.find("{"), contenu.rfind("}")
        contenu = contenu[debut:fin + 1] if debut >= 0 else contenu
    return {"texte": contenu, "duree": time.time() - t,
            "jetons_sortie": r["usage"]["completion_tokens"], "jetons_entree": r["usage"]["prompt_tokens"],
            "timings": r.get("timings", {})}


def contexte(cas: dict) -> str:
    morceaux = [f"Matière : {cas['matiere']}", f"Leçon : {cas['lecon']}"]
    if cas["cours"]:
        morceaux.append(f"Cours (cahier de textes) :\n{cas['cours']}")
    if cas["document"]:
        morceaux.append(f"Document du cours :\n\"\"\"\n{cas['document']}\n\"\"\"")
    return "\n\n".join(morceaux)


VERIF_SYSTEME = "Tu es un professeur de lycée exigeant qui relit des questions de révision. Réponds uniquement en JSON."


def verifier(modele: dict, cas: dict, type_q: str, q: dict) -> dict:
    """2e passe. QCM : on le résout sans connaître la bonne réponse. Autres : relecture critique."""
    if type_q == "qcm":
        choix = "\n".join(f"{i}. {c}" for i, c in enumerate(q.get("choix", [])))
        texte = (f"{contexte(cas)}\n\nRésous ce QCM posé à un élève de seconde :\n{q.get('enonce')}\n{choix}\n\n"
                 "S'il n'y a pas exactement une bonne réponse, mets \"choix\": null.")
        schema = {"type": "object", "properties": {"choix": {"type": ["integer", "null"]}, "justification": {"type": "string"}},
                  "required": ["choix", "justification"]}
        r = demander(modele, texte, schema, VERIF_SYSTEME)
        try:
            v = json.loads(r["texte"])
            ok = v.get("choix") == q.get("bonne")
            return {"ok": ok, "detail": v.get("justification", ""), "choix_verif": v.get("choix"), "duree": round(r["duree"], 1)}
        except json.JSONDecodeError:
            return {"ok": False, "detail": "vérification illisible", "duree": round(r["duree"], 1)}
    attendu = q.get("reponses_acceptees") or q.get("attendus")
    texte = (f"{contexte(cas)}\n\nQuestion écrite pour un élève de seconde :\n{q.get('enonce')}\n\n"
             f"Réponse attendue : {json.dumps(attendu, ensure_ascii=False)}\nExplication : {q.get('explication')}\n\n"
             "Vérifie : (1) tout ce que la question affirme ou suppose est vrai ; (2) la réponse attendue est exacte "
             "(refais le calcul, applique la règle) ; (3) on peut y répondre avec ce cours et ce document. "
             "\"valide\" est false au moindre problème.")
    schema = {"type": "object", "properties": {"valide": {"type": "boolean"}, "probleme": {"type": ["string", "null"]}},
              "required": ["valide", "probleme"]}
    r = demander(modele, texte, schema, VERIF_SYSTEME)
    try:
        v = json.loads(r["texte"])
        return {"ok": bool(v.get("valide")), "detail": v.get("probleme") or "", "duree": round(r["duree"], 1)}
    except json.JSONDecodeError:
        return {"ok": False, "detail": "vérification illisible", "duree": round(r["duree"], 1)}


# ---------------------------------------------------------------- contrôles automatiques

BATEAU = re.compile(r"^\s*(donne la d[ée]finition|qu'est-ce qu|quelle est la d[ée]finition|qui a |qui est |en quelle ann[ée]e|cite )", re.I)


def controles(type_q: str, q: dict, cas: dict) -> dict:
    enonce = q.get("enonce", "")
    c = {"bateau": bool(BATEAU.search(enonce)), "longueur_enonce": len(enonce)}
    if type_q == "qcm":
        choix = [unicodedata.normalize("NFC", x.strip().lower()) for x in q.get("choix", [])]
        c["choix_distincts"] = len(set(choix)) == 4
        longueurs = [len(x) for x in choix] or [0]
        c["bonne_plus_longue"] = len(choix) == 4 and longueurs[q.get("bonne", 0)] == max(longueurs) and max(longueurs) > 1.3 * sorted(longueurs)[1]
    if type_q == "document":
        c["cite_le_document"] = bool(re.search(r"document|texte|extrait|passage|selon|d'apr[eè]s|phase|activit", enonce, re.I))
    return c


def main():
    nom = sys.argv[1]
    modele = MODELES[nom]
    SORTIE.mkdir(parents=True, exist_ok=True)
    cas = cas_reels()
    print(f"{len(cas)} cas × {len(TYPES)} types avec {nom}", flush=True)
    vram = Vram()
    vram.start()
    t0 = time.time()
    serveur = lancer(modele)
    chargement = time.time() - t0
    resultats = []
    # Chaque question est aussi écrite tout de suite, pour pouvoir les lire pendant que le banc tourne.
    direct = open(SORTIE / f"{nom}.jsonl", "w", encoding="utf-8")
    try:
        # BANC_CAS=0,7,11 : seulement ces cas (pour un modèle lent).
        choisis = {int(x) for x in os.environ.get("BANC_CAS", "").split(",") if x.strip()}
        for i, k in enumerate(cas):
            if choisis and i not in choisis:
                continue
            for type_q, spec in TYPES.items():
                systeme = SYSTEME_V2 if modele.get("verification") else SYSTEME
                essais, duree, jetons = [], 0.0, 0
                for _ in range(2 if modele.get("verification") else 1):
                    r = demander(modele, message(k, type_q), spec["schema"], systeme)
                    duree += r["duree"]
                    jetons += r["jetons_sortie"]
                    try:
                        q = json.loads(r["texte"])
                        ok = True
                    except json.JSONDecodeError:
                        q, ok = {}, False
                    verif = None
                    if ok and modele.get("verification"):
                        verif = verifier(modele, k, type_q, q)
                        duree += verif["duree"]
                    essais.append({"question": q, "verification": verif})
                    if not modele.get("verification") or (verif and verif["ok"]):
                        break
                gardee = not modele.get("verification") or bool(essais[-1]["verification"] and essais[-1]["verification"]["ok"])
                resultats.append({"cas": i, "matiere": k["matiere"], "lecon": k["lecon"], "type": type_q,
                                  "json_valide": ok, "question": q, "controles": controles(type_q, q, k) if ok else {},
                                  "essais": essais, "gardee": gardee,
                                  "duree": round(duree, 2), "jetons_entree": r["jetons_entree"],
                                  "jetons_sortie": jetons,
                                  "ecriture_tps": round(r["timings"].get("predicted_per_second", 0), 1)})
                direct.write(json.dumps(resultats[-1], ensure_ascii=False) + "\n")
                direct.flush()
                print(f"  cas {i:2} {type_q:9} {r['duree']:5.1f}s {r['jetons_sortie']:4} jetons  {q.get('enonce', '')[:70]}", flush=True)
    finally:
        serveur.kill()
        vram.stop = True
    resume = {
        "modele": nom, "fichier": modele["fichier"], "chargement_s": round(chargement, 1), "vram_max_mo": vram.max,
        "questions": len(resultats),
        "json_valides": sum(r["json_valide"] for r in resultats),
        "bateau": sum(r["controles"].get("bateau", False) for r in resultats),
        "duree_moyenne_s": round(sum(r["duree"] for r in resultats) / len(resultats), 2),
        "ecriture_tps_moyenne": round(sum(r["ecriture_tps"] for r in resultats) / len(resultats), 1),
        "jetons_sortie_moyens": round(sum(r["jetons_sortie"] for r in resultats) / len(resultats)),
        "qcm_choix_distincts": sum(r["controles"].get("choix_distincts", False) for r in resultats if r["type"] == "qcm"),
        "qcm_bonne_plus_longue": sum(r["controles"].get("bonne_plus_longue", False) for r in resultats if r["type"] == "qcm"),
        "document_cite": sum(r["controles"].get("cite_le_document", False) for r in resultats if r["type"] == "document"),
        "gardees": sum(r["gardee"] for r in resultats),
        "rejetees_au_1er_essai": sum(len(r["essais"]) > 1 for r in resultats),
    }
    (SORTIE / f"{nom}.json").write_text(json.dumps({"resume": resume, "cas": cas, "resultats": resultats},
                                                   ensure_ascii=False, indent=1), encoding="utf-8")
    print(json.dumps(resume, ensure_ascii=False, indent=1))


if __name__ == "__main__":
    main()
