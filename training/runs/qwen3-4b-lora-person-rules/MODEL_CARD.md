# Game Review Reader

Run `qwen3-4b-lora-person-rules`, fine-tuned from `Qwen/Qwen3-Embedding-4B`.

Reads one point from a Steam review and says which subject it is about, whether
it is praise or a complaint, and how sure it is. Below a calibrated threshold it
says nothing, and that is a supported answer rather than a failure.

## Measured

- Accuracy 0.778, macro F1 0.706
- Polarity macro F1 0.861
- Calibration error 0.126
- Below 0.41 confidence it says nothing, which leaves it answering 98% of claims at 0.787 accuracy, somewhere in [0.775, 0.798] over 4993 claims
- Area under the risk-coverage curve 0.073 (lower is better; it says whether the model knows when it does not know)
- Trained on 36017 claims, validated on 4310, measured on 5080
- Data fingerprint `ee81c9ae0b75fd1b`, code `6d55fc474a28`

**Every figure above is from the frozen games**, which the model never saw and
which chose nothing about it, not even the threshold. At that same threshold the
validation games report 0.751, which is
what it scores on games used to build it rather than what a new game gets.
Weakest subjects here: `accessibility` 0.31, `community` 0.35, `vr` 0.36, `licensing` 0.55, `genre` 0.60.

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
