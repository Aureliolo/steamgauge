# SteamGauge

![SteamGauge's mark, a pressure dial](crates/steamgauge-app/icons/128x128.png)

[![CI](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/ci.yml)
[![CodeQL](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql/badge.svg)](https://github.com/Aureliolo/steamgauge/actions/workflows/github-code-scanning/codeql)
[![Scorecard](https://api.scorecard.dev/projects/github.com/Aureliolo/steamgauge/badge)](https://scorecard.dev/viewer/?uri=github.com/Aureliolo/steamgauge)
[![Rust](https://img.shields.io/badge/dynamic/toml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2Frust-toolchain.toml&query=%24.toolchain.channel&label=rust&logo=rust&color=b7410e)](rust-toolchain.toml)
[![Tauri 2](https://img.shields.io/badge/Tauri-2-24c8db?style=flat&logo=tauri&logoColor=white)](https://tauri.app)
[![Training on Python](https://img.shields.io/badge/dynamic/yaml?url=https%3A%2F%2Fraw.githubusercontent.com%2FAureliolo%2Fsteamgauge%2Fmain%2F.github%2Fworkflows%2Fci.yml&query=%24.jobs.python.steps%5B1%5D.with%5B%27python-version%27%5D&label=training&logo=python&logoColor=white&color=3776ab)](training/)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-2f6f4e?style=flat)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/Aureliolo/steamgauge?style=flat)](https://github.com/Aureliolo/steamgauge/releases/latest)
[![Downloads](https://img.shields.io/github/downloads/Aureliolo/steamgauge/total?style=flat)](https://github.com/Aureliolo/steamgauge/releases)
[![SLSA Build L3](https://img.shields.io/badge/SLSA-Build%20L3-2f6f4e?style=flat)](.github/release-process.md#slsa)
[![SBOM SPDX](https://img.shields.io/badge/SBOM-SPDX-2f6f4e?style=flat)](.github/release-process.md#what-a-release-carries)
[![Sigstore attested](https://img.shields.io/badge/Sigstore-attested-2f6f4e?style=flat)](SECURITY.md#releases)

A tool for finding out what players of a game actually think, rather than what the loudest
reviews say.

## Install

Download SteamGauge from the [latest release](https://github.com/Aureliolo/steamgauge/releases/latest).
Each system has an installer and a portable archive with the program in it:

| System | Installer | Portable archive |
|---|---|---|
| Windows 10 or 11, x64 | `steamgauge-<version>-windows-x64-setup.exe` | `steamgauge-<version>-x86_64-pc-windows-msvc.zip` |
| macOS on Apple Silicon | `steamgauge-<version>-macos-arm64.dmg` | `steamgauge-<version>-aarch64-apple-darwin.tar.gz` |
| Linux on x86-64, Debian and Ubuntu | `steamgauge_<version>-1_amd64.deb` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |
| Linux on x86-64, Fedora | `steamgauge-<version>-1.x86_64.rpm` | `steamgauge-<version>-x86_64-linux-gnu.tar.gz` |

Or install it with a package manager. Each takes a release's file only if it matches the hash
the release is signed over.

```powershell
# Windows, with winget: the installer
winget install Aureliolo.SteamGauge
# Windows, with Scoop: the portable program, with a Start menu shortcut
scoop install https://github.com/Aureliolo/steamgauge/releases/latest/download/steamgauge.json
```

```bash
# macOS on Apple Silicon, with Homebrew: this repository is the tap
brew tap aureliolo/steamgauge https://github.com/Aureliolo/steamgauge
brew install --cask aureliolo/steamgauge/steamgauge
```

Scoop also takes this repository as a bucket, which `scoop update` then follows:
`scoop bucket add aureliolo https://github.com/Aureliolo/steamgauge`, then
`scoop install aureliolo/steamgauge`. Homebrew and Scoop both put `steamgauge` on the `PATH` for
the pipeline.

**Windows.** Windows 10 or 11 on x64. The setup installs SteamGauge for your user alone, with no
administrator rights, and installs Microsoft's WebView2 runtime where it is missing. Reviews are
read on any graphics card with DirectX 12, and on the processor where there is none. The program
is a windowed application, so it opens no console window. Given arguments from a terminal it
runs the pipeline, and its output reaches the terminal through a pipe: Git Bash makes one by
itself, and in PowerShell you add one, as in `steamgauge --help | Out-Host`.

**macOS.** A Mac with Apple Silicon. Open the `.dmg` and drag SteamGauge into Applications.

**Linux.** x86-64, and reading runs on the processor. The program needs glibc 2.39 or later
(Ubuntu 24.04, Debian 13, Fedora 40 and later) and WebKitGTK 4.1 for its window. The `.deb` and
the `.rpm` declare both, so the package manager installs what is missing with SteamGauge
(`sudo apt install ./steamgauge_<version>-1_amd64.deb`, or
`sudo dnf install ./steamgauge-<version>-1.x86_64.rpm`). The portable archive needs WebKitGTK
installed already: `libwebkit2gtk-4.1-0` on Debian and Ubuntu, `webkit2gtk4.1` on Fedora.

### The first launch

The installers carry no certificate from Microsoft or Apple, so both systems warn the first time
SteamGauge opens. [SECURITY.md](SECURITY.md#what-signing-does-and-does-not-tell-you) explains
what that certificate does and does not prove.

- **Windows** shows a blue SmartScreen box, "Windows protected your PC", naming an unrecognised
  app. Choose **More info**, then **Run anyway**.
- **macOS** says Apple could not verify that SteamGauge is free of malware, and offers only
  **Done** and **Move to Bin**. Choose **Done**, open System Settings, Privacy and Security,
  and choose **Open Anyway** beside the line about SteamGauge, then confirm with your password.
  Where macOS instead calls the app damaged, it is the quarantine flag on an app Apple has not
  notarised; after verifying the download, `xattr -dr com.apple.quarantine /Applications/SteamGauge.app`
  removes it.

Either way it asks once.

### Verifying a download

Every file in a release is listed in `SHA256SUMS` and carries Sigstore build provenance naming
the workflow and the commit it was built from. [SECURITY.md](SECURITY.md#releases) has the
commands that check a file against both.

### Upgrading

When a newer version is out, the window names it under the menu and offers **Update now**, which
downloads the new release's file for this computer and installs it:

- **Windows**: the setup program, which installs over this copy without asking anything, keeps
  your shortcuts, and opens the new version when it is done.
- **macOS**: the new app, put in place of the old one in a single step, and opened again.
- **Linux**: the `.deb` or the `.rpm`, installed by the system's package manager once you give
  your password; where no password prompt is available, or it is closed, the window offers the
  downloaded package to install yourself. A portable archive unpacked into a folder of your own
  has its files replaced.

The file is installed only when the release's signed build provenance shows that SteamGauge's
release workflow built it, for that version's tag, under the file's own name and with the
SHA-256 it arrived with. Anything else stops the update with the reason and nothing installed,
and the window offers the release page instead. [SECURITY.md](SECURITY.md#updates-from-the-window)
lists every check.

`steamgauge update` does the same from a terminal, without opening the window afterwards.

A copy Scoop installed, or a portable archive on Windows or macOS, is updated the way it was put
there: `scoop update steamgauge`, or the new archive's program in place of the old one. winget
and Homebrew install the same setup program and app the window updates, so the window updates
those copies too; `winget upgrade Aureliolo.SteamGauge` and `brew upgrade --cask --greedy
steamgauge` work as well. Settings turns the daily question off, and with it the notice. Your
library, your settings and the models already downloaded stay where they are.

## The problem it solves

Steam sorts reviews by helpfulness. If you read the top forty to judge a game, you are
reading the reviews other people upvoted, which are also the longest and angriest ones. That
is a measure of agreement, not of how common an opinion is.

A theme can fill the top of that ranking and still be a minority opinion among everyone who
reviewed the game. Both shares are true; only the second is a fact about players. The tool
reports the ratio between the two as a **bias factor**, so the distortion is a number you can
read rather than an argument you have to have.

The only way to remove the argument is to hold every review and count.

## What it does

- Find a game by its name, its app ID or its store link, or take a whole list of app IDs.
- Download **every** review for those games, not a sample, and bring the capture up to date
  later without downloading it again.
- Split each review into the points it makes and sort each point into a subject, with a
  measured error and an honest "cannot tell".
- Show, per subject, the words the praise uses and the complaints use, counted by reviewer.
- Build a picture of the game in a paragraph from those counts.
- Split every figure by who wrote it: how long they had played (under the two-hour refund
  window, 2 to 10 hours, 10 to 30, 30 to 100, 100 or more), whether mostly on a Steam Deck,
  whether during early access, and whether they got the game free. One kind of reviewer is shown
  beside everyone else or beside another kind, with its timeline and the reviews behind each
  figure, and the differences wider than chance are listed in plain sentences. A kind with fewer
  than 100 reviews shows no figure at all.
- Click through from any number, anywhere, to the actual reviews behind it.
- Mark a game's own updates on its review timeline, named, dated and linked to their posts on
  Steam, and show each subject's praise and complaints in the 28 days before an update against
  the 28 days after. A difference is called a change only where it is wider than chance and at
  least two points, and an update with too few reviews either side says so instead.
- Ask a game about anything, "Steam Deck" or "desync": what its reviewers said in those
  words, counted, and what they said in other words or other languages, found by meaning.

Results come out as one self-contained page: every rate, the reviews behind it, and what the
classifier is measured to get wrong, in a single file you can open from disk, send to someone,
or print. It fetches nothing, because a corpus that never left your machine should not start
leaving it the moment somebody looks at it.

A read game's data also exports for a spreadsheet or your own analysis: "Export data" on its
page, or `steamgauge export-data <app id> --to <folder>`, makes a folder of CSV and JSON files.
`subjects` holds every subject's mention rate, praise, complaints and both, for everyone and for
each kind of reviewer; `months` the timeline; `points` every point read, with its subject, side,
the reader's sureness, its exact words and the review it came from (link, day, language,
recommendation, time played, Steam Deck, early access, free copy, helpful votes); `updates` the
game's updates. A `README.txt` says what every column holds, which reader read the game and how
far its figures can be trusted. A figure the app does not show as a number is left empty, with
the reason in a column beside it. The CSV opens in Excel with every language's letters intact,
and a review that starts like a formula is written so a spreadsheet shows it rather than runs it.

The app reads with the largest reader your graphics card has room for. Without a card it
starts with the fast one and offers the most accurate beside it, saying how long each takes on
that computer.

The desktop app opens on a cockpit: what changed since you last looked, game by game, with the
subjects whose share in the new reviews differs from the year before by more than chance;
everything running, with its speed and time left; the library's state, including the games Steam
has new reviews for and the games read by an older reader; what moved in the last three months
across every game, where the change is wider than chance; and which reader and models this
computer uses. Each game's page says what changed since you last looked at it. The library lists
every game, sorted and grouped as you like, and updates, reads, compares or reports on any
selection of them. Work runs in the background, downloads beside reads, and how much of the
graphics card a read may take is a setting.

While the app is open, it brings a game up to date by itself once Steam has a hundredth more
reviews of it than the library holds, and reads the new reviews with the reader in use. That work
gives way to anything you start, and stops when the app closes; Settings turns it off. A desktop
notification, off until you turn it on, names a game and a subject when such an update finds the
subject moved, and clicking it opens the game.

Claude Code, or any other program that speaks the Model Context Protocol, can steer the app.
Settings shows the command that adds it to Claude Code, `claude mcp add steamgauge --` followed
by this copy's path and `mcp`. Every tool is something the window can do, from finding,
downloading and reading a game to searching its reviews, exporting, changing settings and
removing things, and the window shows each one done. The open app answers; where it is not
running, it opens with its window hidden and closes once the program is finished with it and its
work is done. It listens on a local socket that no other user and no other machine can reach. For
a program that only connects over HTTP, Settings can also have the open app answer on
127.0.0.1, at a port you choose, to requests that carry a token Settings shows; it is off until
you switch it on.

### What a percentage means here

The headline figure for a category is a **mention rate**: the share of all reviews in the
corpus that say something about it. A review that covers two topics counts towards both, so
mention rates across categories add up to more than 100%, and they are meant to. This is the
number people usually have in mind when they ask how common a complaint is.

Where a figure is not a mention rate, it is labelled. Two others appear:

- **Primary share**, the share of reviews whose single main subject is that category. These
  are exhaustive and add up to the number of reviews.
- **Claim share**, used in deep reading, where the unit counted is an individual opinion
  rather than a whole review.

No percentage is ever shown without saying which of the three it is. The point of counting
everything is lost if the denominator is ambiguous.

### The unit is a claim, not a review

A review is not one opinion. "Looks incredible, runs like a slideshow, and the story is the
best in the series" is three, about three different things, and a single vector for the whole
review is their average: a point that belongs to none of them. So every review is first split
into the separate points it makes, and a subject belongs to a point rather than to a review.

That is what keeps the arithmetic honest at both ends. A review that makes twelve points
contributes to twelve subjects instead of being flattened into one. A point carries the subjects
it names and no others, which is the part that matters more than it sounds: a two-word review
cannot be filed under four topics, because it does not contain four, and "great music, awful
controls" is one point about two things, each with what it says of it.

A review's subjects are the union of its claims' subjects, and it still counts once towards
each of them. The headline is a mention rate, a share of reviews, and it stays that way
deliberately: counting claims instead would let whoever writes most set the numbers, which is
the same distortion the tool exists to expose at the top of the pile.

### What reads a claim

A model trained on labelled claims, and nothing else. It says which subject a claim is about,
whether it is praise, a complaint or neither, and how sure it is. Below a calibrated threshold
it says nothing at all, and those claims are reported as unclassified rather than filed under
whichever category happened to be nearest.

That last sentence matters more than any other. Steam is full of two-word reviews such as
"gfg", and a method that always picks the nearest category files each of them under some
subject, adding a fraction of a percent to a rate that is supposed to be a fact about players.

The model is fine-tuned from a multilingual encoder and shipped as an ONNX graph, so it runs
on the same local runtime as everything else. No API key, no network, no account. Which
encoder it starts from is decided by measurement across candidates on identical labels and an
identical split, judged on accuracy, throughput, and how well its confidence tracks whether it
is right, because a model that cannot tell when it is guessing cannot be allowed to abstain.

**The threshold is chosen by what it promises, not by how much it answers.** Maximising
accuracy times coverage, the obvious way to pick one, has a degenerate optimum on a model that
is not yet good: coverage rises faster than accuracy falls all the way down, so the sweep
settles at its own floor and the model answers everything. The threshold is instead the one
giving the most coverage at a promised accuracy, three answers in four right, drawn for each
subject and for each language, and a claim is answered only when it clears both. When no
threshold reaches that accuracy the model is recorded as not good enough rather than quietly
lowered to whatever it can manage.

Some categories are defined by what a claim does *not* say. A bare verdict and a claim that
says nothing about the game are both statements that no aspect was named, and on a corpus of
real reviews they are the commonest labels there are. A verdict can also stand beside an aspect,
"a few bugs, but I still find it fun", so the verdict row counts every claim that judges the
whole game, not only the ones that do nothing else.

Where two categories genuinely overlap, the taxonomy settles it with a written rule rather
than leaving each labeller to decide: replayability and repetitiveness are amount-of-content,
balance is difficulty, animation speed is graphics, port requests are compatibility, sequel
requests are a verdict, a system nobody explains is tutorial however good the system is,
calling a game co-op is naming a kind of game and belongs to genre, and a protest about
anything the publisher did belongs to policy even when it names no particular term. The rules
ship with the taxonomy and generate the sheet every labeller works from, so a boundary can
only be defined in one place.

A category is added when labellers report having nowhere to put something, not when somebody
thinks of one.

### Depth is how closely each review is read

Every review is analysed. Depth does not decide how many are included, it decides how finely
each one is taken apart. **Deep** is the default and is described above: a review becomes the
points it makes. **Shallow** treats the whole review as one point, which is faster and
systematically understates anyone who wrote more than a sentence. Neither setting drops a
review.

### Praised, criticised, or both

A thumb is attached to a review, not to a subject. Somebody who loves the art and despairs of
the framerate has one thumb and two opposite opinions, and crediting both subjects with the
same verdict is a straightforward misreading of what they wrote.

So polarity belongs to the claim, and is reported per review per subject: of the reviews that
discuss performance, the share that criticise it, the share that praise it, and the share that
do both. **Mixed** is a real answer and appears as one. It is the most interesting thing a long
review has to say, and any tool that forces it to a single sign is throwing that away.

Claim-level polarity is available underneath, for reading rather than for headlines, and is
labelled as what it is: a count of opinions, which the most talkative reviewers dominate.

### What they said about it

A fifth of reviewers complaining about performance is a count. Whether they mean stutter,
crashes or load times is the thing a reader opened the row for, and no local model is asked to
paraphrase anybody to say it. Instead, each side of a subject shows the **words that stand
out**: the terms its complaints use far more than its praise does, and the reverse, each with
the number of reviewers who used it. "stutter", "crashes" and "memory leak" under performance,
each with its count, is what was said, and every term opens onto the exact claims it was counted
from.

The comparison is complaint against praise within one subject rather than against the corpus,
because against the corpus a subject's vocabulary is mostly its own name: "fps" stands out in
every performance claim and tells nobody anything. The ranking is a log-odds z-score with a
half-count prior, so a word three reviewers used and nobody on the other side did does not
outrank one three hundred used against ten, and a side with a handful of reviews shows nothing
rather than promoting whatever those few happened to write. Counts are by reviewer, once per
review however often it repeats itself, for the same reason the headline is a mention rate.

The comparison is also made within each language and then pooled, because reviews written in
thirty languages come from speakers who do not praise and complain in the same proportions.
Compared across languages, the word for "story" in whichever language liked the story most would
stand out in the praise and say nothing but "story". Each language's praise is compared with its
own complaints, and a word a language uses either way contributes nothing. What a subject is
called in each language stays off its rows as the English label does, read from the corpus
rather than translated: "сюжет" under story says what the row said.

A term is a word or a pair of adjacent words. Chinese is cut into words by a dictionary (jieba,
with the words of the trade added, since a general dictionary reads 掉帧 as "drop" and "frame");
Korean is spaced words with the particle taken off; Japanese, which has no dictionary here, is
cut into pairs of adjacent characters, the best that can be done without one. Everything the
page shows is added up when a game is read, so a change to the adding up does not cost the
hours of a reading again: `steamgauge recount` replays the stored readings through the same
counting in seconds, and refuses if this build takes a review apart differently from the build
that read it.

### Asking it about anything

The subjects are the questions the reader was trained to answer, and the words that stand out
are the ones the counts surfaced on their own. Neither answers "what do they say about the
Steam Deck", which is the first thing a developer asks. Every read game has a search box for
that. Type a word or a phrase and it counts the reviews and the points that say it, split into
praise and complaint and by the subject each point was filed under, with the forms it was found
in ("steam deck" and "steam decks", each counted) and every point quoted in its review. A word
of four letters or more also finds the words it begins, so "stutter" finds "stuttering"; a
shorter one finds only itself, so "art" does not find "artist".

Those counts are of the words typed and nothing else. Below them the app can also find where
reviewers said the same thing in other words or in another language: "boring" finds "재미없음",
"无趣" and "pretty dull after a while". That is ranked and never counted, because meaning has
no line at which saying it stops, and a number would only say where the line was drawn. Two
models do it, Qwen3-Embedding-0.6B to gather the hundred points nearest the search and
Qwen3-Reranker-0.6B to read each of them beside the search and put them in order. Reading the
two together is what the encoder cannot do on its own, and it is what pushes opposites ("fun"
for "boring") down the list. It needs each game prepared once: minutes on a graphics card,
hours on a processor. So the app asks first, says what it will cost on that computer, and
recommends from what it can see.

### Ratings that disagree with the text

A thumbs-down is not always a complaint. "0/10, haven't slept in three days" is praise wearing
a costume, and counting it as negative quietly poisons every number downstream. The reverse is
just as common: a recommendation that is really a warning not to buy yet.

These are **flagged, not filed away**. What a labeller flags is what the text does, judged
from the words alone: they are never shown whether the reviewer recommended the game, so that
they cannot be led by it, and so they are in no position to report that the two disagree.
Putting that flag next to the rating is what finds the disagreement, and that is arithmetic
rather than a judgement. The review still sorts by what it is actually about, so the praise
buried in a joke review still counts as praise of whatever it praises.

The flag lives on labelled reviews and nowhere else. Reading it off a whole corpus needs a
classifier measured to find irony, and nothing here measures that. What the reference sets give
is the rate: how often, in reviews drawn at random, the text and the rating point opposite ways
at all.

## How it knows whether it is right

A census that cannot say how often it is wrong is just an opinion with decimal places.

The model is measured against **reference sets**: claims labelled one at a time, stored under
`reference/claims/<app id>/` with the drawn sample beside them. Whole games are held out rather
than whole claims, because two claims from one review are not independent evidence and a score
that mixes them is a score for how well the model repeats itself. A report scores a game
against its own set only when the model never trained on it: on the games it learned from it
reproduces its labels at 99%, and a page that printed that as agreement would advertise its
memory.

Three things are reported together, and separating them is what makes the number mean anything:

- **Agreement**, over the claims the model was willing to answer for.
- **Abstention**, the share it declined. A score that quietly drops those is a score for a
  classifier nobody is running.
- **Contest**, the share the labeller marked as genuinely ambiguous, reported apart from the
  rest. Disagreement there says as much about the taxonomy as about the model.

### What it is worth against the alternatives

Measured over one set of claims drawn from the frozen games, twenty a subject, which chose
nothing about any row: 458 claims, labelled under the category sheet as it stands. The key is
`reference/frontier-key.json`, and the frontier model's answers are
`reference/frontier-opus-answers.json`. Every row abstains where it is unsure, and every row is
scored only on what it answered, because a score that quietly drops the declined claims is a
score for a classifier nobody is running. The readers are scored on DirectML, where the tool
runs them, and the baselines are fitted on the same labels the reader learned from.

| | answers | agreement where it answers | macro F1 |
|---|---|---|---|
| the commonest subject | never reaches the promise | 5.9% | 0.004 |
| TF-IDF bag of words | 48% | 75.9% | 0.508 |
| nearest subject centroid over an untuned encoder | 3% | 83.3% | 0.448 |
| **this reader, 560M parameters** (`e5inst-pool-person-rules-s1`, the one that ships) | **93%** | **80.7%** | **0.765** |
| Claude Opus 5, given the same sheet | 99.3% | 85.9% | 0.860 |

On the whole frozen set, 5,080 claims as they come, the reader that ships answers 91.5% at 81.5%
(macro F1 0.707): a stratified sample is the harder question, and the one above.

**Every row is the same claims, and that is not a detail.** Read on the corpus as it comes, a
quarter of which is `verdict`, the commonest-subject baseline scores 28.8% rather than 5.9% and
TF-IDF answers 46% of claims at 75.1% rather than 48%. A stratified sample is the harder
question and the useful one, because the rows a reader has to get right are the rare ones.
Both sets of figures are kept, in `reference/baselines-frontier-sample.json` and
`reference/baselines-frozen.json`, and a row from one does not belong in a table with a row
from the other.

The third row is why the reader is trained rather than matched against descriptions: cosine
distance to a prototype cannot say "this is about nothing", so at the accuracy it promises it
can answer about one claim in forty.

What separates the reader from the bag of words is reading each claim inside its review, a
pretrained encoder of 560M parameters, forty-five thousand labels, many of them drawn at the
subjects the reader was worst at, and a teacher: a 4B-parameter reader trained on the same
labels, whose answers on a quarter of a million unlabelled claims the small one learns from as
well. Together that is forty-five points of coverage over the bag of words
and a quarter of macro F1. The teacher alone is worth four points of coverage, measured against
the same recipe taught by the small reader's own seeds instead; a sweep of everything else,
some fifty configurations measured the same way, moved nothing outside its own noise.
`DECISIONS.md` has the tables and what each change was worth on its own.

The last row is the one worth being honest about. **A frontier model asked directly is better
than this, by five points of agreement and seven of coverage.** What it is not is
affordable: its answers cost 389,000 tokens for 487 claims, and a single large game holds
millions of claims. This reader does that game on one desktop GPU, offline, for the
electricity. The claim being made is not that a 560M-parameter model beats a frontier one. It
is that it gets most of the way there at four orders of magnitude less cost, and that it can
tell you exactly how far short it falls.

The two models in that table are deliberately different ones. The labels this reader was
trained from were written by **Claude Fable 5.1**, and from 27 September 2026 by **Claude Opus
5.5**; the model it is measured against is **Claude Opus 5**, which wrote none of them. A
teacher scoring its own student would make the gap meaningless, and the gap is the point.

Every figure in that table is agreement with those labels, and the labels are a model's. The
one person who has adjudicated 200 of them, answering cold, agrees with the labels on 65% of
subjects and with the second labeller on 64%, which is the section below and the number to hold
the table against.

### The reader in each size

The reader comes in more than one size, and the desktop app picks the largest one the machine's
graphics card has room for. Where there is no card to use it starts with the smallest and offers
the most accurate beside it, saying how long each would take on that computer. Every size below
learned from the same labels and the same teacher, a 4B, and every figure in a column was
measured the same way for every row. There is no larger size because the teacher itself, trained
on exactly these labels, answers its surest claims less accurately than `standard` does.

<!-- reader sizes: rendered from reference/reader-sizes.json by training/sizes.py -->
| Reader | Parameters | Download | Accuracy at 80% coverage | Accuracy at 90% coverage | GPU memory | One large game on the GPU | One small game on the CPU |
|---|---|---|---|---|---|---|---|
| small | 118M | 244 MB | 77.9% | 73.8% | 1.2 GB | 62 s | 36 s |
| standard | 559M | 1.1 GB | 84.7% | 82.0% | 2.6 GB | 125 s | 285 s |

- **Accuracy at a coverage:** 458 claims from ten games held out of training, labelled under the
  current category sheet. Each reader answers the given share of claims it is most confident about,
  scored on DirectML (`frontier.py reader`).
- **GPU memory and one large game on the GPU:** one read of game 920210 (117,664 claims) through the
  desktop app on DirectML, on an otherwise idle NVIDIA GeForce RTX 4090. Memory is the peak during
  the read minus what the GPU held before.
- **One small game on the CPU:** 1,416 claims of game 1888930 on the CPU only, loading included, on
  an AMD Ryzen 9 5950X with other work running.
<!-- end of reader sizes -->

### How a person turns silver into gold

`steamgauge gold` writes one page, holding a blind random sample of claims from the games the
model never saw and the claims two labellers answered differently. Blind means blind: a claim
drawn for measurement carries no answer, because an answer on the page is an answer in the
reader's head, and a figure produced by agreeing with a suggestion is a ratification rather
than a measurement.

The page marks each claim inside the review it came from and keeps answers as they are made. A
letter picks a subject, a digit picks the polarity, and a claim with both moves on by itself; a
thousand claims is not one sitting and a closed tab must not cost a night's work.

**`steamgauge gold --serve` is the way to run it.** The page is served from the loopback
address and every answer lands in a file on disk before the next question is drawn, so the disk
is the copy that matters and the browser is a cache: reopen it anywhere and it carries on where
the file ends. Nothing leaves the machine, and there is nothing to remember to press. Opened as
a plain file instead, the page still works and still asks nothing of the network, but the
browser is then the only copy until the Export button is pressed, which is a bad place for the
only copy of somebody's own judgement.

`steamgauge ingest-gold` reads those answers back, files them beside the labels already there
rather than over them, and prints the share that agrees. That share is the one number here
that may be called accuracy rather than agreement.

**What it says.** One person has adjudicated **200 claims**: 169 drawn blind, in English, from
the frozen games, and 30 the two labellers were sure of and still disagreed about. Answering
cold, before seeing any label, the person named the same subject as the labels the silver
standard is made of on **65.1%** of the blind claims (58% to 72% with 95% confidence), and as
the second labeller on 63.9%; where the two labellers agree with each other, 149 of the 169,
they agree with the person 68.5%. Polarity holds at 85.8%. Two models agree with each other a
good deal more than either agrees with a person, and "both labellers said so" is right about two
times in three. Most of the difference sits at the boundaries the gap list already names:
content against gameplay, genre against verdict, story against gameplay and content. Those
first answers are the gold set as committed in `4c07445`.

Every answer that differed from the labels was then read again, with both labellers' answers and
the sheet's own rule for each category on the card, and the set as filed carries the second
answer. Most of the changes were rules the sheet already had and the person had not applied. One
rule did not carry: the sheet files "the best roguelike out there" under verdict, and with that
sentence in view the person still read "Best Metroidvania I played" as genre. Against the set as
filed the labels name the person's subject on **82.2%** of its 197 claims. That figure is not
blind and is not quoted as accuracy; it says how far the labels and a person agree once the
person is applying the same sheet.

**Right, or something the person would also accept.** A claim can fairly sit under two
subjects, and exact agreement counts every second subject as wrong. So the person has also
judged, for each gold claim where the reader named another subject, whether that one would also
do. Against the 151 blind claims as filed, the reader that ships names the person's own subject
on **84.8%** of those it answers (78% to 90%) and one the person accepts on **98.6%**; on the 46
claims the two labellers disagreed about, 50.0% and 92.9%. `DECISIONS.md` has the whole pass.

### How the reference sets are made

They are a **silver standard**, not a gold one, and the distinction decides what every number
downstream may be called. A gold standard is adjudicated by people. These labels are written by
a language model reading one claim at a time, which makes the set good enough to train a model
on and to measure against, and never good enough to quote as truth. Every manifest records
`human_verified: false`, and until that changes the tool reports **agreement** and refuses the
word accuracy.

Which model wrote them is recorded on every label, in `produced_by`, and printed with every
result. A set labelled by one model and a set labelled by another are not the same evidence and
must not be pooled without saying so; the sets shipped here were written by Claude Fable 5.1,
and every label written from 27 September 2026 by Claude Opus 5.5.

What the set spends its size on is games rather than depth. A hundred reviews of one title
would say nothing about whether a category survives contact with a corpus it was not built
from. So the set runs to thousands of labels spread across dozens of games of different genres
and different overall sentiment, and the figure it exists to produce is the one measured on a
game the model never saw. Nearly two thirds of it is labelled twice by different labellers,
which is what lets the set report its own reliability rather than only its agreement with a
classifier.

The mix of languages is chosen rather than inherited. A corpus is whatever languages its
players happen to write in, and drawing straight from it would train the model mostly on
whichever one that is. Roughly seven claims in ten are English and the rest are drawn from
everything else the corpus holds, so the model is trained on the languages the reports do not
default to. Whether it reads them as well is a separate question, it is answered for four of
them, and the limits below say what the answer is.

The protocol is fixed so it can be repeated, and so a disagreement with it is about the method
rather than about somebody's afternoon:

- **The sample is drawn before anyone reads anything.** `steamgauge sample-claims` takes a seed and
  draws reviews per game with a fixed share of English, then splits each into its claims. The
  same seed against the same capture draws the same reviews, so a set can be rebuilt without
  being stored.
- **Every claim of a drawn review is labelled, never a subset of them.** A review labelled in
  part cannot say what share of a corpus names no aspect at all, which is the first thing worth
  knowing about one.
- **A set drawn to teach the model is marked as one and measures nothing.** A random draw
  spends most of its budget on claims the reader already gets right, so `steamgauge declined`
  draws instead from the claims it abstained on, uniformly rather than from the least confident
  of them, because the bottom of a confidence ordering is mostly text with nothing in it. Every
  row lands with `subset: declined`, which keeps it out of every prevalence figure; only the
  claims drawn are asked about, though the whole review is still handed over, because a claim
  reading "it doesn't" cannot be labelled without the sentence before it; and only games the
  model already trains on may be drawn, because a held-out game taught from is not held out.
- **Labellers are shown the claim inside the review it came from, and nothing else.** Not the
  game, not whether the reviewer recommended it, not what the model guessed. The prediction is
  withheld because anyone shown a proposed answer agrees with it more than someone reading
  cold. The rating and the game are withheld for a different reason: the model does not see
  them either, so a label made from more than the tool can read would measure the gap in what
  the two were shown. The surrounding review is shown because a claim reading "it doesn't" is
  not interpretable alone.
- **The sheet every labeller works from is generated from the taxonomy**, so a boundary rule
  exists in exactly one place and every labeller is given the same one. It is never changed
  mid-run: half a set labelled against a revised sheet is half a set nobody can compare.
- **Labelling runs in parallel, one labeller per game**, each working batch by batch and writing
  each batch out before opening the next.
- **Every label carries seven fields**: the subject the claim is chiefly about, whether it is
  praise, a complaint or neither, every other subject it covers with a polarity for each, whether
  the text is ironic, how sure the labeller was, whether the call was genuinely contested, and
  whether the claim was cut in the wrong place. The last two are read back:
  agreement is reported separately over the contested claims, and the mis-split rate is what
  drives the splitting rules.
- **Every game the model is measured on is read again by a different labeller, blind, in full,
  and a share of every other set is too.** That is 21,449 of the 33,615 claims the random draws
  hold, nearly two thirds, and every one of the 5,579 the frozen games hold.
  `steamgauge second-opinion` draws the same reviews as fresh batches with no labels in them,
  and `steamgauge compare-labels` reads the two labellings together. It reports each field apart
  from the others, because they fail differently: subject is a judgement about the claim, and
  `ambiguous` is a judgement about the taxonomy. Beside every percentage is Cohen's kappa, which
  is what the percentage cannot tell you: a corpus is mostly `verdict` and `offtopic`, so two
  labellers who never read a claim would still agree most of the time by landing on the
  commonest subject.
- **What comes back is checked rather than trusted.** `steamgauge ingest-claims` refuses a set that
  does not cover the drawn sample exactly: claims nobody labelled, labels naming claims nobody
  drew, subjects the taxonomy does not have, claims labelled twice, and labels whose judgements
  were never made. A judgement left out is dropped rather than defaulted, because a `false`
  nobody wrote is a figure nobody stood behind.

These rules keep those figures honest:

- **Agreement is not accuracy.** Where the labels were written by a model, what gets measured
  is consistency between two models, and the tool says so on every report until a person has
  checked part of the set. Two models can be wrong together, most easily on sarcasm and on the
  boundaries between categories, which is exactly where classifiers are weakest.
- **Every rate carries a 95% interval.** Reference sets are small. At fifty reviews the honest
  band around a rate is roughly twenty-five points wide, and a bare percentage invites
  conclusions the counts cannot support.
- **A category that is wrong says what it is wrong about.** Knowing a category scores badly
  says nothing about what to do; knowing it is read as one particular other category is a
  boundary the taxonomy has not settled, and no amount of fitting will settle it. Every
  evaluation names the category each one is most often mistaken for, where that is a pattern
  rather than a single review.
- **Every row says how well it is measured, not just the page.** One agreement figure for a
  whole taxonomy hides the shape of the error: the same run finds nine mentions in ten of one
  category and one in five of another. Each category carries its own precision and recall
  from the reference set, and a category the classifier is measured to miss most of is marked
  as a floor rather than a count.
- **A difference the reader cannot see is not marked as one.** The page draws two kinds of
  claim on top of its numbers, and each is earned rather than automatic. Across games, a
  category is outlined as belonging to one of them only where that game clears every other by
  more than the rounding in the printed figures. Within a game, a category is coloured as warm
  or cold only where the interval on the reviews raising it clears the game's own baseline, so
  eight reviews all recommending the game is left as the 100% it is instead of dressed as the
  finding a thousand reviews at 98% would be.
- **Reference sets span several games, and whole games are held out.** A model trained on one
  game carries that game's vocabulary. Sets are labelled across games of different genres and
  different overall sentiment, and the split holds out whole games rather than whole claims,
  so the figure says whether a subject travels rather than whether the model can repeat itself.
  Numbers from a single corpus describe that corpus and nothing else. The same figure is
  reported again over the claims the labeller called clear-cut and over the ones it called
  contested, because those answer different questions: a model that genuinely cannot reach a
  game is worse where the reading was easy, and a taxonomy that does not fit a game is worse
  only where the labeller could not place the claim either.

## Honest limits

- **Reports default to English, so the headline is a fact about players who write English.**
  The capture is always the whole census, every language in it, and the filter is applied when
  the counting happens rather than when the crawling does, so the choice is reversible and no
  corpus has to be downloaded twice. But a mention rate over English reviews is not a mention
  rate over players: on some corpora English is under half of what was written. Every figure
  says which set it is over, and switching the language recounts from the same capture. The
  reason for the default is that evidence nobody can read is evidence nobody can check, and
  being able to open a rate and read what is behind it is the whole design.

- **The reader is measurably worse in five of the languages it reads, and the reference set
  is 70% English.** Steam reviews of many games are far less English than that, so the
  languages the reader sees most outside English are the ones it learned from least.

  Measured over 40,327 claims held out by cross-validation, each answered by a model of the
  recipe that ships that never trained on the game it came from, over every claim whether or
  not the reader would answer it (`training/language.py`); the last column keeps only the claims
  the labeller marked as sure:

  | | claims | agreement | 95% interval | labeller sure |
  |---|---|---|---|---|
  | english | 28,250 | 75.8% | [75.3, 76.3] | 93.0% |
  | turkish | 529 | 76.0% | [72.2, 79.4] | 95.6% |
  | german | 1,604 | 75.2% | [73.0, 77.2] | 92.0% |
  | polish | 354 | 74.6% | [69.8, 78.8] | 92.4% |
  | french | 1,089 | 74.2% | [71.5, 76.7] | 89.4% |
  | brazilian | 868 | 73.5% | [70.5, 76.3] | 91.7% |
  | **russian** | **1,508** | **71.4%** | **[69.1, 73.6]** | **92.1%** |
  | **spanish** | **841** | **70.2%** | **[67.0, 73.1]** | **87.4%** |
  | **japanese** | **489** | **69.9%** | **[65.7, 73.8]** | **84.5%** |
  | **schinese** | **2,573** | **69.6%** | **[67.8, 71.3]** | **89.6%** |
  | **koreana** | **518** | **65.3%** | **[61.1, 69.2]** | **88.3%** |

  Turkish, German, Polish, French and Brazilian Portuguese match English. **Russian, Spanish,
  Japanese, Simplified Chinese and Korean sit below it on intervals that do not overlap, Korean
  by ten points.** Nothing on a report page says so. Ten of the twenty-nine languages have fewer
  than a hundred held-out claims and are not quotable at all.

  Part of each gap is the labels rather than the reading, since the column is agreement with one
  labeller and carries that labeller's noise. On the claims the labeller was sure of, Russian's
  gap all but closes and Korean's and Simplified Chinese's halve; Spanish and Japanese stay as
  wide.

  **The promise holds in every language.** The reader says a claim it answers is right three
  times in four. A line drawn per subject alone is set mostly by English, which is most of the
  set, so a line is drawn per language as well and a claim is answered only when it clears both.
  The cost is coverage, most of it in the languages read worst: the line a Korean or a Spanish
  claim has to clear is three times the English one. A language with fewer than a hundred
  labelled claims declines rather than borrowing a line fitted on English.

- **The model declines claims it is not sure about, and those are counted rather than hidden.**
  A claim below the threshold gets no subject and is reported as unclassified. That is a real
  answer and an honest one, but it means a mention rate is a rate over the claims the model
  would commit to. The share it declined is printed beside it, and a large one is a finding
  about the corpus rather than a footnote.

  On the frozen games, which it never trained on, the reader that ships answers **91.5%** of
  the labelled claims and agrees with a labeller on **81.5%** of those. Its lines are drawn for
  what they promise, not moved to make that coverage look better, and the share it declines is
  printed beside every rate.

  The model carries what it usually declines, drawn from the folds its abstention lines were
  fitted on, so a corpus that declines far above it can be reported as a finding rather than a
  footnote. The reader that ships carries **10.9%**.

  A game that declines 1.2 times that and five points more is flagged, and the flag has two
  causes: a corpus about something the taxonomy lacks, and a corpus written largely in the
  languages the reference set covers worst, whose lines are highest. Those want opposite
  responses, the first a category and the second more labels, so the page names the languages
  in the corpus the reader has to be surer about than English, and those it has no line for at
  all, with the reviews written in each.

- **A threshold chosen on a few games may not transfer to a new one.** The threshold promises
  an accuracy, and that promise is measured on the games that chose it; on games the lines were
  fitted on too few of, the frozen ones can deliver well short of it. So every figure in the
  model card comes from the frozen games, and the validation figures stay in the run record
  where they belong. Which games are frozen is fixed
  by a hash of each game's id, so adding games never moves one across the line.

- **The tool has to reproduce the training measurement, and when it does not the tool is
  wrong.** They are separate implementations of one question: the trainer builds the window
  around a claim in Python, the tool builds it in Rust over a corpus it split itself. Three
  ways of asking exist so that any two can disagree, and each removes a suspect: Rust over the
  exported claims, Python over the same, and the tool over its own reading of the game. A fault
  in how one of them builds the window around a claim leaves nothing in the output looking
  wrong, and shows only as one of the three answering below the other two, which is the
  argument for keeping all three.

- **Claim share is verbosity-weighted and never a headline.** Counting opinions instead of
  people lets whoever writes most set the numbers, which is the same distortion this tool
  exists to expose at the top of the pile. The headline is always the share of reviews.

- **Nearly two thirds of the reference set is labelled twice, and the rest is labelled once.**
  Reliability is measured on the part read twice and assumed for the rest. That is not the same
  as a set where every label was adjudicated. The silver standard's own error is estimated rather
  than known, and the word accuracy still does not apply: the labels were written by a model, so
  what is measured is consistency between two models.

  Measured over 21,449 claims on every one of the 51 games, with the ten frozen games read
  twice in full: two labellers agree on the subject 88.6% of the time, kappa 0.87, and on
  polarity 94.9%, kappa 0.92.

- **The contested flag measures the labeller as much as the claim.** Two labellers given the
  same definition reached for it on 27.4% and on 43.9% of the same 21,449 claims, kappa 0.46.
  The flag does find the right claims: where neither reached for it the two agree on the
  subject 98.8% of the time, where one of them did 82.4%, and where both did 72.5%. What
  differs is the bar. So a game's contested rate is not compared with another game's, and
  agreement is reported over the contested claims as a floor on how hard the taxonomy is rather
  than as a property of the corpus.

- **A labeller is told one thing about the game, and the model is told the same thing.**
  Withholding the game keeps the label answerable from what the model reads. The one exception
  is whether the store lists the game as played only in a VR headset: two of the sheet's rules
  turn on it ("if you have a headset, get this" divides no readers of a headset-only game; motion
  sickness with nothing named means a headset in one), the text rarely says it, and so the
  handout carries `headset_only` and the model's window opens with "Played in a VR headset." on
  those games. `steamgauge store-facts` asks the store and keeps the answer beside each game.
  A labeller is still given one game at a time, and a corpus with a strong accent can still give
  its game away beyond that one fact; the effect runs one way, towards labels the model cannot
  reproduce, so it understates the model rather than flattering it.

- **A claim is split mechanically, and the splitting is sometimes wrong.** Every label records
  whether its labeller thought the claim was cut in the wrong place, and every rule in the
  splitter answers some of those reports: a list of short comma-separated parts, "stunning
  visuals, calm music, epic story", is that many claims rather than one carrying three
  subjects. Re-cut with the splitter this build ships, **9.1%** of the 30,414 randomly drawn
  claims are still cut the way the labeller objected to (`--example stale-splits`). It is a
  floor rather than the rate: a claim the splitter cuts differently from the one the labeller
  read is not thereby cut correctly. The rate is measured rather than assumed, because it is
  the one error in this pipeline that no amount of training fixes.

  A label names the span of the review it was written about, not a position in a list, so the
  splitter can change under a labelled set: a label whose span this build no longer cuts as one
  claim is counted as unjoined and said, rather than scored against whatever sentence now sits
  at its old index. A reading names its claims the same way, so the two are joined by the bytes
  they cover and neither has to be told which rules cut it before it can be believed.

- **How often labellers find a claim genuinely contested varies more than the claims do.**
  28.9% overall, but from 13.5% on one game to **52.1%** on another. Some of that is the games,
  and some of it is labellers reading "two subjects both fit" more or less strictly. It is the
  clearest argument for reading the set twice: contested is the one field with no way to check
  itself.

- **"Every review" means every review Valve will serve, and the request parameters decide how
  many that is.** Two API defaults quietly remove a large and biased slice. `purchase_type`
  defaults to Steam purchases only, dropping activated keys, and `filter_offtopic_activity`
  defaults to excluding review bombs. The reviews those defaults leave out are not a random
  slice: a review bomb is negative by definition. A census that accepts the defaults
  understates exactly what it is trying to measure. This one always sets both.

- **The default pagination is worse than the defaults above.** Steam's `filter=all` is
  helpfulness-ranked and stalls almost immediately, returning a fraction of a percent of a
  corpus drawn entirely from the top of the pile before it starts repeating its cursor.
  Reading that would reproduce precisely the bias this tool exists to remove. The census
  paginates by creation date instead, and reports the coverage it achieved against Valve's own
  stated total as a figure you can read rather than a claim you have to trust.

- **Some reviews are genuinely unreachable, and a shortfall must never be blamed on that
  without checking.** Reviews from deleted and private accounts are counted in Valve's totals
  and cannot be retrieved by anyone. Steam separately, and intermittently, just stops serving
  a window early: pages arrive full, then one arrives empty long before the window is
  exhausted, which looks exactly like reaching the end. Walking the same window again returns
  everything. A crawler that accepts the first answer therefore undercounts a window at a time
  while reporting every shard as finished, so this one compares each window
  against Valve's count for it, walks it again when it lands short, and refuses to mark a
  window complete while it stays short. Coverage is reported per corpus either way, because
  the first number is the one worth doubting.

- **The target moves.** New reviews arrive constantly and old ones are edited, so a corpus is
  a snapshot with a timestamp, and it is brought up to date rather than rebuilt: one walk in
  last-edit order fetches everything written or changed since, and stops there. Nothing
  already captured is overwritten. An edited review is held in both forms, with a record of
  which one counts, so a capture never loses what a review used to say. Deleted reviews and
  drifting vote counts are what a sweep cannot see, and only a full re-crawl catches those.

- **A category with no labelled examples is only as good as its description.** Fitting cannot
  invent evidence. Where a game barely discusses something, that category stays weak for that
  game, and the fix is labelled examples from a game that does discuss it, not a cleverer
  scoring rule.

- **Counting words is not understanding them.** A category assignment is a useful summary and
  a starting point for reading, never a substitute for it. That is why drilling down to the
  underlying reviews is a first-class feature and not an afterthought.

## Models and dataset

The readers, the search models and the labelled claims are on Hugging Face, each release
tagged `v1`, `v2` and so on. The app fetches every file from one pinned commit and checks its
length and SHA-256 before using it.

| | Repository | DOI |
|---|---|---|
| Reader | [Aureliolo/game-review-reader](https://huggingface.co/Aureliolo/game-review-reader) | [10.57967/hf/10732](https://doi.org/10.57967/hf/10732) |
| Reader, small | [Aureliolo/game-review-reader-small](https://huggingface.co/Aureliolo/game-review-reader-small) | [10.57967/hf/10734](https://doi.org/10.57967/hf/10734) |
| Labelled claims | [Aureliolo/game-review-claims](https://huggingface.co/datasets/Aureliolo/game-review-claims) | [10.57967/hf/10735](https://doi.org/10.57967/hf/10735) |
| Search encoder | [Aureliolo/steamgauge-search-encoder](https://huggingface.co/Aureliolo/steamgauge-search-encoder) | |
| Search reranker | [Aureliolo/steamgauge-search-reranker](https://huggingface.co/Aureliolo/steamgauge-search-reranker) | |

The search models are fp16 ONNX exports of Qwen3-Embedding-0.6B and Qwen3-Reranker-0.6B with
their weights unchanged. The dataset holds no review text and nothing about the authors: each
labelled review carries a SHA-256 fingerprint of its text, and `training/fetch_text.py`
fetches the reviews from Steam and puts the text back on your own machine.

## Data and privacy

This repository holds no review data. Reviews belong to the people who wrote them and to
Valve, and they are downloadable by anyone with the app ID, so there is nothing to gain from
redistributing them here.

Nothing you download leaves your machine. The reading runs on a model on your own computer, so
a complete census needs no account and no key. Reports are the same: one file with no fonts,
scripts or stylesheets fetched from anywhere, so reading a result is not a way of publishing it.

The app makes requests to four places, and to nothing else:

- **Steam**, for the reviews of the games you add, their names and their totals, the store's
  search when you find a game by its name, and each game's store picture, fetched once and kept
  on your disk; and, when the app opens and while it stays open, how many reviews each game in the
  library has now: at most every six hours, one request per game, and Settings turns it off. The
  updates the app makes by itself fetch only what was written or edited since.
- **Hugging Face**, for the readers and the search models. Each file is fetched once, from a
  pinned commit, and checked against its pinned length and SHA-256 before it is used; the
  pipeline's `embed` command fetches its encoder the same way. When the library is checked
  against Steam, the app also asks Hugging Face whether newer releases of those models are
  published, and fetches nothing from them.
- **GitHub**, at most once a day, for whether a newer SteamGauge is out: one request to
  SteamGauge's release page, which names the newest version and nothing else. Settings turns
  it off. When you choose **Update now**, the app downloads that release's file for this
  computer and its build provenance from the release, and asks GitHub's attestations API for
  the provenance only where the release does not carry it.
- **Sigstore**, only during an update, for the current keys and certificates its signatures are
  checked against, from its public TUF repository.

A link in the window, to a review on Steam or to a release page, opens in your browser.

Steam review text, author IDs and profiles are public. A local corpus of millions of accounts
is still personal data, so it stays on your disk, and exporting a corpus offers
de-identification.

## Licence

Apache License 2.0. See [LICENSE](LICENSE).
