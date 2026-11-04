from collections.abc import Mapping, Sequence
from typing import Any

import numpy as np
import numpy.typing as npt

class NodeArg:
    name: str
    shape: list[int | str | None]
    type: str

class InferenceSession:
    def __init__(
        self,
        path_or_bytes: str | bytes,
        sess_options: Any = None,
        providers: Sequence[str] | None = None,
        provider_options: Sequence[dict[str, Any]] | None = None,
    ) -> None: ...
    def get_inputs(self) -> list[NodeArg]: ...
    def get_outputs(self) -> list[NodeArg]: ...
    def get_providers(self) -> list[str]: ...
    def run(
        self,
        output_names: Sequence[str] | None,
        input_feed: Mapping[str, npt.NDArray[np.generic]],
        run_options: Any = None,
    ) -> list[npt.NDArray[Any]]: ...

def get_available_providers() -> list[str]: ...
