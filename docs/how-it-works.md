# How it works

SteamGauge counts what every reviewer of a game says about each part of it, and every count
opens onto the reviews behind it.

## Every review

- **The whole census** is every review Steam serves for a game, in every language. It includes
  reviews of activated keys and review bombs, which Steam's defaults leave out.
- **Each game shows how much of Valve's total it holds.** Reviews by deleted and private accounts
  count in that total and cannot be fetched by anyone.
- **An update** fetches only what was written or edited since. While the app is open, it updates
  a game once Steam has 1% more reviews, which Settings turns off.

## Reading a review

A review is split into the points it makes, its **claims**: "looks incredible, runs like a
slideshow" is two. The reader then gives each claim:

- the subject it is chiefly about, one of 26, and any others it covers
- whether it praises, criticises or neither, for each subject
- how sure it is

A claim the reader is not sure enough of stays unfiled, and the share left unfiled is shown
beside every figure. What the reader does answer is right three times in four on games it never
trained on. [Accuracy](accuracy.md) has the figures.

The 26 subjects are performance, bugs and crashes, gameplay and mechanics, genre and
comparisons, overall verdict, says nothing about the game, story and writing, atmosphere and
feel, graphics and art, audio and music, controls and interface, difficulty and balance, amount
of content, mods and user content, price and value, monetisation and DLC, multiplayer and online,
community and players, updates and developer support, publisher and platform policy, hardware and
compatibility, accessibility and options, language and localisation, tutorial and learning,
licensed content, and VR and headsets.

The reader runs on your computer and comes in two sizes. The app uses the largest the graphics
card has room for, and without a card starts with the small one.

## What a figure counts

| Figure | What it is |
|---|---|
| Mention rate | The share of reviews that say something about a subject. A review counts once for each subject it raises, so rates add up to more than 100%. |
| Praised, criticised, mixed | Of the reviews raising a subject, the share that praise it, criticise it, or do both. |
| In the most helpful | How many times as often a subject comes up in the 50 reviews Steam shows first, against all of them. |
| Unfiled | The share of claims the reader declined. |

Every rate carries a 95% interval. Figures count English reviews unless you choose another
language or all of them. The choice recounts from the same download.

## Looking closer

- **What they praise and complain about** lists the points within each subject, such as one bug,
  with a count and a quote. A point needs five reviews.
- **What stands out** is a paragraph at the top of a game's page, written from those points on
  your computer. A sentence with a number or quote the points do not hold is left out.
- **Words that stand out** are the words a subject's complaints use far more than its praise, and
  the reverse.
- **Search** finds and counts the reviews and claims that use a word or phrase. Words of four
  letters or more also find longer forms, so "stutter" finds "stuttering".
- **Search by meaning** finds where reviewers said the same thing in other words or languages.
  Preparing a game for it takes minutes on a graphics card and hours on the processor.
- **Who wrote it** splits every figure by time played, Steam Deck, early access and a free copy.
  A kind of reviewer with fewer than 100 reviews shows no figure.
- **Around an update** compares each subject over the 28 days before and after an update post. A
  difference counts as a change only when it is wider than chance and at least two points.

## What you get

- **The window** shows what moved since you last looked, the library, each game's page, and games
  side by side.
- **A report** is one self-contained page with every figure and the reviews behind it, which
  fetches nothing and prints. See the
  [sample report](https://aureliolo.github.io/steamgauge/sample-report.html).
- **The data** is a folder of CSV and JSON per game, with a `README.txt` saying what each column
  holds.
