"""Every model this project fetches from the Hub, and the commit each one is read at.

A repository name on the Hub is not a model, it is whatever its owner last pushed. Loaded by
name, a backbone's weights and tokenizer can change under a run, and where the repository ships
its own modelling code, `trust_remote_code` runs whatever Python it holds that day on this
machine, on any version of transformers. So nothing is loaded by name alone: each backbone is
read at the full commit it was trained on, remote code is allowed only where the configuration at
that commit names code of its own, and that code is pinned too. A name missing from the table is
refused rather than fetched.

A directory on disk (a run's tokenizer, a backbone `tapt.py` wrote, a test's fixture) is this
project's own output and loads from where it is, with remote code off: a configuration saved
from a remote-code model still names its code by repository, and following that name would fetch
it unpinned.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from pathlib import Path
from typing import Any

COMMIT = re.compile(r"[0-9a-f]{40}")


@dataclass(frozen=True)
class Backbone:
    revision: str
    # True only where the configuration at `revision` carries an `auto_map`: the architecture is
    # not one transformers ships, and the repository's own Python is what builds it.
    remote_code: bool = False
    # Set when that Python lives in a different repository. transformers fetches another
    # repository's code at its main branch unless given a commit for it, whatever `revision`
    # says, so pinning the backbone alone would leave its code floating.
    code_repository: str | None = None
    code_revision: str | None = None


# Each revision is the snapshot this machine trained on. Where a repository's main holds only a
# pickled `pytorch_model.bin`, transformers read the weights from the safetensors conversion the
# Hub's bot opened as a pull request, and the pin is that commit: its tree is the main commit
# named beside it plus `model.safetensors`, so it is exactly what was trained, and it keeps the
# weights in a format that cannot carry code.
BACKBONES: dict[str, Backbone] = {
    # The reader that ships and its sizes.
    "intfloat/multilingual-e5-large-instruct": Backbone("274baa43b0e13e37fafa6428dbc7938e62e5c439"),
    "intfloat/multilingual-e5-large": Backbone("3d7cfbdacd47fdda877c5cd8a79fbcc4f2a574f3"),
    "intfloat/multilingual-e5-base": Backbone("d128750597153bb5987e10b1c3493a34e5a4502a"),
    "intfloat/multilingual-e5-small": Backbone("614241f622f53c4eeff9890bdc4f31cfecc418b3"),
    # The teacher, taught through adapters.
    "Qwen/Qwen3-Embedding-4B": Backbone("5cf2132abc99cad020ac570b19d031efec650f2b"),
    # Measured and refused.
    "xlm-roberta-base": Backbone("e73636d4f797dec63c3081bb6ed5c7b0bb3f2089"),
    "FacebookAI/xlm-roberta-large": Backbone("c23d21b0620b635a76227c604d44e43a9f0ee389"),
    # Safetensors conversion of main a0484667b22365f84929a935b5e50a51f71f159d.
    "microsoft/mdeberta-v3-base": Backbone("faf660b811036d7b6dfc53aacfe5281447d4e2c6"),
    "Alibaba-NLP/gte-multilingual-base": Backbone(
        "9bbca17d9273fd0d03d5725c7a4b0f6b45142062",
        remote_code=True,
        code_repository="Alibaba-NLP/new-impl",
        code_revision="40ced75c3017eb27626c9d4ea981bde21a2662f4",
    ),
    # Safetensors conversion of main 5617a9f61b028005a4858fdac845db406aefb181.
    "BAAI/bge-m3": Backbone("9a0624b896d81da7492a910ffa53731274b6cf3d"),
    "BAAI/bge-reranker-v2-m3": Backbone("953dc6f6f85a1b2dbfca4c34a2796e7dde08d41e"),
    # Safetensors conversion of main c5955035435e2bf121cde7f3c8863ef52ff35d82.
    "jhu-clsp/mmBERT-base": Backbone("ca5b84ba39fb0531402e727a3d9a50626d9f522c"),
    # Safetensors conversion of main abc32620dd4f6ab06f5fbe905dc25f310618e09f.
    "jhu-clsp/mmBERT-small": Backbone("461475a70b192efcbb760df5541d3825b07c4d5b"),
    "ibm-granite/granite-embedding-311m-multilingual-r2": Backbone(
        "44399559930365213510b1ee2eb15ded83374f0e"
    ),
    "nomic-ai/nomic-embed-text-v2-moe": Backbone(
        "1066b6599d099fbb93dfcb64f9c37a7c9e503e85",
        remote_code=True,
        code_repository="nomic-ai/nomic-bert-2048",
        code_revision="7710840340a098cfb869c4f65e87cf2b1b70caca",
    ),
    "EuroBERT/EuroBERT-610m": Backbone(
        "d9af784ed20db6c2096e335ec6a67dd4a219924c", remote_code=True
    ),
    "Qwen/Qwen3-Embedding-0.6B": Backbone("97b0c614be4d77ee51c0cef4e5f07c00f9eb65b3"),
    "microsoft/harrier-oss-v1-0.6b": Backbone("f9b9dc8d367d443f2479d27aa5d8d2850c0774ee"),
}

# What `load` decides and a caller may not.
DECIDED = frozenset({"revision", "trust_remote_code", "code_revision", "local_files_only"})


class UnpinnedBackbone(ValueError):
    pass


def pinning(name: str | Path) -> dict[str, Any]:
    """The arguments `from_pretrained` is given for this name, or a refusal."""
    if Path(name).is_dir():
        return {"trust_remote_code": False, "local_files_only": True}
    backbone = BACKBONES.get(str(name))
    if backbone is None:
        raise UnpinnedBackbone(
            f"{name!r} is neither a pinned backbone nor a directory. To use a model from the "
            "Hub, add it to BACKBONES in training/backbones.py with the full commit it is to be "
            "read at (huggingface_hub.HfApi().model_info(name).sha, or the snapshot under "
            "~/.cache/huggingface/hub it was trained on), and remote_code=True only if "
            "config.json at that commit has an auto_map, naming the repository and commit of "
            "the code it points to when that is another repository."
        )
    arguments: dict[str, Any] = {
        "revision": backbone.revision,
        "trust_remote_code": backbone.remote_code,
    }
    if backbone.code_revision is not None:
        arguments["code_revision"] = backbone.code_revision
    return arguments


def load(loader, name: str | Path, **options):
    """`loader.from_pretrained(name, ...)` at the pinned commit, or from the directory named.

    `loader` is any transformers class with `from_pretrained`: AutoTokenizer, AutoModel,
    AutoModelForMaskedLM."""
    overridden = DECIDED & options.keys()
    if overridden:
        raise UnpinnedBackbone(
            f"{', '.join(sorted(overridden))} for {name!r} comes from training/backbones.py, "
            "not from the caller"
        )
    loaded = loader.from_pretrained(name, **pinning(name), **options)
    # A tokenizer of a class transformers ships keeps every keyword it was handed and writes it
    # into the `tokenizer_config.json` a run saves; which commit the model's code came from is
    # not a property of the tokenizer.
    if isinstance(getattr(loaded, "init_kwargs", None), dict):
        loaded.init_kwargs.pop("code_revision", None)
    return loaded


def shape(name: str | Path, **options):
    """The model `AutoModel` loads for this name, at the pinned commit, without its weights.

    For a caller about to hand it trained weights with `load_state_dict(..., assign=True)`: read
    in full, the pretrained weights were eight gigabytes for the 4B loaded only to be
    overwritten. The weights are left empty and the buffers are built for real, because a module
    computes some for itself and a checkpoint does not carry those.
    """
    from accelerate import init_empty_weights
    from transformers import AutoConfig, AutoModel

    overridden = DECIDED & options.keys()
    if overridden:
        raise UnpinnedBackbone(
            f"{', '.join(sorted(overridden))} for {name!r} comes from training/backbones.py, "
            "not from the caller"
        )
    # A backbone that runs its own code is built by `load`, weights and all: building it from a
    # config fetches its code by the name the config gives, and not at the commit pinned here.
    if pinning(name)["trust_remote_code"]:
        return load(AutoModel, name, **options)
    config = load(AutoConfig, name)
    with init_empty_weights(include_buffers=False):
        return AutoModel.from_config(config, **options)
