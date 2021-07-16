from __future__ import annotations
from typing import List

class AcceptLanguage:
    def __init__(self, language_code: str, quality: float):
        self.__language_code = language_code
        self.__quality = quality

    @property
    def language_code(self):
        return self.__language_code

    @property
    def quality(self):
        return self.__quality

    @classmethod
    def parse(cls, accept_language_header: str) -> List[AcceptLanguage]:
        """
        Parses a accept language header.
        """
        accepted_languages = list(map(lambda language_value: cls.from_language_value(language_value), accept_language_header.split(",")))
        # Sort by descending quality
        accepted_languages.sort(key = lambda accept_language: accept_language.quality, reverse=True)
        return accepted_languages

    @classmethod
    def from_language_value(cls, language_value: str) -> AcceptLanguage:
        """
        Creates AcceptLanguage from single language value, e.g.:
        * de
        * *
        * de-DE;q=0.9
        """
        # Split by semicolon
        values = language_value.split(";")
        if len(values) == 2:
            return cls(
                values[0],
                float(values[1][2:])
            )
        else:
            # If quality value is not available set to highest
            return cls(
                values[0],
                1
            )

