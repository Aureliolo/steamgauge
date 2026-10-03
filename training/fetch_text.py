"""Puts the review text back beside the published labels, on your own machine.

The dataset carries no review text: the words are their authors', and they stay on Steam.
This fetches each labelled review from Steam's public review endpoint, checks it against the
fingerprint of the text the labels were written against, and writes every claim with its text
cut out at the byte offsets its label names.

    python fetch_text.py --data <dataset folder> --to claims-with-text.jsonl

A review deleted since it was labelled cannot be fetched, and one edited since no longer
matches its fingerprint: the labels of both are left out and counted, never cut out of a
text they were not written against. Standard library only.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import time
import urllib.error
import urllib.parse
import urllib.request
from collections import defaultdict
from collections.abc import Callable, Iterable
from pathlib import Path

ENDPOINT = "https://store.steampowered.com/appreviews/{app}"
DAY = 86_400

# The census parameters the labels' text was fetched with. Steam's defaults leave out
# activated keys and review bombs, and with them reviews that were labelled.
PARAMETERS = {
    "json": "1",
    "language": "all",
    "purchase_type": "all",
    "filter_offtopic_activity": "0",
    "filter": "recent",
    "num_per_page": "100",
    "date_range_type": "include",
}

Page = Callable[[int, dict], dict]


# Steam refuses a caller for minutes at a time, and a refusal waited out for half a minute
# fails inside a window that lifts on its own a few minutes later: so twice as long each time
# from two seconds, never more than five minutes at once, for up to half an hour, and as long
# as Steam's own Retry-After says whenever it says.
BACKOFF_BASE, BACKOFF_CAP, PATIENCE = 2.0, 300.0, 1800.0


def next_wait(refusals: int, waited: float, told: float | None) -> float | None:
    """How long to wait after the `refusals`-th refusal in a row, or None to give up."""
    wait = told if told is not None else min(BACKOFF_BASE * 2 ** (refusals - 1), BACKOFF_CAP)
    return wait if waited + wait <= PATIENCE else None


def steam_page(app: int, query: dict) -> dict:
    url = ENDPOINT.format(app=app) + "?" + urllib.parse.urlencode(query)
    request = urllib.request.Request(url, headers={"User-Agent": "game-review-claims fetch"})
    refusals, waited = 0, 0.0
    while True:
        told = None
        try:
            with urllib.request.urlopen(request, timeout=60) as response:
                answer = json.load(response)
            # A page a second keeps a fetch of every labelled review well inside what Steam
            # serves one caller.
            time.sleep(1)
            return answer
        except urllib.error.HTTPError as refused:
            if refused.code != 429 and refused.code < 500:
                raise
            after = refused.headers.get("Retry-After", "")
            told = float(after) if after.isdigit() else None
        except OSError:
            pass
        refusals += 1
        wait = next_wait(refusals, waited, told)
        if wait is None:
            raise SystemExit(f"Steam refused app {app} for half an hour; run again later")
        print(f"  Steam is refusing requests; waiting {wait:.0f} s")
        time.sleep(wait)
        waited += wait


def windows(created: Iterable[int]) -> list[tuple[int, int]]:
    """Date windows covering every review's creation time, a day either side, overlaps merged."""
    merged: list[list[int]] = []
    for at in sorted(created):
        start, end = at - DAY, at + DAY
        if merged and start <= merged[-1][1]:
            merged[-1][1] = max(merged[-1][1], end)
        else:
            merged.append([start, end])
    return [(start, end) for start, end in merged]


def fetch(app: int, wanted: dict[str, int], page: Page = steam_page) -> dict[str, str]:
    """The text of every wanted review Steam still holds, by review id."""
    found: dict[str, str] = {}
    for start, end in windows(wanted.values()):
        cursor = "*"
        while True:
            answer = page(
                app, {**PARAMETERS, "start_date": start, "end_date": end, "cursor": cursor}
            )
            for review in answer.get("reviews", []):
                if review.get("recommendationid") in wanted:
                    found[review["recommendationid"]] = review.get("review", "")
            following = answer.get("cursor")
            if not answer.get("reviews") or not following or following == cursor:
                break
            cursor = following
            if len(found) == len(wanted):
                break
        if len(found) == len(wanted):
            break
    return found


def rows(path: Path) -> list[dict]:
    with path.open(encoding="utf-8") as lines:
        return [json.loads(line) for line in lines if line.strip()]


def joined(claims: list[dict], facts: list[dict], texts: dict[str, str]) -> tuple[list[dict], dict]:
    """Every claim whose review came back unchanged, with its text; and what was left out."""
    by_review = {row["review_id"]: row for row in facts}
    out, left = [], {"deleted": 0, "edited": 0, "no facts": 0}
    for claim in claims:
        review = by_review.get(claim["review_id"])
        if review is None:
            left["no facts"] += 1
            continue
        text = texts.get(claim["review_id"])
        if text is None:
            left["deleted"] += 1
            continue
        raw = text.encode("utf-8")
        if hashlib.sha256(raw).hexdigest() != review["text_sha256"]:
            left["edited"] += 1
            continue
        out.append({**claim, "text": raw[claim["start"] : claim["end"]].decode("utf-8")})
    return out, left


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--data", default=".", help="the dataset's folder")
    parser.add_argument("--to", default="claims-with-text.jsonl")
    parser.add_argument("--claims", default="claims.jsonl", help="which label file to join")
    args = parser.parse_args()

    data = Path(args.data)
    facts = rows(data / "reviews.jsonl")
    claims = rows(data / args.claims)
    wanted: dict[int, dict[str, int]] = defaultdict(dict)
    labelled = {claim["review_id"] for claim in claims}
    for review in facts:
        if review["review_id"] in labelled:
            wanted[review["app_id"]][review["review_id"]] = review["created"]

    texts: dict[str, str] = {}
    for number, (app, reviews) in enumerate(sorted(wanted.items()), 1):
        texts.update(fetch(app, reviews))
        print(f"{number}/{len(wanted)} app {app}: {len(texts):,} reviews fetched so far")

    out, left = joined(claims, facts, texts)
    with Path(args.to).open("w", encoding="utf-8") as written:
        for row in out:
            written.write(json.dumps(row, ensure_ascii=False) + "\n")
    print(f"{len(out):,} claims with their text -> {args.to}")
    for why, count in left.items():
        if count:
            print(f"{count:,} left out: review {why}")


if __name__ == "__main__":
    main()
