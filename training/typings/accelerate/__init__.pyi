from contextlib import AbstractContextManager
from typing import Any

def init_empty_weights(
    include_buffers: bool | None = None,
) -> AbstractContextManager[Any]: ...
