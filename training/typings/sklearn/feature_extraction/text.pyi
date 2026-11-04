from collections.abc import Iterable
from typing import Any

class TfidfVectorizer:
    def __init__(
        self,
        *,
        analyzer: str = "word",
        ngram_range: tuple[int, int] = (1, 1),
        min_df: float = 1,
        sublinear_tf: bool = False,
    ) -> None: ...
    def fit_transform(self, raw_documents: Iterable[str]) -> Any: ...
    def transform(self, raw_documents: Iterable[str]) -> Any: ...
