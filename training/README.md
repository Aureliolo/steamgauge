# Training

The reader is trained here, in Python, and shipped as an ONNX graph the Rust tool runs. Nothing in
this directory is part of the build, and every artefact records what produced it.

## Setup

```sh
python -m venv .venv
.venv/Scripts/activate          # or .venv/bin/activate
pip install -r requirements.lock
```

`requirements.txt` holds the bounds, and `requirements.lock` what they resolve to. After changing
`requirements.txt`, run the command in the lock file's header from the repository root. Add
`--upgrade` to take every pin to its newest.

With uv, run `uv pip sync training/requirements.lock` from the root. The root `uv.toml` lets uv
find the CUDA build of torch on its second index.

Training wants a GPU. It runs on a CPU, slowly.

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

`--context` is what ships: the model reads a claim inside a window of its review centred on it.

## Learning from unlabelled reviews

- `steamgauge export-pool <training app ids>` draws reviews no reference set holds, from training
  games only.
- `teach.py --runs <saved run ids>` reads that pool with an ensemble of saved runs and writes
  their averaged distributions.
- `train.py --pool data/pool.jsonl --pool-targets data/pool-targets.npz` reads one pool batch
  beside every labelled batch.
- `tapt.py --pool data/tapt.jsonl --run-id tapt-e5` continues the encoder's masked-language
  pretraining. `train.py --backbone runs/tapt-e5/backbone` fine-tunes from it.

The pool never reaches a held-out or validation game, and `train.py` drops any pool row from a
game the run is scored on.

## Other options

`--ema`, `--llrd`, `--rdrop` and `--pooling last` are further options, and the run index says what
each scored. Two aim at the abstention rule:

- `--second-weight` scores against both labellers' answers where a set was read twice, mixed by
  the weight given.
- `--error-reg` charges the subject head for being surer of a wrong claim in the batch than a
  right one, the pairwise hinge of Xin et al. (ACL 2021).

## Averaged weights

`soup.py runs/a runs/b runs/c --run-id <id> --save` averages the weights of saved runs and reads
the average. It refuses ingredients that differ on the encoder, the subjects, the window or the
export. `--greedy` adds ingredients best first and keeps one only while the soup answers more.

## Publishing

- `claims.jsonl` holds review text and is never committed. What is published is the model and a
  label set of review ids, claim offsets and labels, built from `reference/claims/`.
- `publish.py` refuses a row that carries text, and gold answers from a handle
  `reference/people.json` does not name.
- Publishing pins the three model files by hash in `crates/steamgauge-core/src/reader.rs`.
  Rebuild and commit after it runs.
- The search models and the writer are Qwen's, exported by `export_search.py` and
  `uv run export_writer.py --to <model cache>`. `publish.py` puts up only the bytes the tool pins.
- `--only <repository>` publishes one repository alone, so a new one can start at `v1`.

## Pinned backbones

`backbones.py` names every backbone with the commit it is read at and whether its own code may
run. Every script loads through it and refuses a name it does not list. A directory, such as a
`tapt.py` backbone, loads from disk. To try a new backbone, add its line first:

- the full commit, from `huggingface_hub.HfApi().model_info(name).sha`
- `remote_code=True` only if `config.json` at that commit has an `auto_map`
- the commit of the repository that code lives in, when it is not the model's own

## What is recorded

`runs/README.md` indexes every run with its encoder, labels and scores, and marks the shipped one.
`python sweep.py --index` writes it, and nobody edits it by hand.

Every run writes `runs/<id>/`, the only evidence a published figure has:

- the config, the git sha and a hash of the label set
- which games were trained on, validated on and held out
- the metrics per subject and per language, and the whole risk-coverage curve
- the held-out games' figures at the threshold validation chose

## What holds

- **The encoders.** `multilingual-e5-large-instruct` for the standard reader and
  `multilingual-e5-small` for the small one.
- **The teacher.** Both learn from `Qwen3-Embedding-4B` trained through rank-16 adapters, whose
  answers over the unlabelled pool are the distillation targets.
- **The abstention lines** per subject and language are fitted on five folds of the games outside
  the held-out set. `export.py --lines-from` writes them into `reader.json`.
- **Out-of-game answers.** Every training claim is answered by a model that never saw its game.
- **`export-training` runs right before training.** It holds back labels at spans this build no
  longer cuts, and teaching rows from games outside training.
- **Duplicate claims.** A second copy of a claim becomes that claim's second answer.
- **`--card-share`** rests after each batch, and `--card-memory` holds the run to a size.
- **`--cuda-graphs`** replays the passes so the card does not wait on Python.

## How a change is judged

- **The held-out games** are read once, at the end, and choose nothing. The bake-off never reads
  them.
- **A configuration is better** only when it clears the seed spread `sweep.py` prints on coverage
  at the promised accuracy. That spread is about two points.
- **No loss elsewhere.** It also loses no point of macro F1 and does not worsen AURC.
- **Five seeds decide**, in comparison runs of six epochs. By then every seed is within half a
  point of its last epoch.
- **Quote `test.at_validation_threshold`**, the held-out games at the line validation chose. Never
  quote `test.threshold_coverage`, which refits the line on the held-out games.
