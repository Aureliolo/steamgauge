# Game Review Reader

Run `e5inst-pool-person-rules-s1`, fine-tuned from `intfloat/multilingual-e5-large-instruct`.

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.781, macro F1 0.710
- Polarity macro F1 0.855
- Calibration error 0.074
- Below 0.37 confidence it says nothing, which leaves it answering 98% of claims at 0.789 accuracy, somewhere in [0.777, 0.800] over 4989 claims
- **What ships abstains per subject and per language**, not at that one line: 26 of 26 subjects carry a line of their own, 0.15 to 0.82. The coverage above is what this run measured itself at, under one threshold; `steamgauge measure-claims` over the frozen games is the figure for the rule that ships, and it answers less of them more often.
- **And per language**: 19 of 29 carry a line, 0.15 to 0.85. A claim answers only when it clears both its subject's line and its language's, because a line drawn per subject is drawn mostly from English claims and out of fold it leaves Korean 8.3 points under the promise it prints. 10 languages are declined outright, having too few labelled claims to promise anything: bulgarian, danish, dutch, finnish, greek, indonesian, norwegian, portuguese, romanian, vietnamese.
- Area under the risk-coverage curve 0.074 (lower is better; it says whether the model knows when it does not know)
- Trained on 36017 claims, validated on 4310, measured on 5080
- Data fingerprint `ee81c9ae0b75fd1b`, code `409ca2b52b64`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.750, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.33, `vr` 0.40, `community` 0.44, `genre` 0.60, `licensing` 0.64.

## The reader in each size

| Reader | Parameters | Download | Right, answering its surest 80% | Right, answering its surest 90% | Card memory | One big game on a card | One small game on the processor |
|---|---|---|---|---|---|---|---|
| small | 118M | 244 MB | 76.0% | 74.0% | 1.2 GB | 62 s | 36 s |
| standard | 559M | 1.1 GB | 85.2% | 81.3% | 2.6 GB | 125 s | 285 s |
| *4B* (not shipped) | 4.0B | 8.8 GB | 79.8% | 75.5% | 12.5 GB | 23 min | 49 min |

- **Right, answering its surest share:** the 458 claims of the frontier benchmark every reader can be handed, drawn from ten games none of them trained on and labelled under the current category sheet. Each reader answers only the claims it is surest of, the same share for every reader whatever its own abstention lines, scored on DirectML (`frontier.py reader`).
- **Card memory and one big game on a card:** one whole read of game 920210 (117,664 claims) through the desktop app's reading path on DirectML, with the card to itself, on an NVIDIA GeForce RTX 4090. Memory is the most the card held during the read, less what it held before. The 4B's are the first 4B's, which taught the sizes, and whose graph is the same shape as the one in its row.
- **One small game on the processor:** 1,416 claims of game 1888930 read on the processor alone (a build without a GPU backend), loading included, on an AMD Ryzen 9 5950X that was also running other work.

## Honest limits

The labels were produced by a language model working from a written category
sheet, so every figure above is agreement with a model rather than correctness.
One person adjudicated 200 of the frozen claims: the labels name the same
subject 65% of the time when the person reads cold and 89% once the sheet's
rule is in front of them. What this reader scores against the person is measured
once it is installed, by `steamgauge measure-claims --labels gold` over the frozen
games, and the README carries the figure for the reader that ships. Two models
can agree and be wrong together, most easily on sarcasm and on the boundaries
between categories.

## Licence

Apache-2.0, as is the encoder it was fine-tuned from.
