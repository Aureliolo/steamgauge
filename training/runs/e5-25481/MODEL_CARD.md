# Claim reader (intfloat/multilingual-e5-large)

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.740, macro F1 0.678
- Polarity macro F1 0.854
- Calibration error 0.168
- Below 0.68 confidence it says nothing, which leaves it answering 88% of claims at 0.785 accuracy, somewhere in [0.773, 0.796] over 4920 claims
- Area under the risk-coverage curve 0.104 (lower is better; it says whether the model knows when it does not know)
- Trained on 25481 claims, validated on 4658, measured on 5579
- Data fingerprint `431de257d7806513`, code `15965be91439`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.753, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.29, `licensing` 0.35, `community` 0.46, `vr` 0.53, `genre` 0.61.

## Honest limits

The labels were produced by a language model working from a written category
sheet, not by human adjudication. That makes this a silver standard: agreement
with a model rather than correctness. Two models can agree and be wrong together,
most easily on sarcasm and on the boundaries between categories.

## Licence

Apache-2.0, as is the encoder it was fine-tuned from.
