# Claim reader (intfloat/multilingual-e5-large)

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.737, macro F1 0.673
- Polarity macro F1 0.849
- Calibration error 0.170
- Below 0.64 confidence it says nothing, which leaves it answering 90% of claims at 0.772 accuracy, somewhere in [0.760, 0.783] over 5025 claims
- **What ships abstains per subject and per language**, not at that one line: 26 of 26 subjects carry a line of their own, 0.18 to 0.99. The coverage above is what this run measured itself at, under one threshold; `steamgauge measure-claims` over the frozen games is the figure for the rule that ships, and it answers less of them more often.
- **And per language**: 17 of 29 carry a line, 0.31 to 0.93. A claim answers only when it clears both its subject's line and its language's, because a line drawn per subject is drawn mostly from English claims and out of fold it leaves Korean 8.3 points under the promise it prints. 12 languages are declined outright, having too few labelled claims to promise anything: bulgarian, czech, danish, dutch, finnish, greek, hungarian, indonesian, norwegian, portuguese, romanian, vietnamese.
- Area under the risk-coverage curve 0.106 (lower is better; it says whether the model knows when it does not know)
- Trained on 27681 claims, validated on 4658, measured on 5579
- Data fingerprint `8a01195c332f2484`, code `4275955a43a8`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.751, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.25, `community` 0.46, `licensing` 0.46, `vr` 0.50, `genre` 0.59.

## Honest limits

The labels were produced by a language model working from a written category
sheet, not by human adjudication. That makes this a silver standard: agreement
with a model rather than correctness. Two models can agree and be wrong together,
most easily on sarcasm and on the boundaries between categories.

## Licence

Apache-2.0, as is the encoder it was fine-tuned from.
