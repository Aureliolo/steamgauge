from collections.abc import Sequence
from typing import Any

import numpy as np
import numpy.typing as npt

class LogisticRegression:
    classes_: npt.NDArray[np.int_]
    def __init__(
        self,
        *,
        C: float = 1.0,
        max_iter: int = 100,
        class_weight: str | dict[int, float] | None = None,
    ) -> None: ...
    def fit(self, X: Any, y: Sequence[int]) -> LogisticRegression: ...
    def predict_proba(self, X: Any) -> npt.NDArray[np.float64]: ...
