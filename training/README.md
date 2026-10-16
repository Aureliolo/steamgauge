# Training

The classifier is not written in Rust. It is trained here, in Python, and shipped as an ONNX
graph the Rust tool runs. Nothing in this directory is part of the build; everything in it
produces an artefact that is, and every artefact records what produced it.

## What it does

`steamgauge` labels claims, not reviews: the separate points a review makes. A model is trained to
read one claim and say which of the sheet's subjects it is about, whether it is praise,
complaint or neither, and how sure it is. Below a calibrated threshold it says nothing, which
is the whole reason for replacing what came before: the previous classifier compared a review
to twenty-four category prototypes and took the nearest, so a review reading "gfg" was filed
under Graphics and art.

## Setup

```sh
python -m venv .venv
.venv/Scripts/activate          # or .venv/bin/activate
pip install -r requirements.lock
```

`requirements.txt` holds the floors and ceilings; `requirements.lock` is what they resolve to,
compiled from the repository root by the command in its header, with the options in the root
`uv.toml`, and committed beside the runs it was used for. After changing `requirements.txt`,
re-run that command; to take every pin to its newest, add `--upgrade`. With uv rather than pip,
`uv pip sync training/requirements.lock` from the root does the install; the CUDA build of torch
lives on a second index, and `uv.toml` is what tells uv to look past the first one.

Training wants a GPU. It will run on a CPU and you will not enjoy it.

## The order things happen in

```sh
steamgauge export-training --to training/data/claims.jsonl   # labels plus their text, local only
python bakeoff.py                                        # which backbone, decided by measurement
python train.py --backbone intfloat/multilingual-e5-large --context --epochs 5 \
    --learning-rate 2e-5 --run-id <id> --save
python sweep.py --against win-128                        # every run ranked, with the noise beside it
python export.py --run runs/<id> --categories <fingerprint> --fp16   # ONNX, with a parity assertion
python publish.py --owner <you> --tag v<n> \
    --reviews <export-review-facts output>               # to Hugging Face, and pinned in the tool
```

`--context` is what ships: the model reads a claim inside a window of its review centred on it,
as the labeller did. `sweep.py` is not optional reading. It prints, beside every configuration,
how far apart two runs of one configuration land, which on this machine is about two points of
coverage even on the same seed. Almost every configuration ever tried here sits inside that;
`runs/README.md` lists each with what it changed and scored.

A student can learn from more than the labels. `steamgauge export-pool <training app ids>`
draws reviews no reference set holds, from games the model trains on only, and `teach.py
--runs <saved run ids>` reads them with an ensemble of saved runs and writes their averaged
distributions; `train.py --pool data/pool.jsonl --pool-targets data/pool-targets.npz` then
reads one pool batch beside every labelled batch and pays for disagreeing with the teacher as
it pays for disagreeing with a label. The pool never reaches a held-out or validation game, and
`train.py` drops any pool row from a game the run is scored on, so the held-out figure stays a
figure about games nobody trained on. `tapt.py --pool data/tapt.jsonl --run-id tapt-e5`
continues the encoder's masked-language pretraining on the same kind of pool before any
label is spent, and writes a backbone `train.py --backbone runs/tapt-e5/backbone` fine-tunes
from. `--ema`, `--llrd`, `--rdrop` and `--pooling last` are the other knobs, and the run index
says what each scored.

Two knobs aim at the abstention rule rather than at accuracy, which is what coverage at a
promised accuracy is made of. `--second-weight` charges the loss against both labellers'
answers wherever a set has been read twice, mixed by the weight given, instead of against one
of them at full confidence; the export carries the second reading beside the first. And
`--error-reg` charges the subject head for being surer of a wrong claim in the batch than of a
right one, which is the pairwise hinge of Xin et al. (ACL 2021). A run records both.

`soup.py runs/a runs/b runs/c --run-id <id> --save` averages the weights of runs that already
exist and reads the average, which is the cheapest answer there is to the seed spread: the
best of three seeds is the seed the validation draw flattered, and the mean of the three is
neither. It refuses ingredients that differ on the encoder, the subjects, the window or the
export, and allows ones that differ on the learning rate or the objective, which is the soup
worth making. `--greedy` adds them best first and keeps one only while the soup answers more.
The held-out games are read once, by the soup that was chosen on the validation games.

`claims.jsonl` holds review text and is never committed. What gets published is the model and a
label set of review ids, claim offsets and labels, built from `reference/claims/` rather than
from the training export, which anyone can rehydrate with `steamgauge` itself. Reviews belong to
the people who wrote them, and `publish.py` refuses a row that carries text.

Publishing pins the three model files by hash in `crates/steamgauge-core/src/reader.rs`, so the
tool fetches exactly what went up and refuses anything else. Rebuild and commit after it runs.
Until something is published the pin is empty, and an empty pin refuses to fetch rather than
fetching unverified.

The search models and the writer are Qwen's, exported rather than trained: `export_search.py`
and `uv run export_writer.py --to <model cache>` write them, the tool pins their hashes, and
`publish.py` puts up only the bytes it pins. `--only <repository>` publishes one repository
alone, as a new one goes up at `v1` while the rest stay at their tags.

What training fetches is pinned the same way. Every backbone is named in `backbones.py` with the
commit it is read at and whether its own code may run, every script loads through it, and a
name missing from it is refused rather than fetched; a directory (a run's tokenizer, a `tapt.py`
backbone) loads from disk. To try a new backbone, add its line first: the full commit
(`huggingface_hub.HfApi().model_info(name).sha`), and `remote_code=True` only if `config.json`
at that commit has an `auto_map`, with the commit of the repository that code lives in when it
is not the model's own.

## What is recorded

`runs/README.md` is the index: every run there has ever been, what encoder it used, how many
labels it had, which label set those were, and what it scored, with the shipped one marked. It
is generated by `python sweep.py --index` and never edited by hand, because a list somebody
maintains is wrong by the second experiment. Read it before opening any run directory.

Every run writes `runs/<id>/` holding the config, the git sha, a hash of the label set, which
games were trained on, validated on and held out, the metrics per subject and per language, the
whole risk-coverage curve, and the held-out games' figures at the threshold validation chose.
That directory is the only evidence a published figure has, so a run that did not write one
did not happen.

The held-out games are read once, at the end, and choose nothing. The bake-off does not read
them at all: a set used to choose between models cannot also say how the chosen one does.

The bake-off's runs are in the index under `bakeoff-`. It runs five epochs by default: at three
the order was the same but every candidate was still climbing, and a gap measured mid-climb is
a gap in learning speed as much as in the model.

## What holds

- **The recipe.** `multilingual-e5-large-instruct` is the standard reader's encoder and
  `multilingual-e5-small` the small one's. Both learn from the labels and from a teacher,
  `Qwen3-Embedding-4B` trained through rank-16 adapters, whose answers over the unlabelled pool
  are the distillation targets. Every encoder of the same size measured against it answered far
  fewer claims at the promised accuracy.
- **The lines.** The per-subject and per-language abstention lines are fitted on five folds of
  the games outside the held-out set, so every training claim is answered by a model that never
  saw its game. `export.py --lines-from` writes them into `reader.json`.
- **The bar.** A configuration is better only when it clears the seed spread on coverage at the
  promised accuracy, without losing a point of macro F1 or worsening AURC. Five seeds decide; four
  that look separated are not enough. A comparison run is six epochs, since every seed is within
  half a point of its last epoch by then.
- **What is quoted.** `test.at_validation_threshold`, the held-out games at the line the
  validation games chose. `test.threshold_coverage` refits the line on the held-out games and is
  never quoted.
- **The export is fresh.** `export-training` is run immediately before training, never found on
  disk. It holds back a label at a span this build cuts no claim at, a teaching row from a game
  that is not training, and a second copy of a claim, which becomes that claim's second answer.
- **transformers stays below 5** until the exporter's graph runs on DirectML; `requirements.txt`
  says why beside the pin.
- **Training shares the card.** `--card-share` rests after each batch and `--card-memory` holds
  the run to a size; `--cuda-graphs` replays the passes so the card does not wait on Python.
