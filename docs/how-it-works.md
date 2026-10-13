# How it works

SteamGauge counts what every reviewer of a game says about each part of it, and every count
opens onto the reviews behind it.

## Every review

- **The whole census.** It downloads every review Steam serves for a game, in every language,
  including reviews of activated keys and reviews posted during review bombs, which Steam's
  defaults leave out.
- **Checked against Valve's total.** Steam sometimes stops serving a stretch of reviews early, so
  each date window is checked against Valve's count for it and walked again when short. Each game
  shows how much of Valve's total it holds; reviews by deleted and private accounts are counted
  by Valve and cannot be fetched by anyone.
- **Kept up to date.** Bringing a game up to date fetches only what was written or edited since.
  An edited review is kept in both forms. While the app is open it does this by itself once
  Steam has 1% more reviews of a game than the library holds, then reads the new ones; Settings
  turns it off.

## Reading a review

A review is split into the points it makes, its **claims**: "looks incredible, runs like a
slideshow" is two. The reader then gives each claim:

- the subject it is chiefly about, one of 26, and any others it covers;
- whether it praises, criticises or neither, for each subject;
- how sure it is.

A claim the reader is not sure enough of stays unfiled, and the share left unfiled is shown
beside every figure. The line is drawn for each subject and each language, so that what the
reader does answer is right three times in four on games it never trained on.

The 26 subjects: performance; bugs and crashes; gameplay and mechanics; genre and comparisons;
overall verdict; says nothing about the game; story and writing; atmosphere and feel; graphics
and art; audio and music; controls and interface; difficulty and balance; amount of content; mods
and user content; price and value; monetisation and DLC; multiplayer and online; community and
players; updates and developer support; publisher and platform policy; hardware and
compatibility; accessibility and options; language and localisation; tutorial and learning;
licensed content; VR and headsets.

The reader is a multilingual encoder fine-tuned on labelled claims and run as an ONNX graph on
your computer: through DirectML on Windows, Core ML on macOS, and the processor on Linux. It
comes in two sizes, and the app uses the largest the graphics card has room for.
[Accuracy](accuracy.md) has how often each is right.

## What a figure counts

| Figure | What it is |
|---|---|
| Mention rate | The share of reviews that say something about a subject. A review counts once for each subject it raises, so rates add up to more than 100%. |
| Praised, criticised, mixed | Of the reviews raising a subject, the share that praise it, criticise it, or do both. |
| In the most helpful | How many times as often a subject comes up in the 50 reviews Steam shows first, against all of them. |
| Unfiled | The share of claims the reader declined. |

Every rate carries a 95% interval. Figures count English reviews unless you choose another
language or all of them; the choice recounts from the same download.

## Looking closer

- **Words that stand out.** Each side of a subject lists the words its complaints use far more
  than its praise, and the reverse, each with how many reviewers used it. They are compared
  within each language, and every word opens onto the claims it was counted from.
- **Search.** A word or phrase finds and counts the reviews and claims that use it, by side and
  subject. Words of four letters or more also find longer forms: "stutter" finds "stuttering".
- **Search by meaning.** Finds where reviewers said the same thing in other words or languages,
  ranked by Qwen3-Embedding-0.6B and Qwen3-Reranker-0.6B and never counted. A game is prepared
  for it once: minutes on a graphics card, hours on the processor.
- **Who wrote it.** Every figure splits by time played (under 2 hours, 2 to 10, 10 to 30, 30 to
  100, over 100), Steam Deck, early access and a free copy. A kind of reviewer with fewer than
  100 reviews shows no figure.
- **Around an update.** A game's own update posts sit on its timeline, and each subject is
  compared over the 28 days before and after. A difference is called a change only when it is
  wider than chance and at least two points.

## What you get

- **The window**: a cockpit of what moved since you last looked, the library, each game's page,
  and games side by side.
- **A report**: one self-contained page with every figure and the reviews behind it, which
  fetches nothing and prints. See the [sample report](https://aureliolo.github.io/steamgauge/sample-report.html).
- **The data**: a folder of CSV and JSON per game, with every subject's figures, the timeline,
  every claim read and the game's updates, and a `README.txt` saying what each column holds.

`steamgauge recount` recomputes every figure from the readings already on disk, in seconds.
