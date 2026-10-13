# How it works

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
