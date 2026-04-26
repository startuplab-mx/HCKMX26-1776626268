import json
import os
import re
import time
from pathlib import Path

import torch
from dotenv import load_dotenv
from transformers import pipeline
from unidecode import unidecode

LEXICAL_SHORTCUT_SCORE = 0.95
LEXICAL_BOOST_FLOOR = 0.70
MAX_CONTEXT = 4
CONTEXT_SEP = " ⋯ "


def _find_env() -> Path:
    here = Path(__file__).resolve().parent
    for d in [here, *here.parents]:
        candidate = d / ".env"
        if candidate.exists():
            return candidate
    raise FileNotFoundError(
        f"No se encontró .env subiendo desde {here}. Copia .env.example a .env y rellénalo."
    )


def load_config() -> dict:
    env_path = _find_env()
    load_dotenv(env_path, override=True)

    def _require(var: str) -> str:
        v = os.environ.get(var)
        if v is None:
            raise KeyError(f"Falta variable de entorno: {var}")
        return v

    keys = json.loads(_require("CATEGORY_KEYS"))
    return {
        "model_id": _require("NLI_MODEL"),
        "category_keys": keys,
        "hypotheses": {k: json.loads(_require(f"HYPOTHESES_{k.upper()}")) for k in keys},
        "lexical": {k: json.loads(_require(f"LEXICAL_{k.upper()}")) for k in keys},
        "thresholds": json.loads(_require("THRESHOLDS")),
        "test_cases": json.loads(_require("TEST_CASES")),
        "neutral_hypothesis": _require("NEUTRAL_HYPOTHESIS"),
        "context_test_cases": json.loads(os.environ.get("CONTEXT_TEST_CASES", "[]")),
    }


def normalizar(texto: str) -> str:
    return unidecode(texto.lower())


def contar_matches(texto: str, patrones: dict) -> int:
    texto_norm = normalizar(texto)
    n = 0
    for frase in patrones.get("frases", []) or []:
        if normalizar(frase) in texto_norm:
            n += 1
    for emoji in patrones.get("emojis", []) or []:
        if emoji in texto:
            n += 1
    for tag in patrones.get("hashtags", []) or []:
        if normalizar(tag) in texto_norm:
            n += 1
    rgx = patrones.get("regex") or ""
    if rgx and re.search(rgx, texto_norm, flags=re.IGNORECASE):
        n += 1
    return n


def make_clasificar(clf, cfg: dict):
    all_hypotheses = [h for cat in cfg["category_keys"] for h in cfg["hypotheses"][cat]]
    neutral = cfg["neutral_hypothesis"]
    candidates = all_hypotheses + [neutral]

    def clasificar(texto: str, contexto: list[str] | None = None) -> dict:
        if contexto:
            recientes = contexto[-MAX_CONTEXT:]
            texto_eval = CONTEXT_SEP.join(recientes + [texto])
        else:
            texto_eval = texto

        matches = {cat: contar_matches(texto_eval, cfg["lexical"][cat])
                   for cat in cfg["category_keys"]}
        cat_atajo, n_atajo = max(matches.items(), key=lambda kv: kv[1])
        if n_atajo >= 2:
            return {cat: (LEXICAL_SHORTCUT_SCORE if cat == cat_atajo else 0.0)
                    for cat in cfg["category_keys"]}

        out = clf(
            texto_eval,
            candidate_labels=candidates,
            multi_label=True,
            hypothesis_template="{}",
        )
        score_por_hyp = dict(zip(out["labels"], out["scores"]))
        score_neutral = score_por_hyp[neutral]

        resultado = {}
        for cat in cfg["category_keys"]:
            score_cat = max(score_por_hyp[h] for h in cfg["hypotheses"][cat])
            score_cat = max(0.0, score_cat - score_neutral)
            if matches[cat] == 1:
                score_cat = max(score_cat, LEXICAL_BOOST_FLOOR)
            resultado[cat] = float(score_cat)
        return resultado

    return clasificar


def decidir(scores: dict, thresholds: dict) -> tuple[list[str], str]:
    """Multi-label: cualquier categoría que cruce su umbral entra en la lista.
    La acción es la severidad más alta entre las categorías disparadas."""
    bloqueadas, avisadas = [], []
    for cat, score in scores.items():
        th = thresholds.get(cat, 0.70)
        if score >= th + 0.10:
            bloqueadas.append(cat)
        elif score >= th:
            avisadas.append(cat)
    if bloqueadas:
        return bloqueadas + avisadas, "BLOQUEAR"
    if avisadas:
        return avisadas, "AVISAR"
    return [], "PERMITIR"


def main() -> None:
    cfg = load_config()
    print(f"Categorías: {cfg['category_keys']}")
    print(f"Hipótesis totales: {sum(len(v) for v in cfg['hypotheses'].values())}")
    print(f"Test cases: {len(cfg['test_cases'])}\n")

    device = 0 if torch.cuda.is_available() else -1
    print(f"Cargando modelo en device={device} (CPU=-1) ...")
    clf = pipeline(
        "zero-shot-classification",
        model=cfg["model_id"],
        device=device,
    )
    clf("warmup", candidate_labels=["a", "b"], multi_label=True, hypothesis_template="{}")
    print("Modelo listo.\n")

    clasificar = make_clasificar(clf, cfg)

    rows = []
    latencias = []
    aciertos = 0
    total = 0

    for case in cfg["test_cases"]:
        texto = case["text"]
        esperada = case.get("expected")
        t0 = time.perf_counter()
        scores = clasificar(texto)
        dt_ms = (time.perf_counter() - t0) * 1000
        latencias.append(dt_ms)
        cats, accion = decidir(scores, cfg["thresholds"])
        ok = (accion == "PERMITIR") if esperada is None else (esperada in cats)
        total += 1
        if ok:
            aciertos += 1
        rows.append({
            "texto": texto[:60] + ("…" if len(texto) > 60 else ""),
            "scores": {c: round(s, 3) for c, s in scores.items()},
            "predichas": cats,
            "esperada": esperada,
            "accion": accion,
            "ms": round(dt_ms, 1),
            "ok": ok,
        })

    for r in rows:
        pred = "[" + ",".join(r["predichas"]) + "]" if r["predichas"] else "—"
        esp = str(r["esperada"])
        marca = "✓" if r["ok"] else "✗"
        print(f"{marca} [{r['accion']:9s}] pred={pred:<17} esp={esp:<5}  "
              f"{r['ms']:>7.1f}ms  {r['scores']}")
        print(f"   texto: {r['texto']}")

    if latencias:
        media = sum(latencias) / len(latencias)
        print(f"\nLatencia media: {media:.1f} ms  (min={min(latencias):.1f}, max={max(latencias):.1f})")
    print(f"Aciertos (solo): {aciertos}/{total}")

    if not cfg["context_test_cases"]:
        return

    print(f"\n=== Buffer de contexto (últimos {MAX_CONTEXT} mensajes) ===\n")
    ctx_aciertos_solo = 0
    ctx_aciertos_ctx = 0
    ctx_total = 0

    for case in cfg["context_test_cases"]:
        msgs = case["messages"]
        esperada = case.get("expected")
        ultimo = msgs[-1]
        previos = msgs[:-1]

        scores_solo = clasificar(ultimo)
        cats_solo, accion_solo = decidir(scores_solo, cfg["thresholds"])
        ok_solo = (accion_solo == "PERMITIR") if esperada is None else (esperada in cats_solo)

        scores_ctx = clasificar(ultimo, contexto=previos)
        cats_ctx, accion_ctx = decidir(scores_ctx, cfg["thresholds"])
        ok_ctx = (accion_ctx == "PERMITIR") if esperada is None else (esperada in cats_ctx)

        ctx_total += 1
        if ok_solo:
            ctx_aciertos_solo += 1
        if ok_ctx:
            ctx_aciertos_ctx += 1

        print(f"esperada={esperada}")
        print(f"  contexto: {' | '.join(previos)}")
        print(f"  último:   {ultimo}")
        marca_solo = "✓" if ok_solo else "✗"
        marca_ctx = "✓" if ok_ctx else "✗"
        pred_solo = "[" + ",".join(cats_solo) + "]" if cats_solo else "—"
        pred_ctx = "[" + ",".join(cats_ctx) + "]" if cats_ctx else "—"
        print(f"  {marca_solo} solo:    {accion_solo:9s} pred={pred_solo:<17} {scores_solo}")
        print(f"  {marca_ctx} con ctx: {accion_ctx:9s} pred={pred_ctx:<17} {scores_ctx}\n")

    print(f"Aciertos sin contexto: {ctx_aciertos_solo}/{ctx_total}")
    print(f"Aciertos con contexto: {ctx_aciertos_ctx}/{ctx_total}")


if __name__ == "__main__":
    main()
