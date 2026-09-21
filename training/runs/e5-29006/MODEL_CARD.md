# Claim reader (intfloat/multilingual-e5-large)

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.737, macro F1 0.680
- Polarity macro F1 0.848
- Calibration error 0.172
- Below 0.60 confidence it says nothing, which leaves it answering 92% of claims at 0.771 accuracy, somewhere in [0.759, 0.782] over 4986 claims
- **What ships abstains per subject and per language**, not at that one line: 26 of 26 subjects carry a line of their own, 0.21 to 0.98. The coverage above is what this run measured itself at, under one threshold; `steamgauge measure-claims` over the frozen games is the figure for the rule that ships, and it answers less of them more often.
- **And per language**: 18 of 29 carry a line, 0.30 to 0.90. A claim answers only when it clears both its subject's line and its language's, because a line drawn per subject is drawn mostly from English claims and out of fold it leaves Korean 8.3 points under the promise it prints. 11 languages are declined outright, having too few labelled claims to promise anything: bulgarian, danish, dutch, finnish, greek, hungarian, indonesian, norwegian, portuguese, romanian, vietnamese.
- Area under the risk-coverage curve 0.101 (lower is better; it says whether the model knows when it does not know)
- Trained on 29006 claims, validated on 4744, measured on 5423
- Data fingerprint `2871969fd84a8278`, code `9d47548c876f`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.750, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.23, `licensing` 0.36, `vr` 0.53, `genre` 0.56, `policy` 0.61.

## Honest limits

The labels were produced by a language model working from a written category
sheet, not by human adjudication. That makes this a silver standard: agreement
with a model rather than correctness. Two models can agree and be wrong together,
most easily on sarcasm and on the boundaries between categories.

## Licence

Apache-2.0, as is the encoder it was fine-tuned from.
