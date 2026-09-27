"""What may be fetched from the Hub, at which commit, and with whose code.

python -m pytest test_backbones.py
"""

from __future__ import annotations

import ast
from pathlib import Path

import pytest

import backbones
from backbones import BACKBONES, COMMIT, UnpinnedBackbone, load, pinning

HERE = Path(__file__).resolve().parent


class Recorder:
    """Stands in for a transformers class: answers with what it was asked for."""

    @classmethod
    def from_pretrained(cls, name, **options):
        loaded = cls()
        loaded.name = name
        loaded.options = options
        loaded.init_kwargs = dict(options)
        return loaded


def test_a_name_missing_from_the_table_is_refused_and_told_how_to_add_it():
    with pytest.raises(UnpinnedBackbone, match="BACKBONES in training/backbones.py"):
        pinning("someone/new-encoder")
    with pytest.raises(UnpinnedBackbone):
        load(Recorder, "someone/new-encoder")


def test_a_path_that_is_not_a_directory_is_refused_rather_than_looked_up(tmp_path):
    with pytest.raises(UnpinnedBackbone):
        load(Recorder, tmp_path / "runs" / "never-trained" / "backbone")


def test_a_directory_loads_from_disk_without_remote_code(tmp_path):
    for name in (tmp_path, str(tmp_path)):
        loaded = load(Recorder, name, torch_dtype=None)
        assert loaded.name == name
        assert loaded.options == {
            "trust_remote_code": False,
            "local_files_only": True,
            "torch_dtype": None,
        }


def test_a_pinned_name_is_read_at_its_commit():
    loaded = load(Recorder, "intfloat/multilingual-e5-large-instruct")
    assert loaded.options == {
        "revision": BACKBONES["intfloat/multilingual-e5-large-instruct"].revision,
        "trust_remote_code": False,
    }


def test_code_in_another_repository_is_pinned_as_well():
    assert pinning("Alibaba-NLP/gte-multilingual-base") == {
        "revision": "9bbca17d9273fd0d03d5725c7a4b0f6b45142062",
        "trust_remote_code": True,
        "code_revision": "40ced75c3017eb27626c9d4ea981bde21a2662f4",
    }


@pytest.mark.parametrize(
    "option", ["revision", "trust_remote_code", "code_revision", "local_files_only"]
)
def test_a_caller_cannot_choose_the_commit_or_the_code(option, tmp_path):
    for name in ("xlm-roberta-base", tmp_path):
        with pytest.raises(UnpinnedBackbone, match=option):
            load(Recorder, name, **{option: "main"})


def test_the_code_commit_is_not_written_into_a_saved_tokenizer():
    loaded = load(Recorder, "nomic-ai/nomic-embed-text-v2-moe")
    assert "code_revision" in loaded.options
    assert "code_revision" not in loaded.init_kwargs


def test_every_revision_is_a_full_commit():
    for name, backbone in BACKBONES.items():
        assert COMMIT.fullmatch(backbone.revision), name
        if backbone.code_revision is not None:
            assert COMMIT.fullmatch(backbone.code_revision), name


def test_remote_code_is_off_unless_asked_for():
    assert backbones.Backbone("0" * 40).remote_code is False


def test_remote_code_is_on_only_for_the_architectures_transformers_does_not_ship():
    # Each of these has an auto_map in its config.json at the pinned commit. Adding a name here
    # is a decision to run that repository's Python, and belongs in the same change as the pin.
    assert {name for name, backbone in BACKBONES.items() if backbone.remote_code} == {
        "Alibaba-NLP/gte-multilingual-base",
        "nomic-ai/nomic-embed-text-v2-moe",
        "EuroBERT/EuroBERT-610m",
    }


def test_code_elsewhere_names_both_its_repository_and_its_commit():
    for name, backbone in BACKBONES.items():
        assert (backbone.code_repository is None) == (backbone.code_revision is None), name
        if backbone.code_repository is not None:
            assert backbone.remote_code, name


def calls_in(path: Path):
    for node in ast.walk(ast.parse(path.read_text(encoding="utf-8"), filename=str(path))):
        if isinstance(node, ast.Call):
            yield node


def called_name(call: ast.Call) -> str | None:
    if isinstance(call.func, ast.Attribute):
        return call.func.attr
    if isinstance(call.func, ast.Name):
        return call.func.id
    return None


SOURCES = sorted(path for path in HERE.glob("*.py") if path.name != "backbones.py")


@pytest.mark.parametrize("path", SOURCES, ids=lambda path: path.name)
def test_nothing_is_fetched_except_through_the_table(path):
    stray = [
        f"{path.name}:{call.lineno} {called_name(call)}"
        for call in calls_in(path)
        if called_name(call) in {"from_pretrained", "snapshot_download", "hf_hub_download"}
        or any(keyword.arg in backbones.DECIDED for keyword in call.keywords)
    ]
    assert not stray, "load through backbones.load instead: " + ", ".join(stray)


def test_the_check_above_sees_every_loader():
    names = {called_name(call) for path in SOURCES for call in calls_in(path)}
    assert "load" in names
    assert not {"from_pretrained", "snapshot_download", "hf_hub_download"} & names
