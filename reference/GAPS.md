# Gaps the claim labellers reported

Collected as they finish, the same way each earlier sheet's gaps became the next one's.
Nothing here is acted on mid-run: changing the sheet while half a set is labelled leaves the
other half labelled against a taxonomy it never saw. These get applied in one revision, sheet
and splitter together, after the 36-game run is labelled and measured, and the sets are
redrawn against both.

Ordered by how many labellers reported the same thing without being able to see each other's
work, which is the only evidence any of it has. The second reading adds a different kind of
evidence: where two labellers given the same sheet disagree on the subject, the sheet is what
failed.

**Over 21,449 claims read twice, 2,452 of them split on the subject.** The commonest pairs are
`difficulty`/`gameplay` (198), `offtopic`/`verdict` (121), `updates`/`verdict` (120),
`atmosphere`/`verdict` (109), `content`/`gameplay` (107), `genre`/`verdict` (101),
`gameplay`/`verdict` (88) and `price`/`verdict` (81). Every one of those is below. Four of them
are directional rather than a coin toss, which is two readers drawing one line in two places
rather than a claim that fits both: on `atmosphere`/`verdict` the first reading says `verdict`
85% of the time, on `genre`/`verdict` 81%, and on `updates`/`verdict` it says `updates` 75%.
`steamgauge gold --boundary atmosphere/verdict` draws a page of one boundary at a time.

**Why this file is the lever and not the labelling.** Over the 24 subjects with enough claims
to judge, how often two readings agree about a row predicts the reader's own F1 on games it
never saw at Pearson +0.79; how many training labels the row holds predicts it at +0.29. A row
two readers split on three times in ten cannot be learned past seven in ten, so a draw aimed at
it buys the disagreement rather than the row. The rows that want labels are the ones that are
starved and precise; the rows that want a rule are here.

## Defects in the sheet, to fix in the next revision

### `verdict` contradicts itself about money: found in the run that added `atmosphere`

The `verdict` description offers "worth every penny" and "waste of money" as examples, and the
`verdict` RULE says "super fun, worth every penny" is `price`. A labeller cannot follow both.
`price` owns "value for money" by its own description, so the examples are the mistake: both
phrases judge value and belong to `price`. The rule's example is also poorly chosen, since
"super fun" is gameplay rather than price.

Not fixed mid-run, because changing the sheet while half a set is labelled leaves the other
half labelled against a different one. Labellers are marking these `ambiguous`, so the data
records the uncertainty rather than hiding it.

The revision, ready to apply in `taxonomy.rs` when the run ends: drop "worth every penny" and
"waste of money" from the `verdict` examples; change the RULE's example to "super fun, and
the story is great" belongs to `story`; and add "worth every penny", "waste of money" and
"refunded it" to the `price` examples, which is where the second reading found labellers
already putting them when they disagreed.

### `controls` and `accessibility` both claim key remapping: reported on 920210

`controls` lists "keybindings" and `accessibility` lists "remappable controls", and neither
has a RULE pointing at the other, so "I wish I could remap the keys" fits both descriptions
and the labeller flagged every one. The line the sheet already draws elsewhere is between
the thing and the option for it: how the keys are laid out and whether they respond is
`controls`; whether the game lets you change them is `accessibility`, which is where "the
settings players need in order to play at all" already points. The revision adds that
sentence as a RULE on `controls`.

**The rule went in and the description stayed, and the accessibility draw of 2026-09-24/25
measured what that costs.** Four labellers on four games with long accessibility menus put 206
claims under `controls` against 169 under `accessibility`, and all four stopped at remapping:
the description still says "keybindings", the rule says the option to change them is
`accessibility`, and each labeller settled it differently. Two of them, not seeing each other's
work, found the line the sheet should draw: **a setting a player needs in order to play at all
is `accessibility`** (remapping, subtitles, colour-blind modes, a toggle for a held button); **a
setting that is a preference is `controls`** (aim assist, aim acceleration, a deadzone, how many
options the menu holds); and **an option that exists but does not work is `bugs`** (a remap that
does not remap, reported twice). The fix is two edits: take "keybindings" out of `controls`'
description, and put the need-or-preference test in its rule. Two more edges from the same
draw: "accessibility" meaning approachable or available in a region (four claims, which the row
should warn off), and a missing difficulty or assist option (about ten claims on one game), for
which `accessibility`'s "offered for the same reason" gives no test against an ordinary
difficulty selector. **Written into the sheet 2026-09-25**: "keybindings" is gone from
`controls`' description, the need-or-preference test and the broken option are on both rows,
a mode for less challenge is `difficulty` and one that lets somebody finish is `accessibility`,
and the word alone is `tutorial` when it means easy to pick up and `policy` when it means sold
in a country.

**The revisit found the new test's own example wrong within two shares.** "I wish I could remap
the keys" was the example of a need, and states none; "every control can be re-mapped for
personal preference" then sat on both sides, and two labellers split five such claims. Need is
the reason the option exists, not the reason one reviewer wants it, so remapping is
`accessibility` outright now and the test is for every other setting. A story mode split the
same way, since the text never says who it was offered for: it is `difficulty` unless the
claim names somebody who could not otherwise finish. And a settings menu praised for its
breadth, "a lot of graphical and accessibility settings", was split in two shares between the
menu (`controls`) and the accommodation: it is the menu, unless the claim is about the
accommodations themselves. All three written in 2026-09-25, the same day.

Two more from the shares after it. "Names somebody" was read loosely: nine wishes for a
difficulty setting on one game said "most normal players" or "a wider audience", and the
labeller sent those to `accessibility`. A wider audience wanting it easier is `difficulty`; the
row takes a named disability or condition. And a remapping that exists and falls short, keys
reserved for a second player or a control that cannot be unbound, split between the remap rule
and `controls`' layout: it is still the remapping, and `accessibility`.

What the round left open, each for the next sheet round. **A remap named as the workaround**:
"you will need to remap the controls on keyboard" complains about the default layout and names
remapping as the fix; the labeller filed `controls`, which the claim is about, against the
letter of "whatever it is wanted for" (**written into the sheet 2026-09-25**: a remap named only
as the cure for the layout is the layout). **A settings menu that itemises its accommodations**
while praising its breadth takes both halves of the menu rule at once (four claims over three
shares). **An option wanted with no reason given**, a field-of-view slider or a motion-blur
toggle, sits between `graphics`' own toggles, the taste settings of `controls` and
`accessibility`. **A controller that half works on one pad** (a stick in four directions only,
triggers dead on a handheld) is `controls`' "whether they respond", `bugs` and
`compatibility`'s device at once. And a fan-made mode praised as "accessibility for people with
certain body disability" is `mods` by who made it and `accessibility` by what it is for. The
re-ask added two: "a lot of people are unable to pass certain levels" sits between a named
player and a wider audience, and a reviewer who could not finish without a story mode but whose
claim is that the game is too hard went to `difficulty` both times it came up; and a headset
game's comfort and locomotion options praised with the rest of its menu split between `vr`'s
comfort options and the menu.

### `accessibility` is the worst-defined row on the sheet: found by measuring, not reported

No labeller named this one, which is why it took measuring to find. `accessibility` holds 299
training labels, more than `audio` (284) and `vr` (273), and the reader scores 0.28 on it
against their 0.77 and 0.56, with precision and recall both about 0.28. Two blind readings of
the same claim reach the same subject 68% of the time, the lowest on the sheet, and where they
differ they go to `difficulty` and `gameplay`.

The description is "the settings players need in order to play at all", which reads as a
purpose rather than a test, and a purpose is the one thing a labeller cannot check a claim
against. "I could not finish it because the text is unreadable" is `accessibility` by purpose
and `graphics` by content; "no difficulty options for disabled players" is `accessibility` by
purpose and `difficulty` by content; "cannot rebind" is already split with `controls` by the
entry above. Every one of those goes to the aspect under the current sheet, which leaves
`accessibility` holding only claims that name a named accessibility feature, and that is the
row the description should describe. The revision needs the test written out: a claim is
`accessibility` when it is about a setting or feature provided so that somebody can play who
otherwise could not (subtitle size, colourblind modes, one-handed schemes, screen readers,
remapping), and belongs to the aspect when it is about the aspect being hard to see, hear or
beat.

**The test went on the sheet, and the revisit it owed is labelled (2026-09-22).** Every one of
the 369 claims filed under `accessibility`, across 75 sets, read again by three Fable
labellers against the new wording. 162 left the row and none came in: 55 to `graphics`, 31
`difficulty`, 21 `gameplay`, 19 `controls`, 12 `performance`, 7 `compatibility`, 6 `vr`. 207
stay. That is the row the entry above asked for, a named accommodation, and what left is the
aspect it was standing in for.

All three labellers, none able to see the others, hit the same five places the test does not
reach:

- **Motion sickness in a flat game, with no cause named**, about twenty claims over seven
  games ("gives me motion sickness", "3D酔いするゲームです"). The test rules `accessibility`
  out ("the accommodation, never the thing it accommodates"), `vr` owns sickness only in a
  headset, and nothing owns the nausea itself. All three filed `graphics`, low, flagged, which
  is a consistent guess and not a rule. The same holds for flashing that triggers epilepsy.
- **VR comfort options** (teleport, snap turning) are claimed by `vr` ("comfort options") and
  by the test (an option so somebody can play). Filed `vr`.
- **A difficulty or assist mode with no motive stated.** The test needs the option to be
  there so somebody can play who otherwise could not, and "a ton of difficulty levels" does not
  say. Filed `difficulty` unless the claim names who it is for.
- **Settings in general** ("the settings are comprehensive", a graphics-settings recipe for a
  playable frame rate, menu entries nobody explains): the row's title says "and options", its
  test excludes these, and nothing else takes them. Filed `controls`, `gameplay` or
  `performance`, flagged.
- **Display modes**: a missing fullscreen mode (five claims on 1888930), "runs great in window
  mode". Filed `compatibility` as the nearest, flagged.

Once each: subtitles that do not match the dialogue (`language`, low), and "IF YOU ARE
EPILEPTIC, DO NOT BUY THIS" as a conditional recommendation (`verdict`) that names the
animation (`graphics`).

### `policy` against `compatibility`: the 23 claims the second reading moved

`policy` is second-worst on both counts, 70% agreement and 0.60, on 737 labels. The second
reading sent 23 of its disagreements to `compatibility` and 12 to `updates`. The shape is an
account, a launcher or a connection the publisher requires: one reading files "needs a
Rockstar account" under what the publisher demands, the other under what stops the game
running. The sheet settles the neighbouring case already, on `bugs`: "Being unable to log in
is here when the login itself is broken and `policy` when needing an account at all is the
complaint." The same sentence has to be said about `compatibility`, which currently takes
anything that stops a game starting: a requirement the publisher chose is `policy` however
completely it stops the game, and `compatibility` is for the machine.

### `licensing` against `content`: five of its 65 claims read twice

`licensing` is the clearest case of the other kind, a row that is starved rather than
contested: 78% agreement, precision 0.70 and recall 0.38, which is a reader that finds little
and is right when it does. The disagreements go to `content`, which is the missing-competition
seam the entry further down already reports from the licensing draws. It is on this list to
say that its fix is labels and games, not a rule.

## Needs a rule, not a category

### Comparison with the predecessor: three reports, two eras

"Not as good as the first game", "FP1 was a class", "completely different from the original",
"best Anno yet". Reported by both claim labellers and by the review-level labellers before them.
It currently scatters between `genre` (comparison to other games), `verdict` (better or worse)
and `offtopic` (a statement purely about the older game). Labellers marked nearly all of it
contested, which is the definition of a boundary the taxonomy has not settled.

A one-line RULE on `genre` would absorb most of it: a comparison with the game's own
predecessor is `genre` when the difference itself is the point, and `verdict` when it is only
better or worse.

**Reported by all three labellers on 2338770**, a yearly sports title: "exact same game as
last year, copy-paste, a reskin" is the commonest complaint in that set and sits between
`genre` (the predecessor comparison), `content` (nothing new in it) and `updates` (nothing
changed). All three filed it `genre` and flagged it, and 2669320's declined draw raised it
again for another yearly football game. The rule above covers it once it says so:
a yearly release judged against last year's is the predecessor case, whatever the wording.
The same set raised "PC only gets the last-gen version", between `policy` (the publisher's
decision), `compatibility` (what this platform gets) and `updates`; two labellers went one
way and one the other. A decision about which platform gets which build is `policy`; how the
build runs on this machine is `compatibility`.

**The same seam with a rival rather than a predecessor**, reported on 1665460 ("better than
FIFA"), 1716740 ("worse than Freelancer", "like Fallout 4 without the exploration"),
228380 ("the better Flatout") and 1778820 ("SF6 is the superior product"). The
proposed RULE names only the game's own predecessor, so labellers split a comparison with
another studio's game between `genre` and `verdict` and flagged it. The rule should say
"another game" rather than "its predecessor": the difference named is `genre`, better or worse
alone is `verdict`.

### Praise or blame for the studio that is not about patches: five reports

"Applaud the devs for taking a risk", "hope Bandai sells the IP", "director replacement is
urgent", "fix this 11 bit", and on 2357570 and 2669320 a wall of bare insults at Blizzard and
EA. `updates` is written around post-release patching and `policy` around the publisher and
the platform, so a judgement of the studio itself lands in neither, and both labellers filed
it under `updates` and flagged every one. The revision gives `updates` a second sentence:
praise or blame aimed at the studio as a whole, with no patch, decision or term named, is
`updates`; a named decision is `policy`.

**Two more reports on the first aimed draw, and the revision made two rows claim it.** "Greed
machine" and "the shady way Bandai treat the community" on 1778820, "Shame on you Snail Games"
on 2399830. The sheet now says both that blame aimed at the studio itself is `updates` and
that a protest that is angry, brief and names no term is `policy`, and a bare insult at a
publisher is both. The two labellers split, one to each row, and each flagged it. Whichever
row keeps it, the other's sentence has to say it does not.

### A patch that removed content: one report, and it was most of that game

Skullgirls' 2023 censorship patch (245170) is the bulk of its set: "not what I paid for",
"content removed", "woke devs". `updates` (a patch changed the game) against `policy` ("terms
that changed after people had bought it"), and the labeller flagged every one that framed it
as the owner's decision. The RULE proposed above for the studio settles this the same way: a
patch is `updates`; the decision behind it, where the claim names it, is `policy`.

**Reported again on 245170's declined draw**, with a third reading the first report did not
have: where a removal names what went (an outfit, an armband, an announcer's voice), the
labeller filed it under the thing removed, `graphics` or `audio`, and generic removals under
`policy`. The rule needs a third clause: the complaint is about the removal, not about the art
or sound itself, so a named removal is `updates` or `policy` by the same test and never the
row of the thing that was taken out.

### Cannot log in: one report

"Can't log in", "needs a phone number to play", on 2357570. `bugs` when the login is broken,
`policy` when the requirement is the complaint, and the labeller could not always tell which
the reviewer meant. Related to the "will not start" gap below and settled by the same rule.

### Unfinished, not ready, should not have been released: two reports

"Alpha state", "wait until they fix it", "should never have left early access", on 1272080
and 3551340. `content` owns "feels finished", `updates` owns blame at the studio, and `verdict`
owns a judgement with nothing named, and the claim is all three at once. One labeller split it
by whether the sentence addressed the developers (`updates`) or described the game
(`verdict`); the other by whether it was about work still owed (`updates`) or about how thin
the game is (`content`). The rule that settles it: the state of the game as released, with no
missing thing named, is `verdict`; a named missing thing is `content`; anything addressed to
the developers or waiting on their work is `updates`.

### A premise list: one report

"Dwarves, beer, space, guns, bugs" on 548430, a string of nouns that sells the game rather
than judging any part of it. `genre` ("what kind of game") against `gameplay` (the mining and
the shooting) against `story` (the world). The labeller used `genre` for noun lists and
`gameplay` for verb loops, all at low confidence. The sheet should say that a list naming what
the game is about, with no judgement attached, is `genre`.

### How well the computer plays: two reports

Enemy AI that is too dumb or too sharp on 553850 and 597180's declined draws, and AI helpers
that path-find badly or get stuck on 1248130. `difficulty` ("how hard it is to beat"),
`gameplay` ("how it behaves") and, for the helpers, `bugs` ("a system malfunctioning"). The
labellers filed by effect: too easy or too hard is `difficulty`, a stuck helper is `bugs`. That
is the right cut and the sheet should say it: AI judged by how much challenge it gives is
`difficulty`, AI that visibly fails at what it is meant to do is `bugs`, and how it decides
what to do, described without either, is `gameplay`.

### A publisher's decision reversed, celebrated in memes: one report, a tenth of that game

Helldivers 2 (553850) after Sony withdrew the account requirement: "DEMOCRACY HAS
PREVAILED", "we won", "For Democracy". The words say nothing about the game and the context
says `policy`, praise; the labeller filed most `offtopic` at low confidence and marked them
ambiguous, which is the honest reading of a sheet that has no line for it. The revision adds
one to `policy`: a celebration or a protest of something the publisher or platform did is
`policy`, praise or complaint by which it was, however it is phrased. The same set held
anti-cheat complaints between `policy` (the kernel driver is the objection) and
`multiplayer` (it blocks joining), settled by the existing line: what a decision is is
`policy`, what it does when playing is where it does it.

### Luck that is sold: two reports

FC 25's pack luck (2669320) is `monetisation` (loot boxes) and `difficulty` (luck deciding the
outcome) at once, and "scripting" (the game decides you lose) is `difficulty` by the
fairness clause and reads as a `bugs` accusation. The labeller filed both under `difficulty`
and flagged them. A RULE on `monetisation`: what a purchase gives you is `monetisation`, how
it plays once you have it is whatever it is about.

The same game's aimed draw brought "matches are scripted" back to a second labeller, who could
not see the first, and who split it between `difficulty` and `gameplay` rather than `bugs`.
Three rows for one accusation. The sheet never names a rigged outcome; `difficulty`'s fairness
clause is the nearest, and saying so settles it.

### A balance change blamed on a patch: one report, and it dominated that game

Tekken 8's set (1778820) is mostly Season 2 complaints, where one sentence names a balance
problem (`difficulty`), the mechanic it broke (`gameplay`) and the patch that did it
(`updates`). The labeller filed the balance under `difficulty` and the blame under `updates`
and marked most of them contested. A RULE on `updates` would settle it: a complaint about what
a patch changed is about the change (`difficulty`, `gameplay`, `content`); `updates` is for
the patching itself, its pace, and whether the developers listen.

### The alien that cannot be fought, and the commonest disagreement between labellers

"You can't fight back", the alien's AI on 214490, parry windows, enemy spam and damage
sponges on 1809540: `gameplay` against `difficulty` every time, and the sheet's `difficulty`
rule ("how hard, and whether it is fair") does not say whether an enemy's behaviour is a
mechanic or a difficulty. One line would: an enemy's design is `gameplay`; how hard it is to
beat is `difficulty`.

This is now the largest gap the second reading finds: **25 of the 183 subject disagreements**
over 1,400 claims read twice, ahead of every other pair, and it appears in six games rather
than one. Skullgirls mixups, Nine Sols boss readability, Frostpunk 2 faction exploits and Old
World's AI bonuses are all the same shape: a mechanic described through how hard it makes the
game. The rule above settles each of them, and it is the first line the next sheet should carry.

### Faithful to the film: one report, a third of that game's contested claims

"Captures the feeling of the original film", "true to the movie", on 214490. Between
`licensing` (faithful to the thing it adapts), `atmosphere` (the feeling itself) and
`verdict`. The `licensing` description already says "how faithful the game is to the thing it
is adapting", so the sheet has an answer and the labeller did not trust it against
`atmosphere`. A RULE on `atmosphere` pointing at `licensing` for faithfulness would do.

**Reported again 2026-09-11**, independently, by the frontier model reading 471 frozen claims
for the baseline: it named this the single unclearest boundary on the sheet, because such
claims "usually lead with atmosphere words" and the sheet sends faithfulness to `licensing`.
Two readers who could not see each other's work naming the same boundary is the strongest
evidence in this file, and the rule is still not written.

**Third report, 2026-09-23, after the rule was written.** `atmosphere` now says faithfulness to
the film it adapts is `licensing`, and on 1433140, a film adaptation, the labeller still split
the one shape the rule does not name: "it makes you feel like you are in the movie" went to
`licensing` and "brilliant atmosphere, like you're in the movie" to `atmosphere`, both flagged.
The rule speaks of faithfulness; a feeling of being inside the film is the same claim said as a
feeling, and the rule should say so in those words.

The same draw found two edges of the row the sheet does not draw at all. A remaster "faithful
to the originals" (2395210, five claims) is not an adaptation of anything, and `genre` owns how
a game differs from its predecessor; filed `licensing`, flagged. And a licence withheld or
expiring ("NFL, open the licensing", "SI are not extending the licensing") is both the licence
lost (`licensing`) and the agreement protested (`policy`); the labeller filed who holds it as
`policy` and what went missing as `licensing`, which is a workable line and should be the
written one.

### A real competition that is missing: two reports, on the licensing draws

"No Liga MX", "no World Cup mode", "add the Czech league", "the national teams are gone", on
1665460 and 2669320, the sets drawn to teach the `licensing` row. The RULE says a real name
missing is `licensing`, and `content` says wanting more game is `content`; a missing league
is both, and each labeller drew the line in a different place: one at teams and leagues
against tournaments and modes, the other at "very realistic football" against anything named.
Both flagged nearly all of it. The rule that settles it: a named real-world team, league,
competition or player that is absent, wrong or generic is `licensing` however it is phrased;
`content` keeps only a mode or feature that names nothing real.

### A VR complaint whose cause is not the headset: reported by the frontier reader

Claims about stutter over wireless streaming, or about a mod that makes a flat game playable
in VR, sit between `vr`, `performance` and `mods` with nothing to choose between them. The
`vr` row is about playing in VR; it does not say whether a VR-specific performance problem is
`vr` or `performance`, and the same question will arise for every VR game added. With two VR
games now drawn (546560, 438100), this stops being hypothetical.

**Reported again by a claim labeller on 1716740**, from the other side: physics-driven hands
(an item misaligned in the grip, a crouch that does not register, a rifle stock that
jitters) sit between `controls` ("clunky controls"), `vr` ("the controllers in your hands")
and `gameplay`, and neither RULE settles which owns a hand interaction. The line to draw:
anything that exists only because the player's body is tracked is `vr`; the same complaint
about a gamepad or a mouse is `controls`.

### A claim that carries two subjects because it was cut wrong

Not a taxonomy gap but a splitter one, reported by the frontier reader as the largest single
source of its own low-confidence answers: checkbox-template rows, pros-and-cons lists, and
"great X, but Y" arrive as one claim carrying two subjects. It took the head of the claim
except where a "but" clause carried the conclusion. The `split_wrong` flag exists for exactly
this and is set on a sixth of the set; what is missing is a rule saying which half to label
when it happens, so that two labellers make the same choice.

### Cutscenes that cannot be skipped: one report

Filed under `gameplay` at low confidence on 1809540. It is a `controls` question by the
sheet's "how many clicks it takes to do anything", and nothing says so.

### A game that will not start, with no reason given: two reports

"Bought it, installed it, cannot run it". Splits between `bugs` and `compatibility` with
nothing to choose between them.

The sheet has since settled the no-reason case (`compatibility`), and 1592190 found the case
beside it: a game that will not launch on a Turkish-locale Windows. A reason is given, and it
is the machine's setup, which reads as `compatibility`; the labeller filed all three `bugs`,
flagged. "No reason given" should become "no reason, or a reason in the player's own setup".

### A voice in a given language: two reports, on the first aimed draw

"No Turkish commentator is a big lack" and "Korean commentary is the best ever" on 2669320;
characters who each speak their own language and understand one another on 1778820. `audio`
owns the voice acting and `language` owns whether a language exists, and a voice in one
language is both. The first labeller flagged every one; the second, with no row that fit,
filed them under `story` as a writing choice. A RULE on `language`: whether a language is
spoken, in commentary or dubbing, is `language`; how well the voice is performed is `audio`.

### Bans and moderation: four reports, from every community draw but one

"You get banned for anything", "banned for being mass reported", a false anti-cheat ban and its
appeal, "a single chat moderator abusing his role", "elitist chat moderation", forum and
Discord bans by the studio: on 471710, 1203220, 230410 and 381210. `policy` owns what the
publisher or platform requires of the player, `updates` owns blame aimed at the studio, and
`community` owns harassment by other players; none names the enforcement of the rules. The
labellers split it between `policy` and `community`, flagged throughout. **Written into the
sheet 2026-09-24:** how a game enforces its rules on its players (bans, reports, an anti-cheat
verdict and its appeal, and the moderators who apply them, including one who abuses the role)
is `policy`; what the players do to one another is `community`.

The revisit found the rule's edges at once. **An anti-cheat that fails** ("does nothing against
the cheaters", "good anti-cheat now which prevents the obvious cheating"; 2357570, 2338770,
2878980) is the decision and its effect at once: `policy` owns the verdict, `multiplayer` owns
whether cheaters ruin it, and the labellers split. The line that follows the rule's own logic:
what the anti-cheat is and does to an innocent player is `policy`; how many cheaters get
through is `multiplayer`. **"Ban" that is not discipline**: a card game's ban list (1449850) is
a balance change, and lobbies whose players ban certain cars (228380) are players, not the
publisher. The rule should say it means an action against a player. **An anti-cheat that stops
the game launching** (1361210, 553850) went to `policy` by the launcher rule, flagged, which
that rule already settles.

### A headset game called immersive, with nothing named: three reports, a dozen claims in one set

"One of the most immersive VR games out there", twelve times on 1012790 alone, and again on
555160 and 916840. The `vr` RULE says "a flat game called immersive is not this: immersion
belongs to whatever creates it", which settles the flat game and leaves the headset one with
nothing named unplaced. All three labellers filed it `atmosphere`, flagged. That is the right
answer by the sheet's own logic, and it should say so: immersion with nothing named behind it
is `atmosphere` in a headset game as in any other; `vr` takes it only when the headset itself
is named as the reason ("being able to lean over the table in VR"). **Written into the sheet
2026-09-24**, on `vr`.

Its edge, reported by two revisit labellers: **immersion with more than one thing named**.
"Beautiful immersive game", "immersive and lore rich", immersion credited to the environments,
the soundtrack and the threats together (548430, 916440, 68267013, 163864741). `atmosphere`
sends immersion to what creates it, and when several things are named no single one is left
to take it. The same shape as the comma list: the claim carries several subjects, and one is
picked. And **"an immersive dive into the wizarding world"** (990080, four claims) sits between
this rule and `licensing`'s "feeling as if you are inside the film, show or book the game
adapts"; the labeller took `licensing`, which the licensing rule settles.

### A VR controller is a headset and a controller: two reports

Quest controllers detected as Vive wands (eight claims on 916840), "it says it supports Valve
Index" (450540). `vr` owns which headsets a game supports, `controls` owns whether a controller
is supported, and a tracked VR controller is both. Both labellers filed `vr`, flagged, which
matches the line already drawn under "A VR complaint whose cause is not the headset": anything
that exists only because the player's body is tracked is `vr`. **Written into the sheet
2026-09-24:** `controls` owns whether a gamepad is supported, and the tracked controllers of a
headset belong to `vr` with the headset they come with.

Its edge, from the revisit: **how a game uses those controllers** is a design choice rather
than the hardware. "Jump is really annoying on Quest controllers" (629730) went to `controls`,
a physics-based control scheme (1592190, three claims) split. The line: whether the controllers
are detected, tracked and supported is `vr`; what the game binds to them and how its scheme
feels in the hand is `controls`, headset or not.

**Three more reports from the neighbour draw of 2026-09-24/25, five in all.** A scheme judged
on one headset's controllers (555160, 36452827 and 92266170; "intuitive for Quest headsets",
about ten claims on 450540), "no way to change controller layout" (916840), and a game's own
body model misplacing the tracked hands (hand offset, a body that turns with the head, gear
slots that grab the wrong thing; five claims on 1012790). The labeller of 450540 drew exactly
the line above without having seen it, and the others split. Five reports are enough to write
it into the `vr` rule, with the body model on the `controls` side: the headset tracks the hands
correctly and the game places them wrong. **Written into the sheet 2026-09-25**, on `vr` and
`controls`.

### "Community" meaning how many: two reports

"The community is pretty small", "deader than my grandma", "a big community", "3/10
Playerbase", on 381210 and 1203220. The sheet sends whether there are enough players online to
`multiplayer`, and reviewers say "community" for the headcount, so the word and the rule pull
apart. Both labellers followed the rule, flagged. The rule is right and should say it outright:
the size of the player base is `multiplayer` whatever word names it; `community` is what the
players are like. **Written into the sheet 2026-09-24**, on `community`.

## Might need a category

### Lost immersion, emotional distance: three reports, the commonest complaint in one set

"They are just numbers now", "lost its soul", "you cannot feel the people any more", "death is
a statistic". Sits between `story` and `gameplay` and the rules do not settle it. This was the
single most frequent complaint in the Frostpunk 2 pilot, so it is not a rare shape.

Related to the atmosphere and fear gap the review-level labellers reported six times across
two horror games. Both are about what a game makes a player feel rather than about any part
that produces it. One category might take both.

**`atmosphere` was added for this and is holding up.** Measured 2026-09-11 over
the 205 atmosphere claims in the frozen games, the reader scores F1 0.55 on it, which is the
middle of the table and the same as `gameplay` scores on twenty times the labels. It is not
one of the broken rows; those are all under about 150 labels. The claims it does miss go to
`audio`, `graphics` and `verdict`, which is the boundary this entry was always about, so
boundary text naming what atmosphere is not would still be worth writing. It is a refinement
rather than a rescue.

### Mods and user content: the most-reported gap there is

Five games in the review era (Blade and Sorcery, Beat Saber, Helldivers 2, Cyberpunk 2077,
SnowRunner) and now three more at claim level, on 1385380, 1465360 and 1592190, where one
labeller called modding "the game's dominant theme" and had to file it under `content` at low
confidence with every claim marked contested. Falls between `content`, `updates` and
`community` with no rule choosing.

The model agrees, without having been asked. On the labelled claims of the three frozen games
under the fifteen-game reader, 1466860 is the one it declines most of, 80% against 61% on the
least, and it is the one where modding is the dominant theme. A subject the sheet lacks is a
subject the model cannot commit to. Over the whole corpus the gap is softer, 72.9% against a
usual 73.2%, because the labelled sample is stratified towards subject-bearing claims and
shows it more sharply. Still two independent measurements pointing at one missing row.

Eight games is not a boundary dispute, it is a subject. The revision adds `mods`: "Mods and
user-made content. Whether the game supports them, how easy they are to install, what the
community has made, and whether the game is worth playing without them. A game that is
better modded than shipped is a statement about mods, not about content." With a RULE on
`content` sending anything user-made here.

### Playing with friends: three reports, and it dominated two games

"Better with friends", "fun with mates", "does not grab you alone". Not servers or matchmaking
(`multiplayer`), not calling it a co-op game (`genre`), and not a bare verdict. It was the
commonest qualified claim in DEVOUR's set, BeamNG's labeller (228380) hit the same three
ways of filing it, and Deep Rock Galactic's (548430) split it between `multiplayer` when the
point is solo against co-op and `verdict` when the friends are incidental. "The game is dead"
is its cousin, between `multiplayer` (nobody online) and `updates` (nobody developing), and
the same labeller marked it contested.

### A community's salute on its own: two reports, forty claims and more

"Rock and Stone!", about forty times in Deep Rock Galactic's set, sometimes as the whole
review. It works as an endorsement and passes no judgement in its words, so the labeller
filed it `offtopic`, neutral, and marked every one ambiguous. A reader who knows the game
reads it as praise; the sheet says `offtopic` is for what a reader learns nothing from. The
revision decides it in one line under `verdict`: a community's own catchphrase used as a
salute is a verdict, and it is praise. Twenty games have one ("Praise the Sun", "For Rock and
Stone", "Ave Nex Alea"), and `offtopic` is the wrong bucket for a reviewer declaring
themselves a fan.

**Reported again on 553850's declined draw**, with a wrinkle the first report did not have:
"Managed Democracy has been saved", "KILL THE CLANKERS", in-universe roleplay written while the
community was protesting the account requirement. The labeller split them between `verdict`,
`offtopic` and `policy`. The rule above settles the salute; what it has to add is that the
salute stays `verdict` even when the review around it is a protest, because the label is about
the claim, and a protest is `policy` only where the claim itself names the decision.

### Destruction and physics: one report, and it is what that game is

BeamNG's damage deformation is a system, and reviewers praise how it looks. Sits on the
`gameplay` and `graphics` line, filed `gameplay` and marked contested nearly every time. A
sheet row will not fix a game whose whole point is one mechanic; this is what the induced
subjects are for, and 228380 is a good second game to run the induction on.

## Confirmed by the run that added `atmosphere`

`atmosphere` works. On the first horror game labelled against it, it took 40 of 118
aspect-naming claims, nearly every "scary" and "I peed myself" among them. Before it existed
those fell to `verdict` or were scattered across `graphics` and `audio`.

### A verdict that names the genre as a noun: three reports, and the labels measured

"Excellent city builder", "among the best platformers", "great platformer in every way". The
`verdict` rule says any named aspect wins and `genre` owns naming what kind of game it is, so
these are forced to `genre` while reading as bare verdicts. Labellers flagged nearly all of
them contested. The sheet should say whether a genre noun counts as naming an aspect.

Measured on 2026-09-12 across the 18,907 labels then exported: the sheet already says a
judgement with only the kind of game attached is a `verdict`, and of the 152 claims that name
a kind of game *and* carry a judgement word, labellers wrote `genre` 57 times and `verdict`
33. Two to one against the rule as written. The reader learns the labels and repeats it:
`verdict` and `genre` are the second largest confusion in the whole out-of-fold matrix, 248
claims over twenty-eight games. Stating the principle was not enough, so the revision wants
the counter-example beside it: "a great roguelike" is `verdict`, "it is a roguelike" is
`genre`, and the test is whether removing the judgement leaves a claim that still says
something.

### How much story there is: five subjects for one question

Of the 624 labelled claims that contain a story word, 35 also say something about length, and
those went to `content` (7), `story` (5), `gameplay` (4), `difficulty` (4) and `verdict` (3).
No majority. The rule exists and lives in the wrong entry: `gameplay` says "how much game
there is belongs to content", and the `story` entry says nothing about amount at all. A rule
under one category is applied by labellers reading that category's paragraph and not
otherwise. The reader keys on the word: out of fold, `story` predictions are right 58% of the
time and 95 of its mistakes were labelled `gameplay`, among them "the main story seemed
fairly short" and "you're going down a predetermined path".

The revision: the `story` boundary gains "how much story there is belongs to content, the same
way how much game there is does; this is what the narrative is and whether it is worth
following".

### Balance complaints that name a mechanic: the rule is followed half the time

The `difficulty` boundary is explicit that balance complaints belong there rather than to
`gameplay`, *including when they name a specific mechanic as overpowered or useless*. Of the
97 labelled claims that say something is overpowered, nerfed, buffed or unbalanced, 50 went
to `difficulty` and 18 to `gameplay`. This is the largest disagreement between the two
labellers (25 of 1,400 claims read twice) and the largest confusion the reader has (357 claims
out of fold, both directions). The sheet already says it, so no edit; this is the boundary
the adjudication settles, and the 192 split claims in front of the adjudicator hold it by
name.

**Reported by all four labellers on 1449850**, a card game, without seeing each other's
work: "I sit and watch the opponent combo for ten minutes and cannot do anything" is the
commonest complaint in that set and each labeller split it differently between `gameplay`
(the turn structure that allows it), `difficulty` (going first wins, an unstoppable board)
and `multiplayer` (what playing against people is like). The sheet's balance rule covers the
second reading; the first and third are the same claim seen from the mechanic and from the
opponent, and a rule that says a complaint about what other players are allowed to do is
`difficulty` when it is about winning and `multiplayer` when it is about the experience of
the match would cut it. Every card and fighting game will raise it.

**Luck deciding the outcome is the same seam again, five reports**: shooting RNG on 2878980,
blueprint and loot RNG deciding a run on 1336490, "a guessing game, a 50-50" on the fighting
game 1778820, and "scripting, the game decides who wins" on both football games, 1665460 and
2669320. Each labeller split it between `difficulty` (the outcome is not down
to skill) and `gameplay` (the mechanic that produces it). The balance rule already sends
"unfair" to `difficulty`; it should say in as many words that a complaint that luck decides
who wins is `difficulty`, and a description of how the random system works is `gameplay`.

### Addictive, could not stop playing: one report, a dozen claims

Filed under `atmosphere` by its "pulls you in, lose whole evenings" example, but it reads as a
verdict to many. A single line would settle it either way.

### Pure emotional reaction with nothing named: two reports

"Made me cry", "the feels", "I'm scared", "AHHH". Sits between `atmosphere` and `story`, and
the polarity is about the reviewer rather than the game, which the sheet says polarity is not.

### Sickness with no screen or headset named: three reports, the day the rule was written

The rule sends motion sickness on a screen to `graphics` and in a headset to `vr`, and three of
the four labellers of its revisit met the claim that says neither: "motion sickness
simulator" as a whole review, "gives me motion sickness" in a review that never mentions how
it was played. The sheet forbids labelling from more than the text, so the fact the rule turns
on is not there to read. Two filed `graphics`, one `vr`, all flagged. Two readings of the fix:
default to `graphics` unless a headset is named, since most games are played on a screen; or
give the claim to whatever the review blames (the camera, the controls, "motion blur and film
grain"), which a labeller also reported. The first is a rule a labeller can follow every time.
Two more reports from the VR neighbour draw ("好晕哦" on 555160, "very little motion sickness"
on 450540), where the game is a headset game and both went to `vr`: the default should be the
platform the game is played on, which the store page says and the claim does not. **Written into
the sheet 2026-09-25**, on `graphics`, in the form a labeller can use without the store page:
the review around the claim decides, and one that never mentions a headset is about a screen.

Beside it, once each: settings nobody explains ("don't know what 3-4 of the settings do")
between `controls` and `tutorial`; VR comfort options between `vr` and `accessibility`, a
second report of that entry above; and advice or a wish about who should hold a licence
("hope some other studio picks up the license") between `policy` and `updates`.

### A headset that stopped working after an update: three reports from two labellers

"After the most recent update the game will not open with WMR headsets" (555160), "no longer
playable on Quest" (916840, 160610529 and 199033877), supported through Steam at release and
now only through the Oculus store (916840). `bugs` owns what "worked and now does not", `vr`
owns whether a given headset is supported, and a removal by decision is `policy`. Both
labellers filed `vr`, flagged. The line the other rows already draw would give it to `bugs`
when the text calls it broken and to `policy` when it calls it withdrawn, and to `vr` only when
it says which headsets work without saying anything changed. **Written into the sheet
2026-09-25**, on `vr` and `bugs`.

### A recommendation conditioned on a headset: four reports

"Do not buy if you own an Oculus headset" (555160), "if you have a Quest 2, skip it" (450540),
"unless you have a Quest 2, then I recommend the Quest 2 version" (916840), "not for beginning
VR players" (1012790). `verdict`'s rule gives every recommendation to `verdict`; the reason the
reader wants is the headset. The labellers split between `verdict` and `vr`. The same shape
as a recommendation with a reason anywhere on the sheet, and it wants the same answer: the
reason, when one is named, is the subject. Three more in the accessibility draws ("if you
struggle with motor skills, do not get this game"). **Written into the sheet 2026-09-25**, on
`verdict`: a condition about the reader's taste keeps the recommendation a verdict, and one
about their hardware or body sends it to that fact's row, since it says whether the game will
work for them.

### "An actual game, not a tech demo": one report, four claims

On 1012790 (104282741, 127689974, 173865089, 161828427). Whether a headset title is a real game
or a showcase is an axis `content`'s "finished or thin" comes close to and `verdict` does not
name. Filed `content`.

## Smaller, one report each

- Pre-order regret ("I who pre-ordered deluxe am a clown") fits neither `price` nor
  `monetisation` squarely.
- "I refunded it" is listed under `price` but reads as a verdict.
- Complaints about world plausibility ("how does XIX-century tech forecast a storm 88 weeks
  out") fit neither `story` nor `gameplay`.
- A reviewer's own hardware ("using an RTX 3070") sits between `compatibility` and
  `performance`. Reported again on 2399830 as a bare spec sheet ("Sound card: HyperX"),
  which says nothing about the game and went to `offtopic`, flagged. And twice in the headset
  revisit as a disclaimer ("I'm using Oculus Rift w/ Touch Controllers", "Quest 3
  controllers"), which went to `vr`, neutral, low: in a headset game the setup says which
  headset it was tried on, which the row owns. Four reports now, two answers. **Written into
  the sheet 2026-09-25**, on `compatibility`: the machine named with no judgement is there and
  neutral, a headset `vr`.
- "Fix the accessibility", "fix support for Quest 2 controllers please": `updates` owns telling
  the developers to fix something, and the thing named owns a claim that names one. Both went
  to the thing named. **Written into the sheet 2026-09-25**, on `updates`.
- A real injury from room-scale play ("broke my irl hand slapping an npc"): `vr`, low.
- Neutral narrative lines inside a long bug report have nowhere to go but `bugs`.
- Wishing a licence away ("pray FIFA gives the licensing to another studio", 2669320) fits
  `licensing`, `policy` and `updates` at once.
- A bare "toxic game" (2669320): one word that judges is `verdict`, but the word belongs to
  `community`; nothing says whether a row's own adjective makes it that row.
- "The devs are over-relying on the modding community" (1592190): `mods` owns whether the
  developers support modding, `updates` owns blame at the studio.
- "$40 for a six-hour campaign" (1592190): `price` and `content` named in one breath, and
  neither rule ranks them.
- A player's avatar joked about ("the anime girls have physics", 1592190): the joke rule sends
  it to what it jokes about without saying whether that is the model (`graphics`) or the
  physics (`gameplay`).
- A character creator the developers built (1778820) has no row; `mods` excludes it by being
  player-made.
- A training mode (1778820) sits between `tutorial` and `gameplay`.
- A font too small in one language only (2399830): the sheet sends small text to `graphics`
  and how a language reads to `language`.
- A request for a feature ("would love VR support", eight claims on 2399830) has no polarity
  guidance; the labeller used `neutral` throughout.
- A studio's television show that streamers rejected (2399830): `offtopic` owns a protest
  about something the publisher did elsewhere, `policy` keeps everything but no connection at
  all.
- Other games' maps playable in this one ("MW2 maps in VR", five claims on 555160): `genre`'s
  bare list or `mods`' "anything made by players", and the review never says who made them.
- One game mode praised ("TTT in VR is amazing", 555160): `verdict` sends a named thing to that
  thing, and no row owns a game mode; filed `gameplay`.
- A standalone-headset port, awaited or bought through another region's store (1012790):
  `compatibility` owns asking for a port, `vr` owns which headsets it works with.
- Crossplay wanted "so I can play with my friends" (1203220): neither `compatibility`'s console
  request nor `multiplayer` names it.
- Shared face presets from other players (1203220): `mods` or the customisation system under
  `gameplay`.
- A player-made revival of a shut-down game (471710): `mods` never contemplates one.
- "The most toxic relationship I've ever been in" (381210, three claims): a love-hate joke that
  implies compulsion without stating it, between `atmosphere` and `verdict`.
- Seated play (450540): `vr` owns it, `accessibility` owns a setting so somebody can play who
  otherwise could not; filed `vr`.
- Launching through the Oculus runtime rather than SteamVR for smoothness (1012790, five
  claims): `performance` or `vr` for a choice of runtime; split by what the sentence leads with.
- Teleport and arm-swing locomotion (450540, six claims): `gameplay`'s movement or `vr`; filed
  `gameplay`.
- A bare "crouching" in a headset game (916840): `vr` only when the player crouches physically.
- Pride flags and a missing UN flag in the scenery (1817070, four claims): no row owns
  depicted political content; filed `graphics`, low.
- Motion blur that stays on after being turned off (1817070): `graphics`, the option under
  `accessibility`, or `bugs` for an option that does not work; filed `graphics`.
- Subtitles that vanish in some cutscenes (2215430, five claims): `bugs`, `language` or the
  subtitles under `accessibility`; filed `bugs`.
- A PSN sign-in fixed or broken (2215430): `bugs`' broken login and `policy`'s launcher that
  will not sign you in point opposite ways for one event.


## Splitter, not taxonomy

Both labellers flagged these through `split_wrong`, and they are fixed: bullet markers,
numbered list markers, headings ending in a colon, semicolons, quotations, parenthetical
asides, Steam markup, and web addresses.

### The comma list, reported by six labellers and counting

"Stunning visual, calm music, epic story". "Great story and the sound design is top notch".
"Music 9/10 Buildings 9/10". "Runs well, isn't misrepresented, just not for me". Three subjects
in one claim, so the labeller picks one and marks it contested, and two subjects go uncounted.

The sixth, on 2669320's aimed draw, found about thirty in two hundred claims ("Trash Server,
Bugs, Toxic Players") and asked the question the sheet does not answer: which of the stacked
points gets the one subject. They took the first or the loudest.

Three more on the community draws (471710, 230410, 381210, about thirty claims in the last):
"Crashes all the time and has a horrible community", "story, music, community, and it's free".
The comma split below takes three short parts; these are two long ones joined by "and", or a
list whose last part carries a clause, and the splitter leaves them whole.

This looks fixable after all, and precisely: split on a comma only when the sentence is three
or more comma-separated parts that are each *short*, measured in the same weight the splitter
already uses so it works in scripts without spaces. "Stunning visual, calm music, epic story"
is three parts of weight 14, 10 and 10 and splits; "The combat, which took a while to click, is
superb" has a long middle and does not.

**Done**, mid-run after all, because a label names a span of its review rather than an index,
and `steamgauge measure-claims` joins by span: a labelled claim this build cuts differently is
counted as unjoined rather than silently scored against the wrong sentence. Three or more comma-separated parts, each of weight 6 to 18, split; the cut
happens after the fragments are joined, or the short parts would be joined straight back.
Readings record the splitter that cut them, and a reading cut by an older one is refused
wherever a claim is quoted or scored by its index, until the game is read again.

### A question and its answer are one claim, reported on 1601580

The worst-split game so far, 165 of 697. Two shapes the splitter has never seen. Rhetorical
question-and-answer pairs: "Is this Frost Punk?" / "No. Is this a phenomenal game?" and "Want
to see your objectives?" / "Top left of the screen." And the inline scorecard: "Visuals?
fantastic!" / ", Atmosphere?" / "fantastic!, Story?" / "mid at best, Balance?", one review
shredded into fragments that mean nothing apart.

The rule: a question mark followed by a short answer, a few words ending in a full stop or an
exclamation mark, is one claim with the question, not two. The scorecard is the comma list
again with a question mark in it, and the same short-parts rule that fixes the comma list
covers it once "?" is allowed to end a part.

**Done** for the question and its answer: a piece of weight 30 or under after
a piece ending in "?" joins it. The scorecard is not: "?" still ends a piece, so "Visuals?
fantastic!, Atmosphere? fantastic!" comes out as one claim per question through the
question-and-answer rule, which is the right count, and the tail "Story? mid at best,
Balance? bad" stays one claim of two.

### Abbreviations the list does not have, reported on 1466860

"Since I've been playing the Def. Editions for a while" cut at "Def.". A capitalised word
after a full stop that follows a three-letter capitalised token is a weaker signal than the
list, but "Def." is not in ABBREVIATIONS and "Ed." is not either. Add both, and consider
treating a one-to-four-letter capitalised token before a full stop, followed by a lowercase
word, as an abbreviation regardless of the list. To do with the comma list.

**Done** as list entries: "def", "ed", "ver", "vers", "esp", "resp", "orig"
and "hrs", and the list is now matched with the dots taken out, so "i.e." and "z.B." are
caught when a capital follows them (the 3551340 labeller found two "i.e." cuts). The
capitalised-token rule is not in, because a lowercase word after "Fun." is how most
reviewers start their next sentence. The same labeller reported "the old Champ." cut before
"Manager", which "champ" would cover if it were an abbreviation anybody else used; it is not
added on one report.

### A heading over a numbered list keeps the marker and loses the point, found by the adjudicator

`[h1] The Good [/h1]\n1. Best graphics in any lego gamer ever.` came back as `The Good \n1.`
and `Best graphics in any lego gamer ever.`: a title wearing a list marker and saying nothing,
and the point it introduces standing alone with nothing to say which half of the review it
belongs to. Two labellers split on it `offtopic` against the subject the heading names, which
is what an unanswerable fragment always does, and it rose to the front of the adjudication
queue on that strength.

The tag is why the plain case worked and this one did not. A heading tag used mid-line is
styling rather than a heading, correctly, so the title and the list ran on as one piece and
the stop after the marker cut it in the only place it could.

**Done**, in the fragment joiner rather than in the terminator rules, because the marker can
be stranded by any of them: a piece whose last line is nothing but a list marker introduces the
next one, exactly as a piece ending in a colon does. Measured over eleven captures before the
fix, 3,500 of 2,353,549 claims ended that way, 0.149%, worst game 0.237%, and it crosses
scripts: `Рецепт отличной игры:\n1.`, `...Что понравилось\n1)`, `勉勉强强\n1.`. On 920210 it
absorbed 237 stranded markers.

### A heading tag used as bold, reported on 1466860 and 1716740

"The biggest improvement might be [h1]EVERYONE WALK FAST[/h1], really nice" came back as
three claims, the middle one shouting and the other two saying nothing. The Chinese
long-form review on 1716740 does it eleven times: `[h2]` opened after a colon or a comma and
closed before one, so the labeller received claims beginning with "，" and "？". The
splitter treats every heading tag as a break because on Steam a heading is a block, but a
reviewer who opens one mid-sentence wanted emphasis, and the text runs straight through it.

The rule: a heading tag breaks only at a line boundary. An opening tag with nothing but
whitespace between it and the previous line break, or the start of the review, begins a
point; a closing tag with nothing but whitespace between it and the next line break, or the
end, ends one. Anywhere else the tag is styling and is skipped like `[b]`. Every real
heading in the same review still splits, since they all sit on lines of their own. Goes in
the splitter.

**Done**, decided at the opening tag: it is a heading where nothing but markup
has been written since the line began or the last point ended, or where the previous
sentence has ended; otherwise it and its closing tag are styling. A heading then ends with
its closing tag wherever that sits, and weighs what its words weigh, so "Cons" joins the
point under it exactly as "Cons" on a bare line does. In the drawn sets 147 of 150 closing
heading tags are followed by a line break, so the line rule and this one agree everywhere
but in the synthetic cases.

### A line that ends on a comma has not finished, reported on 1809540

A Japanese review of the economy (184812704) came back as thirteen claims, four of them
ending on "、" and the next line starting mid-thought: the writer wrapped long sentences by
hand and never used "。" at all. A line break ends a claim, which is right for lists and
headings and wrong here. The rule: a line ending in a comma (",", "，" or "、") joins the line
after it. A comma is never how anybody ends a point, in any script the splitter handles.
To do with the next splitter change.

**Done** as written.

### An emoticon after a sentence joins the wrong neighbour

":D", ":(", ";)" and ":|" sit after the sentence they colour, but a fragment below the
minimum weight joins forward, so "Great fun. :D If you like Vermintide..." hands the smile to
the next claim. Five claims across the drawn sets begin this way. A fragment with no letter
in it, on a line that has one before it, should join backward. To do with the next splitter change.

**Done** in both shapes: a token of up to four characters with at most one
letter in it, and at least one that is not a letter, following a sentence end on the same
line is run into that sentence's boundary, and a piece that is only such a token joins the
piece before it. "xD" is the one two-letter emoticon named. A bare letter never counts,
because "A" after a full stop is the next sentence starting. Not covered: an emoji that
opens a line and is followed by words on it (548430 reported a pickaxe and a heart handed
to the line after), because on the next line it is as often that line's bullet.

### A bold heading before a list stood alone, reported on 548430

"The Good:" and "Pro Tips:" came back as claims of their own with their items after them,
though a heading ending in a colon has joined what it introduces since the second set of rules. The
reviews wrote them as `[b][u]The Good[/u][/b]:` followed by `[list]` on a line of its own,
and that bare `[list]` line was a piece: it weighed six characters of markup, took the held
heading, and the two together weighed enough to stand. **Done**: a run with no
words in it is never a piece, markup weighs nothing, and a heading is read for its colon
with the tags taken out.

### Drawn art cut one line per claim, reported on 916440 and 629730

A fifteen-line braille drawing, a nine-line ASCII hand, a zalgo block: one picture, cut into
a claim per line, each of which is not words at all. Recognisable by what a line is made of
rather than by what it says: a line with no letter or digit in it, in a run of such lines, is
part of a picture. Three or more in a row are one claim, which the reader will decline and
which is the right answer for a drawing.

**Done** with the box template, with one addition and one limit. A drawn line
has to be more than one character, or a review that wraps on ":" would read as a drawing.
And a picture is only its own claim where the review is nothing else: a drawing between two
sentences is absorbed into the point around it rather than standing alone, because the
alternative is a claim of pure punctuation in the middle of a review that has words.

### Two shapes that may not be fixable mechanically

- A crash log or a poem cut line by line into fragments that say nothing alone.
- One sentence carrying three subjects ("beautiful art and story", "runs well, isn't
  misrepresented, just not for me"). The comma-list rule takes the ones written as a
  list; a sentence that names three things in ordinary grammar keeps one subject and the model
  learns the rest from context.

### The ballot-box template, reported by four labellers in a row

The copypasta review: `{ Graphics }` and then a column of options, one ticked and the rest
left blank. One on 690790 came back as **58 claims**, one on 774361 as 70, one on 916440 as
65, one on 629730 as 22, and every labeller flagged nearly every fragment. Across every drawn
set it is 8 reviews of 5,760, one in seven hundred, but those eight hold 443 claims, and 346
of them are a box line: **1.8% of every drawn claim** in the whole reference set is a tick
box. It is also what pushed 629730 to a 40.7% mis-split rate, the worst of any game, and
916440 to 29%.

It is mechanically recognisable, which the other noise shapes are not. A line whose first
character is a ballot box (`☐` U+2610, `☑` U+2611, `☒` U+2612) is a template option, and the
rule follows from what the reviewer did:

- An **unticked** option is a line they did not choose. It says nothing and is not a claim.
- A **ticked** option is the answer to the heading above it, so "{ Graphics } ☑ Beautiful" is
  one claim about graphics rather than a heading and a word.
- A run of them under one heading is one claim, not one per box.

**Done**, and the set of marks is wider than the three the reports showed.
`▢`, `✓` and `✔` are the same template in a different font, and a bare "x" or "X" followed by
a space is how somebody without any of those fonts answers it, so all of them tick. The blank
boxes are what identifies the template, since nobody types one of those by accident, which
is why an "x" counts only inside a review that has three blank boxes in it.

### A review with no punctuation at all, reported on 553850 and 3551340

"屎 闪退bug 各种奇怪bug 游戏内容太少 难度高无脑堆大怪", four complaints separated by spaces and
nothing else, and the same shape in Turkish, Polish and Czech, plus three English rants of two
hundred words with one full stop between them. The splitter has nothing to cut on: no
terminator, no comma, no line break. Splitting on spaces would shatter every ordinary
sentence, and splitting long runs at a word count would cut mid-thought at a place decided by
arithmetic. This is the shape a splitter cannot reach, and the labellers who reported it
labelled the whole run under its first subject, which is the honest reading.
