# Game Review Reader

Run `e5small-pool-qwen4b-headset-s1`, fine-tuned from `intfloat/multilingual-e5-small`.

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.712, macro F1 0.615
- Polarity macro F1 0.794
- Calibration error 0.073
- Below 0.58 confidence it says nothing, which leaves it answering 78% of claims at 0.795 accuracy, somewhere in [0.782, 0.807] over 3973 claims
- **What ships abstains per subject and per language**, not at that one line: 26 of 26 subjects carry a line of their own, 0.16 to 0.89. The coverage above is what this run measured itself at, under one threshold; `steamgauge measure-claims` over the frozen games is the figure for the rule that ships, and it answers less of them more often.
- **And per language**: 19 of 29 carry a line, 0.30 to 0.73. A claim answers only when it clears both its subject's line and its language's, because a line drawn per subject is drawn mostly from English claims and out of fold it leaves Korean 8.3 points under the promise it prints. 10 languages are declined outright, having too few labelled claims to promise anything: bulgarian, danish, dutch, finnish, greek, indonesian, norwegian, portuguese, romanian, vietnamese.
- Area under the risk-coverage curve 0.116 (lower is better; it says whether the model knows when it does not know)
- Trained on 36038 claims, validated on 4310, measured on 5083
- Data fingerprint `1a14a58410437c6a`, code `499668c6bf6e`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.753, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.00, `community` 0.17, `vr` 0.36, `licensing` 0.53, `tutorial` 0.54.

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
